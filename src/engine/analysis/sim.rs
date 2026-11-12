//! A believable 120 BPM track's band powers (kick, bass, pads, hats, snare, a riser into a
//! drop, a lead), for the demo engine: headless scenes and videos show the visualizers moving
//! without a sound card.

use super::analyzer::{BANDS, HOP, band_freq};

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

impl SimTrack {
    pub const DURATION: f64 = 261.0;

    pub fn bands_at(&self, pos: f64, p: &mut [f32; BANDS]) {
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
            let step = eighth % 16;
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

