//! PCM to band powers: a Hann-windowed 2048-point FFT, its bins summed into the analyzer's
//! log-spaced bands.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use super::analyzer::{BANDS, F_HI, F_LO};

pub const WINDOW: usize = 2048;
pub const RATE: f32 = 44_100.0;
/// Lifts typical music, about 20 dB under the full-scale tones the analyzer's range assumes,
/// into the middle of that range.
const GAIN: f32 = 60.0;

pub struct Spectrum {
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
    input: Vec<f32>,
    output: Vec<Complex32>,
    scratch: Vec<Complex32>,
    /// Each band's bins, as a half-open range; a band narrower than a bin gets its nearest one.
    edges: [(usize, usize); BANDS],
    norm: f32,
}

impl Default for Spectrum {
    fn default() -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(WINDOW);
        let hann: Vec<f32> = (0..WINDOW).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / WINDOW as f32).cos()).collect();
        let window_power: f32 = hann.iter().map(|w| w * w).sum();
        let bin_hz = RATE / WINDOW as f32;
        let ratio = (F_HI / F_LO).powf(1.0 / (BANDS - 1) as f32);
        let edges = std::array::from_fn(|i| {
            let centre = F_LO * ratio.powi(i as i32);
            let (lo, hi) = (centre / ratio.sqrt(), centre * ratio.sqrt());
            let first = (lo / bin_hz).ceil() as usize;
            let last = ((hi / bin_hz).ceil() as usize).min(WINDOW / 2);
            if last > first { (first, last) } else { ((centre / bin_hz).round() as usize, (centre / bin_hz).round() as usize + 1) }
        });
        Self {
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            hann,
            edges,
            norm: 2.0 * GAIN / (WINDOW as f32 * window_power),
        }
    }
}

impl Spectrum {
    /// `samples` is the mono window, oldest first; `out` gets each band's share of the window's
    /// mean square, scaled by `GAIN`.
    pub fn bands(&mut self, samples: &[f32; WINDOW], out: &mut [f32; BANDS]) {
        for ((slot, sample), w) in self.input.iter_mut().zip(samples).zip(&self.hann) {
            *slot = sample * w;
        }
        if self.fft.process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch).is_err() {
            *out = [0.0; BANDS];
            return;
        }
        for (band, (first, last)) in out.iter_mut().zip(self.edges) {
            let power: f32 = self.output[first..last].iter().map(Complex32::norm_sqr).sum();
            *band = power * self.norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::analyzer::band_freq;

    fn sine(freq: f32, amp: f32) -> [f32; WINDOW] {
        std::array::from_fn(|i| amp * (std::f32::consts::TAU * freq * i as f32 / RATE).sin())
    }

    #[test]
    fn a_tone_lands_in_its_own_band() {
        let mut spectrum = Spectrum::default();
        let mut out = [0.0; BANDS];
        for band in [6, 20, 40] {
            spectrum.bands(&sine(band_freq(band), 0.5), &mut out);
            let loudest = (0..BANDS).max_by(|a, b| out[*a].total_cmp(&out[*b])).unwrap();
            assert!(loudest.abs_diff(band) <= 1, "a tone at band {band} peaked in band {loudest}");
        }
    }

    #[test]
    fn silence_has_no_power() {
        let mut spectrum = Spectrum::default();
        let mut out = [1.0; BANDS];
        spectrum.bands(&[0.0; WINDOW], &mut out);
        assert!(out.iter().all(|p| *p == 0.0));
    }
}
