//! 01 Groundline: a hairline spectrum standing on the mini player's bottom edge, in the cover's
//! colours, under a faint loudness glow. Every bar is a live rect; the view never re-runs.

use rune::{El, Spring, el};

use super::q;
use crate::chrome::{MINI_SIZE, MINI_TILE, mini_controls, mini_frame, mini_progress, mini_tiles, mini_top, cover};
use crate::signal::BANDS;
use crate::theme::Palette;
use crate::{State, publish};

const N: usize = 64;
const STRIP_H: f32 = 12.0;
const MAX_H: f32 = 11.0;
const BAR_W: f32 = 2.2;

fn pitch() -> f32 {
    (MINI_SIZE.0 - 24.0) / N as f32
}

pub fn view(s: &State, p: Palette) -> El {
    let pal = s.palette();
    let mut strip = el().key("groundline").pin(Some(0.0), None, Some(0.0), Some(0.0)).h(STRIP_H);
    strip = strip.child(
        el().key("ground-glow")
            .pin(Some(0.0), None, Some(0.0), Some(0.0))
            .h(44.0)
            .radial_gradient([0.5, 1.0], [210.0, 44.0], [(0.0, pal.glow_a(0.22)), (0.55, pal.glow_a(0.07)), (1.0, pal.glow_a(0.0))])
            .live_opacity(s.sc.levels[0].smooth(Spring::new(0.25, 0.0))),
    );
    for i in 0..N {
        let k = i as f32 / (N - 1) as f32;
        let c = pal.glow.mix(pal.glow2, k);
        strip = strip.child(
            el().key(format!("g{i}"))
                .absolute(0.0, 0.0)
                .size(BAR_W, 0.0)
                .corners([BAR_W / 2.0, BAR_W / 2.0, 0.0, 0.0])
                .gradient(c, c.alpha(0.5))
                .live_rect(s.sc.bars[i].smooth(Spring::new(0.07, 0.0))),
        );
    }
    mini_frame(p)
        .child(strip)
        .child(mini_top(s, p, cover(s, s.track, 64.0, 6.0)))
        .child(mini_controls(p, s.playing()))
        .child(mini_progress(s, p, MINI_SIZE.0 - 24.0))
        .child(mini_tiles(s, p, MINI_TILE))
}

/// Band level for bar `i`, interpolated across the analysis bands (the lowest two are mostly
/// rumble and the top two mostly air, so they are left out).
pub fn level_at(bands: &[f32; BANDS], i: usize, n: usize) -> f32 {
    let x = 2.0 + (BANDS as f32 - 5.0) * i as f32 / (n - 1) as f32;
    let (a, t) = (x.floor() as usize, x.fract());
    let v = bands[a] * (1.0 - t) + bands[(a + 1).min(BANDS - 1)] * t;
    let v = if v > 0.0 { v + 0.1 * i as f32 / (n - 1) as f32 } else { 0.0 };
    ((v - 0.16) / 0.84).clamp(0.0, 1.0).powf(1.25)
}

pub fn poll(s: &mut State, hops: usize) -> bool {
    if hops == 0 {
        return false;
    }
    let f = s.frame;
    let pitch = pitch();
    for i in 0..N {
        let v = level_at(&f.bands, i, N);
        let h = q(MAX_H * v);
        let x = 12.0 + pitch * (i as f32 + 0.5) - BAR_W / 2.0;
        publish(&s.sc.bars[i], [x, STRIP_H - h, BAR_W, h], s.t);
    }
    publish(&s.sc.levels[0], (f.loudness * 20.0).round() / 20.0, s.t);
    false
}
