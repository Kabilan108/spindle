//! A stand-in for real audio analysis.
//!
//! The real pipeline would tap decoded PCM from librespot's sink, run a 2048-point FFT every
//! 1024 samples (about 43 hops a second at 44.1 kHz), sum the bins into log-spaced bands and hand
//! those band powers to [`Analyzer`]. Here [`SimTrack`] synthesises the band powers of a
//! believable 120 BPM track instead (kick, bass, pads, hats, snare, a riser into a drop, a lead),
//! and everything downstream of the FFT is the code a real implementation would run.

pub const BANDS: usize = 48;
pub const F_LO: f32 = 40.0;
pub const F_HI: f32 = 16_000.0;
/// Seconds per analysis hop: 1024 samples at 44.1 kHz.
pub const HOP: f64 = 1024.0 / 44_100.0;
/// Display range of the band levels, in dB below full scale.
const RANGE_DB: f32 = 48.0;

/// What the UI gets each hop. Every field is derivable from decoded PCM.
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
        self.loudness == 0.0 && self.bands.iter().all(|b| *b == 0.0)
    }
}

/// Band powers for one hop: what the FFT stage hands the analyzer. A real source reads PCM.
pub trait BandSource {
    /// Linear band powers (1.0 = full scale) for the hop ending at `position` seconds.
    fn bands_at(&self, position: f64, out: &mut [f32; BANDS]);
    fn duration(&self) -> f64;
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
        for i in 0..BANDS {
            self.level[i] = follow(self.level[i], target[i], dt, 0.012, 0.16);
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
            for i in 0..BANDS {
                let f = band_freq(i);
                if f >= lo && f < hi {
                    sum += target[i];
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

/// Stable integer hash to 0..1, so the track is identical every run.
fn hash(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B).rotate_left(13);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x & 0x00FF_FFFF) as f32 / 16_777_216.0
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Intro,
    Verse,
    Build,
    Drop,
    Breakdown,
    Outro,
}

/// A synthetic 4:21 track at 120 BPM: one bar is two seconds.
pub struct SimTrack {
    freqs: [f32; BANDS],
}

impl Default for SimTrack {
    fn default() -> Self {
        SimTrack { freqs: std::array::from_fn(band_freq) }
    }
}

const BEAT: f64 = 0.5;
const BAR: f64 = 2.0;
const ROOTS: [f32; 4] = [55.0, 43.65, 65.41, 49.0];
const CHORDS: [[f32; 3]; 4] = [[220.0, 261.6, 329.6], [174.6, 220.0, 261.6], [261.6, 329.6, 392.0], [196.0, 246.9, 293.7]];
const LEAD: [f32; 6] = [440.0, 523.3, 587.3, 659.3, 784.0, 880.0];

impl SimTrack {
    pub fn section(pos: f64) -> Section {
        match pos {
            p if p < 16.0 => Section::Intro,
            p if p < 52.0 => Section::Verse,
            p if p < 56.0 => Section::Build,
            p if p < 88.0 => Section::Drop,
            p if p < 104.0 => Section::Breakdown,
            p if p < 136.0 => Section::Verse,
            p if p < 140.0 => Section::Build,
            p if p < 204.0 => Section::Drop,
            p if p < 236.0 => Section::Breakdown,
            _ => Section::Outro,
        }
    }

    fn bump(&self, p: &mut [f32; BANDS], fc: f32, sigma: f32, amp: f32) {
        if amp <= 0.0 || fc <= 0.0 {
            return;
        }
        for (i, f) in self.freqs.iter().enumerate() {
            let d = (f / fc).log2() / sigma;
            if d.abs() < 4.0 {
                p[i] += amp * (-0.5 * d * d).exp();
            }
        }
    }

    fn tone(&self, p: &mut [f32; BANDS], f0: f32, amp: f32, harmonics: u32, roll: f32) {
        for h in 1..=harmonics {
            self.bump(p, f0 * h as f32, 0.16, amp * roll.powi(h as i32 - 1));
        }
    }

    fn noise(&self, p: &mut [f32; BANDS], lo: f32, hi: f32, amp: f32) {
        if amp <= 0.0 {
            return;
        }
        for (i, f) in self.freqs.iter().enumerate() {
            let below = ((f / lo).log2() * 3.0).clamp(-12.0, 0.0);
            let above = ((hi / f).log2() * 3.0).clamp(-12.0, 0.0);
            p[i] += amp * (below + above).exp2() * (0.75 + 0.5 * hash(i as u32, (f.to_bits() >> 8) ^ 77));
        }
    }
}

impl BandSource for SimTrack {
    fn duration(&self) -> f64 {
        261.0
    }

    fn bands_at(&self, pos: f64, p: &mut [f32; BANDS]) {
        *p = [0.0; BANDS];
        let section = Self::section(pos);
        let bar = (pos / BAR).floor() as usize;
        let chord = bar % 4;
        let in_beat = pos.rem_euclid(BEAT) as f32;
        let beat = (pos / BEAT).floor() as u32;
        let sixteenth = (pos / (BEAT / 4.0)).floor() as u32;
        let in_16 = pos.rem_euclid(BEAT / 4.0) as f32;
        let eighth = (pos / (BEAT / 2.0)).floor() as u32;
        let in_8 = pos.rem_euclid(BEAT / 2.0) as f32;
        let hop = (pos / HOP).floor() as u32;

        let (drums, bass, pad, hats, lead, level) = match section {
            Section::Intro => (0.0, 0.0, 0.55, 0.35, 0.0, 0.55),
            Section::Verse => (0.8, 0.75, 0.6, 0.7, 0.0, 0.75),
            Section::Build => (0.0, 0.35, 0.7, 0.5, 0.0, 0.8),
            Section::Drop => (1.0, 1.0, 0.45, 1.0, 1.0, 1.0),
            Section::Breakdown => (0.0, 0.0, 0.8, 0.2, 0.25, 0.6),
            Section::Outro => {
                let fade = (1.0 - (pos - 236.0) / 25.0).clamp(0.0, 1.0) as f32;
                (0.7 * fade, 0.6 * fade, 0.6 * fade, 0.5 * fade, 0.0, 0.7 * fade)
            }
        };

        let kick = drums * (-in_beat / 0.11).exp();
        if drums > 0.0 {
            let sweep = 48.0 + 110.0 * (-in_beat / 0.025).exp();
            self.bump(p, sweep, 0.35, 1.1 * kick);
            self.noise(p, 2500.0, 6000.0, 0.012 * drums * (-in_beat / 0.012).exp());
            if beat % 2 == 1 {
                let snare = drums * (-in_beat / 0.09).exp();
                self.bump(p, 200.0, 0.4, 0.12 * snare);
                self.noise(p, 1200.0, 9000.0, 0.05 * snare);
            }
        }
        let duck = 1.0 - 0.75 * kick;
        if bass > 0.0 {
            let env = 0.55 + 0.45 * (-in_8 / 0.1).exp();
            self.tone(p, ROOTS[chord], 0.12 * bass * env * duck, 5, 0.4);
        }
        if pad > 0.0 {
            let swell = 0.75 + 0.25 * ((pos * 0.9).sin() as f32);
            for (k, f) in CHORDS[chord].iter().enumerate() {
                self.tone(p, *f, 0.005 * pad * swell * (1.0 - 0.3 * kick) / (1.0 + k as f32 * 0.3), 6, 0.55);
            }
        }
        if hats > 0.0 {
            let open = section == Section::Drop && sixteenth % 4 == 2;
            let vel = [0.55, 0.25, 0.85, 0.3][(sixteenth % 4) as usize] * (0.8 + 0.4 * hash(sixteenth, 3));
            let env = (-in_16 / if open { 0.1 } else { 0.028 }).exp();
            self.noise(p, 6500.0, 16_000.0, 0.012 * hats * vel * env);
        }
        if section == Section::Build {
            let k = ((pos - if pos < 100.0 { 52.0 } else { 136.0 }) / 4.0).clamp(0.0, 1.0) as f32;
            let rate = if k < 0.5 { BEAT / 2.0 } else if k < 0.85 { BEAT / 4.0 } else { BEAT / 8.0 };
            let in_roll = pos.rem_euclid(rate) as f32;
            let snare = (0.35 + 0.65 * k) * (-in_roll / 0.05).exp();
            self.bump(p, 210.0, 0.4, 0.1 * snare);
            self.noise(p, 1500.0, 9000.0, 0.045 * snare);
            let centre = 600.0 * (14.0f32).powf(k);
            self.bump(p, centre, 0.7, 0.03 * k * k);
            self.bump(p, centre * 2.0, 0.5, 0.012 * k * k);
            if k > 0.94 {
                *p = p.map(|v| v * 0.08);
            }
        }
        if lead > 0.0 {
            let step = (eighth % 16) as u32;
            let note = LEAD[(hash(step, (bar % 2) as u32) * LEAD.len() as f32) as usize % LEAD.len()];
            let gate = if hash(step, 91) > 0.3 { 1.0 } else { 0.0 };
            let env = gate * (0.35 + 0.65 * (-in_8 / 0.18).exp());
            self.tone(p, note, 0.02 * lead * env, 8, 0.62);
        }
        for (i, v) in p.iter_mut().enumerate() {
            *v *= level * (0.85 + 0.3 * hash(i as u32, hop));
        }
    }
}

/// Track playback plus the analysis that follows it, stepped once per hop.
pub struct Feed<S: BandSource> {
    pub source: S,
    pub analyzer: Analyzer,
    pub frame: AudioFrame,
    clock: f64,
    scratch: [f32; BANDS],
}

impl<S: BandSource> Feed<S> {
    pub fn new(source: S) -> Self {
        Feed { source, analyzer: Analyzer::default(), frame: AudioFrame::SILENT, clock: 0.0, scratch: [0.0; BANDS] }
    }

    /// Advance the analysis clock to `t`, hop by hop. `position(t)` is the track position at clock
    /// `t`, or `None` while paused. Returns how many hops ran (0 means the frame is unchanged).
    pub fn advance(&mut self, t: f64, position: impl Fn(f64) -> Option<f64>) -> usize {
        if t - self.clock > 3.0 {
            self.clock = t - 3.0;
        }
        let mut hops = 0;
        while self.clock + HOP <= t {
            self.clock += HOP;
            let power = position(self.clock).map(|pos| {
                self.source.bands_at(pos, &mut self.scratch);
                self.scratch
            });
            self.frame = self.analyzer.step(power.as_ref(), HOP as f32);
            hops += 1;
        }
        hops
    }
}

/// Loudness per column across the whole track, as the trace concept would record it while
/// playing (and could cache per track). Runs the same analyzer over the track.
pub fn loudness_columns(source: &impl BandSource, columns: usize, until: f64) -> Vec<f32> {
    let mut analyzer = Analyzer::default();
    let mut sums = vec![(0.0f32, 0u32); columns];
    let mut scratch = [0.0; BANDS];
    let per = source.duration() / columns as f64;
    let mut pos = 0.0;
    while pos < until {
        source.bands_at(pos, &mut scratch);
        let f = analyzer.step(Some(&scratch), HOP as f32);
        let c = ((pos / per) as usize).min(columns - 1);
        sums[c].0 += f.loudness;
        sums[c].1 += 1;
        pos += HOP;
    }
    sums.iter().map(|(s, n)| if *n > 0 { s / *n as f32 } else { -1.0 }).collect()
}
