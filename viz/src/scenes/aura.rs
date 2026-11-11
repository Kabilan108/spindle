//! 04 Aura: the mini player's background is a rune-fx mesh gradient made from the cover's
//! colours, drifting faster when the music is louder; the art glows with the bass and strong
//! kicks send a ripple out from it. All of it is compositor work: the view is built once.

use rune::{Color, El, el, effect};

use crate::chrome::{MINI_PAD, MINI_SIZE, MINI_TILE, cover, mini_controls, mini_progress, mini_tiles, mini_top};
use crate::gfx::{GLOW, MESH, RIPPLE};
use crate::theme::Palette;
use crate::State;

pub fn view(s: &State, p: Palette) -> El {
    let (w, h) = MINI_SIZE;
    let art_centre = [MINI_PAD + 32.0, MINI_PAD + 32.0];
    let art = el()
        .key("art-box")
        .size(64.0, 64.0)
        .child(effect(GLOW).key("art-glow").absolute(0.0, 0.0).size(64.0, 64.0).radius(6.0).effect_data([22.0, 0.7, 0.0, 0.0]).effect_bleed(48.0))
        .child(cover(s, s.track, 64.0, 6.0).absolute(0.0, 0.0).border(1.0, Color::WHITE.alpha(0.08)));
    rune::col()
        .key("mini")
        .w_full()
        .h_full()
        .padding(MINI_PAD)
        .gap(10.0)
        .fill(p.bg)
        .child(effect(MESH).key("mesh").absolute(0.0, 0.0).size(w, h))
        .child(effect(RIPPLE).key("ripple").absolute(0.0, 0.0).size(w, h).effect_data([art_centre[0], art_centre[1], 0.55, 0.0]))
        .child(
            el().key("scrim")
                .absolute(0.0, 0.0)
                .size(w, h)
                .linear_gradient(std::f32::consts::PI, [(0.0, p.bg.alpha(0.18)), (0.6, p.bg.alpha(0.32)), (1.0, p.bg.alpha(0.62))]),
        )
        .child(el().key("edge").absolute(0.0, 0.0).size(w, h).border(1.0, Color::WHITE.alpha(0.09)))
        .child(mini_top(s, p, art))
        .child(mini_controls(p, s.playing()))
        .child(mini_progress(s, p, w - 2.0 * MINI_PAD))
        .child(mini_tiles(s, p, MINI_TILE))
}

pub fn poll(s: &mut State) -> bool {
    super::step_fx(s, 0.45);
    false
}
