use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;

use std::sync::Arc;

pub enum ReadResult {
    Ok,
    NotYetStarted,
    Ended,
}

#[derive(Clone)]
pub struct Playback {
    pub start: usize,
    pub repetition_period: usize,
    pub repetition_count: Option<usize>,
    pub samples: Arc<Vec<f32>>,
}

impl Playback {
    pub fn new(samples: Arc<Vec<f32>>) -> Playback {
        Playback {
            start: 0,
            repetition_period: 0,
            repetition_count: None,
            samples,
        }
    }
    pub fn offset(self, offset: usize) -> Self {
        Playback {
            start: offset,
            ..self
        }
    }
    pub fn repeat(self, period: usize, count: Option<usize>) -> Self {
        Playback {
            repetition_period: period,
            repetition_count: count,
            ..self
        }
    }

    pub fn end(&self) -> Option<usize> {
        self.repetition_count.map(|repetition_count| {
            self.start + self.samples.len() + self.repetition_period * repetition_count
        })
    }

    pub fn read(&self, time: usize, buffer: &mut [f32]) -> ReadResult {
        let time_end = time + buffer.len();

        if time_end < self.start {
            return ReadResult::NotYetStarted;
        }
        if matches!(self.end(), Some(end) if time > end) {
            return ReadResult::Ended;
        }

        // Play last repetition
        let mut rep = (time.saturating_sub(self.start)) / self.repetition_period;
        loop {
            let rep_time = self.start + rep * self.repetition_period;
            if rep_time >= time_end {
                break;
            }

            self.read_sample(rep_time as isize - time as isize, buffer);

            rep += 1;
        }

        ReadResult::Ok
    }

    pub fn read_sample(&self, time_offset: isize, output: &mut [f32]) {
        let read_offset = (-time_offset).clamp(0, self.samples.len() as isize) as usize;
        let write_offset = (time_offset).clamp(0, output.len() as isize) as usize;

        let src = &self.samples[read_offset..];
        let dst = &mut output[write_offset..];

        dst.iter_mut().zip(src.iter()).for_each(|(d, s)| *d += *s);
    }
}

enum PlayerCommand {
    AddPlaybacks(Vec<Playback>),
    ClearPlaybacks,
    SetVolume(f32),
}

pub struct Player {
    config: cpal::SupportedStreamConfig,
    send: std::sync::mpsc::Sender<PlayerCommand>,
    _stream: cpal::Stream,
}
impl Player {
    pub fn start() -> anyhow::Result<Player> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(anyhow::anyhow!("No output device available"))?;
        println!("Using output device: {}", device.name()?);

        let config = device.default_output_config()?;
        println!("Using output config: {:?}", config);
        let num_channels = config.channels() as usize;

        let (send, recv) = std::sync::mpsc::channel::<PlayerCommand>();

        let mut playbacks = Vec::<Playback>::new();
        let mut time = 0usize;
        let mut volume = 1f32;

        let mut tmp_buffer = vec![0.0f32; 2 << 14];
        let stream = device.build_output_stream(
            &config.config(),
            move |data: &mut [f32], _info| {
                // Handle commands
                for cmd in recv.try_iter() {
                    match cmd {
                        PlayerCommand::AddPlaybacks(new_playbacks) => {
                            playbacks.extend(new_playbacks.into_iter().map(|p| Playback {
                                start: p.start + time,
                                ..p
                            }));
                        }
                        PlayerCommand::ClearPlaybacks => {
                            playbacks.clear();
                        }
                        PlayerCommand::SetVolume(new_volume) => {
                            volume = new_volume;
                        }
                    }
                }

                // Read playbacks into temporary buffer in mono format
                let mono = &mut tmp_buffer[..(data.len() / num_channels)];
                mono.fill(0.0);
                playbacks.retain(|p| match p.read(time, mono) {
                    ReadResult::Ok => true,
                    ReadResult::NotYetStarted => true,
                    ReadResult::Ended => false,
                });
                for f in mono.iter_mut() {
                    // Volume and clipping
                    *f = (volume * *f).tanh();
                }
                time += mono.len();

                // Convert mono to as many channels as needed
                for ch in 0..num_channels {
                    data.iter_mut()
                        .skip(ch)
                        .step_by(num_channels)
                        .zip(mono.iter())
                        .for_each(|(d, s)| *d = *s);
                }
            },
            |e| eprintln!("an error occurred on the output audio stream: {}", e),
            None,
        )?;

        Ok(Player {
            config,
            _stream: stream,
            send,
        })
    }

    pub fn sample_rate(&self) -> usize {
        self.config.sample_rate().0 as usize
    }

    pub fn add_playbacks(&self, playbacks: Vec<Playback>) {
        self.send
            .send(PlayerCommand::AddPlaybacks(playbacks))
            .unwrap();
    }

    pub fn clear_playbacks(&self) {
        self.send.send(PlayerCommand::ClearPlaybacks).unwrap();
    }

    pub fn set_volume_db(&self, volume_db: f32) {
        self.send
            .send(PlayerCommand::SetVolume(10.0f32.powf(volume_db / 20.0)))
            .unwrap();
    }
}
