//! Procedural audio: everything is synthesised in real time (no sample files).
//! The game thread only publishes a few parameters; the audio callback renders from them.
use crate::game::{Game, Screen};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Arc;
use winit::keyboard::KeyCode;

/// Parameters shared with the audio thread (floats stored as bits).
#[derive(Default)]
pub struct Shared {
    throttle: AtomicU32,
    volume: AtomicU32,
    alarm: AtomicU32,
    mining: AtomicU32,
    hits: AtomicU32,
    blips: AtomicU32,
}

fn set(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Relaxed);
}
fn get(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

/// The sound generator, independent of any audio device so it can be tested.
pub struct Synth {
    shared: Arc<Shared>,
    rate: f32,
    t: f32,
    phase: [f32; 4],
    lp: f32,
    rng: u32,
    seen_hits: u32,
    seen_blips: u32,
    hit_env: f32,
    blip_env: f32,
    thr: f32,
}

impl Synth {
    pub fn new(shared: Arc<Shared>, rate: f32) -> Synth {
        Synth {
            shared,
            rate,
            t: 0.0,
            phase: [0.0; 4],
            lp: 0.0,
            rng: 0x1234_5678,
            seen_hits: 0,
            seen_blips: 0,
            hit_env: 0.0,
            blip_env: 0.0,
            thr: 0.0,
        }
    }

    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Render one mono sample.
    pub fn sample(&mut self) -> f32 {
        let dt = 1.0 / self.rate;
        let sh = Arc::clone(&self.shared);
        let vol = get(&sh.volume).clamp(0.0, 1.0);
        let target = get(&sh.throttle).clamp(0.0, 1.0);
        self.thr += (target - self.thr) * (dt * 6.0).min(1.0);
        let hits = sh.hits.load(Relaxed);
        if hits != self.seen_hits {
            self.seen_hits = hits;
            self.hit_env = 1.0;
        }
        let blips = sh.blips.load(Relaxed);
        if blips != self.seen_blips {
            self.seen_blips = blips;
            self.blip_env = 1.0;
        }
        let tau = std::f32::consts::TAU;
        for (p, f) in self.phase.iter_mut().zip([
            52.0 + 40.0 * self.thr,
            78.0 + 55.0 * self.thr,
            1250.0,
            118.0,
        ]) {
            *p = (*p + f * dt).fract();
        }
        let n = self.noise();
        self.lp += (n - self.lp) * 0.04;
        // Ambient drone, engine rumble, mining buzz, alarm, impact, UI blip.
        let drone = 0.05 * (self.phase[0] * tau).sin() + 0.03 * (self.phase[1] * 0.5 * tau).sin();
        let engine = self.thr * (0.22 * self.lp * 4.0 + 0.10 * (self.phase[1] * tau).sin());
        let mining = if get(&sh.mining) > 0.5 {
            0.06 * (self.phase[3] * 2.0 - 1.0)
        } else {
            0.0
        };
        let alarm = if get(&sh.alarm) > 0.5 {
            let gate = if (self.t * 3.0).fract() < 0.5 {
                1.0
            } else {
                0.0
            };
            let f = if (self.t * 1.5).fract() < 0.5 {
                880.0
            } else {
                660.0
            };
            0.07 * gate * (self.t * f * tau).sin().signum()
        } else {
            0.0
        };
        let hit = self.hit_env * (0.5 * n + 0.3 * (self.hit_env * 90.0 * tau).sin());
        self.hit_env = (self.hit_env - dt * 3.0).max(0.0);
        let blip = self.blip_env * 0.12 * (self.phase[2] * tau).sin();
        self.blip_env = (self.blip_env - dt * 14.0).max(0.0);
        self.t += dt;
        if self.t > 1000.0 {
            self.t -= 1000.0;
        }
        ((drone + engine + mining + alarm + hit * 0.5 + blip) * vol * 1.5).clamp(-1.0, 1.0)
    }
}

pub struct Audio {
    shared: Arc<Shared>,
    _stream: Option<cpal::Stream>,
    last_hull: f64,
    last_msgs: usize,
    last_screen: Option<Screen>,
}

impl Audio {
    /// Open the default output device; falls back to a silent no-op when none is available.
    pub fn new() -> Audio {
        let shared = Arc::new(Shared::default());
        let stream = Self::open(Arc::clone(&shared));
        Audio {
            shared,
            _stream: stream,
            last_hull: 0.0,
            last_msgs: 0,
            last_screen: None,
        }
    }

    pub fn available(&self) -> bool {
        self._stream.is_some()
    }

    fn open(shared: Arc<Shared>) -> Option<cpal::Stream> {
        let host = cpal::default_host();
        let dev = host.default_output_device()?;
        let cfg = dev.default_output_config().ok()?;
        let format = cfg.sample_format();
        let config: cpal::StreamConfig = cfg.into();
        let stream = match format {
            cpal::SampleFormat::F32 => Self::build::<f32>(&dev, &config, shared),
            cpal::SampleFormat::I16 => Self::build::<i16>(&dev, &config, shared),
            cpal::SampleFormat::U16 => Self::build::<u16>(&dev, &config, shared),
            _ => return None,
        }?;
        stream.play().ok()?;
        Some(stream)
    }

    fn build<T: SizedSample + FromSample<f32>>(
        dev: &cpal::Device,
        config: &cpal::StreamConfig,
        shared: Arc<Shared>,
    ) -> Option<cpal::Stream> {
        let channels = usize::from(config.channels);
        let mut synth = Synth::new(shared, config.sample_rate.0 as f32);
        dev.build_output_stream(
            config,
            move |data: &mut [T], _| {
                for frame in data.chunks_mut(channels) {
                    let v = T::from_sample(synth.sample());
                    for out in frame {
                        *out = v;
                    }
                }
            },
            |_| {},
            None,
        )
        .ok()
    }

    /// Publish the current game state to the audio thread (call once per frame).
    pub fn update(&mut self, g: &Game) {
        let s = &self.shared;
        set(&s.volume, f32::from(g.settings.volume) / 10.0 * 0.8);
        let flying = g.screen == Screen::Flight && g.sim.docked.is_none();
        let thrust = flying
            && g.timescale <= 2.0
            && g.sim.ship.fuel > 0.0
            && (g.keys.contains(&KeyCode::Space) || g.sim.autopilot.is_some());
        set(&s.throttle, if thrust { 1.0 } else { 0.0 });
        set(&s.mining, if g.sim.laser_on { 1.0 } else { 0.0 });
        let hull_frac = g.sim.ship.hull / g.sim.ship.stats.hull_max.max(1.0);
        set(&s.alarm, if flying && hull_frac < 0.3 { 1.0 } else { 0.0 });
        if g.sim.ship.hull < self.last_hull - 0.5 && self.last_hull > 0.0 {
            s.hits.fetch_add(1, Relaxed);
        }
        self.last_hull = g.sim.ship.hull;
        if g.messages.len() != self.last_msgs || self.last_screen != Some(g.screen) {
            if self.last_screen.is_some() {
                s.blips.fetch_add(1, Relaxed);
            }
            self.last_msgs = g.messages.len();
            self.last_screen = Some(g.screen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_is_bounded_silent_when_muted_and_reacts_to_parameters() {
        let shared = Arc::new(Shared::default());
        let mut s = Synth::new(Arc::clone(&shared), 44_100.0);
        // Muted: volume 0 gives exact silence.
        set(&shared.volume, 0.0);
        assert!((0..2000).all(|_| s.sample() == 0.0));
        // Full volume with engine, alarm, mining and a hit stays within range and is audible.
        set(&shared.volume, 1.0);
        set(&shared.throttle, 1.0);
        set(&shared.alarm, 1.0);
        set(&shared.mining, 1.0);
        shared.hits.fetch_add(1, Relaxed);
        let buf: Vec<f32> = (0..44_100).map(|_| s.sample()).collect();
        assert!(buf.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        let rms = (buf.iter().map(|v| v * v).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(rms > 0.02, "rms {rms}");
        // The engine is louder than idle.
        let mut idle = Synth::new(Arc::new(Shared::default()), 44_100.0);
        set(&idle.shared.volume, 1.0);
        let idle_rms = ((0..44_100).map(|_| idle.sample().powi(2)).sum::<f32>() / 44_100.0).sqrt();
        assert!(rms > idle_rms * 2.0, "{rms} vs idle {idle_rms}");
    }
}
