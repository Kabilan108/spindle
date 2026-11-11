//! 03 Sleeve: the cover is a record sleeve; while playing, the record slides out and turns at
//! 33⅓ rpm with the cover as its label. Paused, it eases to a stop and slides home. Uses no
//! audio analysis at all: play state and nothing else.

use std::f32::consts::PI;

use rune::{Color, El, Spring, SurfaceFit, Tx, el, oklch};

use crate::chrome::{MINI_SIZE, MINI_TILE, mini_controls, mini_frame, mini_progress, mini_tiles, mini_top};
use crate::theme::Palette;
use crate::State;

const ART: f32 = 64.0;
const DISC: f32 = 60.0;
const PEEK: f32 = 26.0;
pub const STOP_SECS: f32 = 1.6;

pub fn record(s: &State, d: f32, label: f32) -> El {
    let r = d / 2.0;
    let vinyl = oklch(0.2, 0.004, 70.0);
    let mut grooves: Vec<(f32, Color)> = vec![(0.0, Color::WHITE.alpha(0.0)), (label / d + 0.02, Color::WHITE.alpha(0.0))];
    let n = 5;
    for k in 0..n {
        let t0 = label / d + 0.06 + k as f32 * (0.93 - label / d - 0.06) / n as f32;
        grooves.push((t0, Color::WHITE.alpha(0.07)));
        grooves.push((t0 + 0.03, Color::WHITE.alpha(0.0)));
    }
    grooves.push((0.965, Color::WHITE.alpha(0.0)));
    grooves.push((0.985, Color::WHITE.alpha(0.09)));
    grooves.push((1.0, Color::WHITE.alpha(0.02)));
    el().size(d, d)
        .radius(r)
        .fill(vinyl)
        .radial_gradient([0.5, 0.5], r, grooves)
        .center()
        .child(
            el().size(label, label)
                .radius(label / 2.0)
                .pixels_fit(s.arts.pixels(s.track), SurfaceFit::Fill)
                .center()
                .child(el().size(label * 0.12, label * 0.12).radius(label * 0.06).fill(oklch(0.12, 0.004, 70.0)).border(0.5, Color::WHITE.alpha(0.25))),
        )
}

/// A highlight that stays put while the record turns under it, which is what sells the spin.
pub fn sheen(d: f32) -> El {
    el().size(d, d).radius(d / 2.0).linear_gradient(
        PI * 0.72,
        [
            (0.0, Color::WHITE.alpha(0.0)),
            (0.36, Color::WHITE.alpha(0.0)),
            (0.47, Color::WHITE.alpha(0.13)),
            (0.53, Color::WHITE.alpha(0.13)),
            (0.64, Color::WHITE.alpha(0.0)),
            (1.0, Color::WHITE.alpha(0.0)),
        ],
    )
}

pub fn view(s: &State, p: Palette) -> El {
    let playing = s.playing();
    let peek = if playing { PEEK } else { 0.0 };
    let slide = Spring::new(if playing { 0.6 } else { 0.9 }, 0.0);
    let off = (ART - DISC) / 2.0;
    let disc = record(s, DISC, 26.0).key("record").rotate(s.sc.spin.target).rotate_motion(s.sc.spin.anim);
    let art = el()
        .key("sleeve-box")
        .size(ART + peek, ART)
        .motion(slide)
        .child(el().key("disc-pos").absolute(off + peek, off).size(DISC, DISC).motion(slide).child(disc).child(sheen(DISC).absolute(0.0, 0.0)))
        .child(
            el().key("sleeve")
                .absolute(0.0, 0.0)
                .size(ART, ART)
                .radius(4.0)
                .pixels_fit(s.arts.pixels(s.track), SurfaceFit::Fill)
                .shadow(0.0, 10.0, Color::BLACK.alpha(0.55))
                .border(1.0, Color::WHITE.alpha(0.06)),
        );
    mini_frame(p)
        .child(mini_top(s, p, art))
        .child(mini_controls(p, playing))
        .child(mini_progress(s, p, MINI_SIZE.0 - 24.0))
        .child(mini_tiles(s, p, MINI_TILE))
}

pub fn poll(s: &mut State, changed: bool, first: bool, tx: &mut Tx) -> bool {
    let _ = tx;
    if first {
        return false;
    }
    if s.playing() && s.sc.spin.started.is_none() {
        s.sc.spin.start(s.t);
        return true;
    }
    if changed && !s.playing() {
        s.sc.spin.stop(s.t, STOP_SECS);
        return true;
    }
    false
}
