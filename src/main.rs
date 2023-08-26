use std::{
    f64::consts::TAU,
    sync::Arc,
    time::{Duration, Instant},
};

use player::Playback;

mod player;

fn main() -> anyhow::Result<()> {
    let player = player::Player::start()?;

    let hi_click = Arc::new(generate_click(
        player.sample_rate(),
        Duration::from_millis(100),
        880.0,
        1.0,
    ));
    let mid_click = Arc::new(generate_click(
        player.sample_rate(),
        Duration::from_millis(100),
        659.25,
        1.0,
    ));
    let lo_click = Arc::new(generate_click(
        player.sample_rate(),
        Duration::from_millis(100),
        440.0,
        1.0,
    ));

    let mut bpm = 120.0;
    let mut numerator = 4;
    let mut subdivision = 4;
    let mut tap_tempo = TapTempo::new();
    let mut volume_db = 0.0;

    let mut last_state = (bpm * 2.0, numerator, subdivision);

    eframe::run_simple_native("metronome", Default::default(), move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                for (_, x) in ui.style_mut().text_styles.iter_mut() {
                    x.size *= 4.0;
                }

                ui.add(
                    egui::DragValue::new(&mut bpm)
                        .clamp_range(30.0..=400.0)
                        .suffix(" BPM"),
                );
                if ui.button("Tap Tempo").clicked() {
                    if let Some(tapped_bpm) = tap_tempo.tap() {
                        bpm = tapped_bpm;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(20.0);
                    ui.add(egui::DragValue::new(&mut numerator).clamp_range(0..=32));
                    ui.menu_button(subdivision.to_string(), |ui| {
                        for i in [4, 8, 16, 32] {
                            if ui.button(i.to_string()).clicked() {
                                subdivision = i;
                                ui.close_menu();
                            }
                        }
                    });
                });
            });

            if ui
                .add(
                    egui::DragValue::new(&mut volume_db)
                        .clamp_range(-36.0..=36.0)
                        .suffix("db"),
                )
                .changed()
            {
                player.set_volume_db(volume_db);
            }

            let new_state = (bpm, numerator, subdivision);
            if last_state != new_state {
                last_state = new_state;

                let subdiv_duration = ((player.sample_rate() as f32 * 60.0 * 4.0)
                    / bpm
                    / subdivision as f32) as usize;
                let bar_duration = subdiv_duration * numerator;

                player.clear_playbacks();
                player.add_playbacks(
                    (0..numerator)
                        .map(|i| {
                            let sample = if i == 0 {
                                hi_click.clone()
                            } else if i % 2 == 1 {
                                lo_click.clone()
                            } else {
                                mid_click.clone()
                            };

                            Playback::new(sample)
                                .offset(i * subdiv_duration)
                                .repeat(bar_duration, None)
                        })
                        .collect(),
                );
            }
        });
    })
    .unwrap();

    Ok(())
}

struct TapTempo {
    taps: Vec<f32>,
    last: Instant,
}
impl TapTempo {
    pub fn new() -> TapTempo {
        TapTempo {
            taps: Vec::new(),
            last: Instant::now(),
        }
    }

    pub fn tap(&mut self) -> Option<f32> {
        let now = Instant::now();
        let duration = (now - self.last).as_secs_f32();
        self.last = now;

        if self
            .taps
            .last()
            .map_or(false, |&v| v < duration * 0.5 || v > duration * 2.0)
        {
            self.taps.clear();
            None
        } else {
            self.taps.push(duration);

            let mean = geometric_mean(self.taps.iter().copied());
            Some(60.0 / mean)
        }
    }
}

fn geometric_mean(values: impl Iterator<Item = f32>) -> f32 {
    let mut n = 0;

    values
        .fold(1.0, |a, b| {
            println!("{} {}", a, b);
            n += 1;
            a * b
        })
        .powf(1.0 / n as f32)
}

fn generate_click(sample_rate: usize, duration: Duration, freq: f32, gain: f32) -> Vec<f32> {
    let freq = freq as f64;
    let gain = gain as f64;
    let duration = duration.as_secs_f64();

    let n = (duration * sample_rate as f64) as usize;
    let mut result = Vec::with_capacity(n);

    let minimum_volume = 0.01f64;
    let decay_factor = minimum_volume.powf(1.0 / n as f64);

    let mut envelope = 1.0;
    for i in 0..n {
        let w = (TAU * i as f64) / sample_rate as f64;

        let sine_wave = (w * freq).sin();
        result.push((gain * envelope * sine_wave) as f32);

        envelope *= decay_factor;
    }

    result
}
