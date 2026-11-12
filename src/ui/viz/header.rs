//! The ambient header: four pools of the cover's colours behind an album or playlist header.
//! They drift with the music and swell with the bass, mids, treble and loudness. Paused, they
//! stop where they are and keep the page tinted. Everything moves through live values; the
//! view never re-runs for it.

use rune::{El, Live, Spring, Tx, el};

use super::cover::ArtPalette;
use super::quarter;
use crate::engine::analysis::AudioFrame;
use crate::ui::Main;
use crate::ui::theme::Palette;

const POOL: f32 = 620.0;
/// How far the tint reaches below the header's top: the header's height, so it has faded out
/// where the track list's sticky heading starts.
pub const DEPTH: f32 = 236.0;
const FADE: f32 = 120.0;
/// Each pool's resting place, as a fraction of the header's width and an offset from its top.
const HOMES: [(f32, f32); 4] = [(-0.07, -260.0), (0.34, -300.0), (0.63, -180.0), (0.13, -120.0)];
/// Drift rates (x, y) and phase per pool.
const WANDER: [(f32, f32, f32); 4] = [(0.31, 0.23, 0.0), (0.27, 0.35, 1.7), (0.37, 0.29, 3.1), (0.21, 0.33, 4.4)];

pub struct Header {
    offsets: [Live<[f32; 2]>; 4],
    scales: [Live<f32>; 4],
    glow: Live<f32>,
    /// The drift clock: advances while playing, faster when louder.
    drift: f64,
    width: f32,
}

impl Default for Header {
    fn default() -> Self {
        let header = Self {
            offsets: std::array::from_fn(|_| Live::new([0.0; 2])),
            scales: std::array::from_fn(|_| Live::new(1.0)),
            glow: Live::new(0.7),
            drift: 0.0,
            width: 870.0,
        };
        header.place();
        header
    }
}

impl Header {
    fn place(&self) {
        let d = self.drift as f32;
        for (i, ((x, y), (fx, fy, phase))) in HOMES.iter().zip(WANDER).enumerate() {
            let home = [x * self.width, *y];
            let wander = [70.0 * (d * fx + phase).sin(), 46.0 * (d * fy + phase * 1.3).cos()];
            self.offsets[i].set([quarter(home[0] + wander[0]), quarter(home[1] + wander[1])]);
        }
    }

    #[cfg(test)]
    pub fn pool_offset(&self, i: usize) -> [f32; 2] {
        self.offsets[i].get()
    }

    fn resize(&mut self, width: f32) {
        if (width - self.width).abs() > 0.5 {
            self.width = width;
            self.place();
        }
    }

    /// One analysis step, `dt` seconds after the last.
    pub fn apply(&mut self, frame: &AudioFrame, dt: f64, playing: bool) {
        if playing {
            self.drift += dt * (0.25 + 1.1 * f64::from(frame.loudness));
        }
        self.place();
        let energies = [frame.bass, frame.mid, frame.treble, frame.loudness];
        for (scale, energy) in self.scales.iter().zip(energies) {
            scale.set(((1.0 + 0.12 * energy) * 100.0).round() / 100.0);
        }
        self.glow.set(((0.7 + 0.3 * frame.loudness) * 50.0).round() / 50.0);
    }
}

/// The pools, faded into the page by `DEPTH`, to sit behind a page header.
pub fn ambient(s: &Main, p: Palette, palette: &ArtPalette) -> El {
    let header = &s.viz.header;
    let mut layer = el()
        .key("ambient")
        .pin(Some(0.0), Some(0.0), Some(0.0), None)
        .h(DEPTH)
        .clip_rounded(0.0)
        .on_layout_tx(|s: &mut Main, r: [f32; 4], tx: &mut Tx| {
            s.viz.header.resize(r[2]);
            tx.live_only();
        });
    for (i, tone) in palette.pools(p.dark).into_iter().enumerate() {
        layer = layer.child(
            el().key(format!("pool{i}"))
                .absolute(0.0, 0.0)
                .size(POOL, POOL)
                .radial_gradient(
                    [0.5, 0.5],
                    POOL / 2.0,
                    [(0.0, tone.alpha(0.42)), (0.3, tone.alpha(0.30)), (0.55, tone.alpha(0.13)), (0.8, tone.alpha(0.035)), (1.0, tone.alpha(0.0))],
                )
                .live_offset(header.offsets[i].smooth(Spring::new(0.35, 0.0)))
                .live_scale(header.scales[i].smooth(Spring::new(0.3, 0.0)))
                .live_opacity(header.glow.smooth(Spring::new(0.4, 0.0))),
        );
    }
    layer.child(el().key("ambient-fade").pin(Some(0.0), None, Some(0.0), Some(0.0)).h(FADE).gradient(p.bg.alpha(0.0), p.bg))
}
