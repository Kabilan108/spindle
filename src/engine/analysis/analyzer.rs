//! Smoothing, normalisation, band aggregates and onset detection over band powers: the part
//! of the analysis that doesn't care whether the powers came from an FFT or a simulation.

pub const BANDS: usize = 48;
pub const F_LO: f32 = 40.0;
pub const F_HI: f32 = 16_000.0;
/// Seconds per analysis hop: 1024 samples at 44.1 kHz.
pub const HOP: f64 = 1024.0 / 44_100.0;
/// Display range of the band levels, in dB below full scale.
const RANGE_DB: f32 = 48.0;

/// What the visualizers get each hop.
#[derive(Clone, Copy, Debug)]
pub struct AudioFrame {
    /// Band levels 0..1, low to high, smoothed with a fast attack and a slower release.
    pub bands: [f32; BANDS],
    /// Short-term loudness 0..1.
    pub loudness: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// Onset strength 0..1 on the hop where a low-band onset (a kick) was detected, else 0.
    pub onset: f32,
    /// Seconds since the last onset.
    pub since_onset: f32,
}

impl AudioFrame {
    pub const SILENT: AudioFrame =
        AudioFrame { bands: [0.0; BANDS], loudness: 0.0, bass: 0.0, mid: 0.0, treble: 0.0, onset: 0.0, since_onset: 10.0 };

    pub fn is_silent(&self) -> bool {
        self.loudness == 0.0 && self.bass == 0.0 && self.mid == 0.0 && self.treble == 0.0 && self.bands.iter().all(|b| *b == 0.0)
    }
}

pub fn band_freq(i: usize) -> f32 {
    F_LO * (F_HI / F_LO).powf(i as f32 / (BANDS - 1) as f32)
}

/// Smoothing, normalisation, band aggregates and onset detection over band powers.
pub struct Analyzer {
    level: [f32; BANDS],
    prev_db: [f32; BANDS],
    flux_hist: [f32; 24],
    flux_at: usize,
    since_onset: f32,
    loud: f32,
    bass: f32,
    mid: f32,
    treble: f32,
}

impl Default for Analyzer {
    fn default() -> Self {
        Analyzer {
            level: [0.0; BANDS],
            prev_db: [-RANGE_DB; BANDS],
            flux_hist: [0.0; 24],
            flux_at: 0,
            since_onset: 10.0,
            loud: 0.0,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
        }
    }
}

fn follow(v: f32, target: f32, dt: f32, attack: f32, release: f32) -> f32 {
    let tau = if target > v { attack } else { release };
    let next = v + (target - v) * (1.0 - (-dt / tau).exp());
    if target == 0.0 && next < 1e-2 { 0.0 } else { next }
}

impl Analyzer {
    /// One hop. `power` is `None` while paused or stopped: levels fall back to silence.
    pub fn step(&mut self, power: Option<&[f32; BANDS]>, dt: f32) -> AudioFrame {
        let mut target = [0.0f32; BANDS];
        let mut flux = 0.0;
        if let Some(power) = power {
            for i in 0..BANDS {
                let db = (10.0 * (power[i] + 1e-9).log10()).max(-RANGE_DB);
                target[i] = ((db + RANGE_DB) / RANGE_DB).clamp(0.0, 1.0).powf(1.8);
                if band_freq(i) < 120.0 {
                    flux += (db - self.prev_db[i]).max(0.0);
                }
                self.prev_db[i] = db;
            }
        } else {
            self.prev_db = [-RANGE_DB; BANDS];
        }
        for (level, target) in self.level.iter_mut().zip(target) {
            *level = follow(*level, target, dt, 0.012, 0.16);
        }
        let mean = self.flux_hist.iter().sum::<f32>() / self.flux_hist.len() as f32;
        self.flux_hist[self.flux_at] = flux;
        self.flux_at = (self.flux_at + 1) % self.flux_hist.len();
        self.since_onset += dt;
        let mut onset = 0.0;
        if flux > mean * 1.5 + 18.0 && self.since_onset > 0.2 {
            onset = (flux / 70.0).clamp(0.2, 1.0);
            self.since_onset = 0.0;
        }
        let avg = |lo: f32, hi: f32| {
            let (mut sum, mut n) = (0.0, 0.0);
            for (i, level) in target.iter().enumerate() {
                let f = band_freq(i);
                if f >= lo && f < hi {
                    sum += level;
                    n += 1.0;
                }
            }
            if n > 0.0 { sum / n } else { 0.0 }
        };
        let shape = |v: f32| (v / 0.7).clamp(0.0, 1.0);
        let rms = (target.iter().map(|v| v * v).sum::<f32>() / BANDS as f32).sqrt();
        self.loud = follow(self.loud, shape(rms * 1.15), dt, 0.05, 0.35);
        self.bass = follow(self.bass, shape(avg(F_LO, 200.0)), dt, 0.015, 0.18);
        self.mid = follow(self.mid, shape(avg(200.0, 3000.0)), dt, 0.03, 0.22);
        self.treble = follow(self.treble, shape(avg(3000.0, F_HI)), dt, 0.01, 0.12);
        AudioFrame {
            bands: self.level,
            loudness: self.loud,
            bass: self.bass,
            mid: self.mid,
            treble: self.treble,
            onset,
            since_onset: self.since_onset,
        }
    }
}

