//! 07 Stage: a full-window now-playing view. The cover, blurred once per track on the CPU, fills
//! the window; the cover itself turns slowly as a disc inside a radial spectrum, glows with the
//! bass (rune-fx) and sends a ripple across the window on strong kicks. The spectrum ring is the
//! one part that commits the view, once per analysis hop, with everything else memoized.

use std::f32::consts::PI;

use rune::{Color, El, LineCap, Shape, SurfaceFit, canvas, col, el, effect, memo, row, spacer, text};

use super::groundline::level_at;
use crate::chrome::{icon_button, play_button};
use crate::gfx::{GLOW, GRAIN, RIPPLE};
use crate::icons;
use crate::scenes::sleeve::sheen;
use crate::theme::{Palette, font};
use crate::{State, publish};

const W: f32 = 1100.0;
const H: f32 = 720.0;
const C: [f32; 2] = [390.0, 372.0];
const DISC: f32 = 320.0;
const R0: f32 = 178.0;
const LEN: f32 = 64.0;
const PER_SIDE: usize = 44;
const CANVAS: f32 = 2.0 * (R0 + LEN + 8.0);
pub const SEEK_W: f32 = 250.0;

fn ring(levels: &[f32; crate::signal::BANDS], glow: Color, glow2: Color) -> Shape {
    let mut sh = Shape::new();
    let c = CANVAS / 2.0;
    for k in 0..PER_SIDE {
        let v = level_at(levels, k, PER_SIDE);
        let colour = glow2.mix(glow, k as f32 / (PER_SIDE - 1) as f32).alpha(0.45 + 0.55 * v);
        let len = 0.5 + LEN * v;
        for side in [1.0f32, -1.0] {
            let a = side * (PI - (k as f32 + 0.5) / PER_SIDE as f32 * PI);
            let (sx, cy) = (a.sin(), -a.cos());
            let p0 = [c + R0 * sx, c + R0 * cy];
            let p1 = [c + (R0 + len) * sx, c + (R0 + len) * cy];
            sh = sh.line(p0, p1, 3.4, LineCap::Round, colour);
        }
    }
    sh
}

fn disc(s: &State) -> El {
    let r = DISC / 2.0;
    el().key("disc")
        .size(DISC, DISC)
        .radius(r)
        .pixels_fit(s.arts.pixels(s.track), SurfaceFit::Fill)
        .center()
        .rotate(s.sc.spin.target)
        .rotate_motion(s.sc.spin.anim)
        .child(el().size(34.0, 34.0).radius(17.0).fill(Color::BLACK.alpha(0.35)).border(1.0, Color::WHITE.alpha(0.18)).center().child(el().size(9.0, 9.0).radius(4.5).fill(Color { r: 0.02, g: 0.02, b: 0.02, a: 1.0 })))
}

fn info(s: &State, p: Palette) -> El {
    let i = s.track.info();
    col()
        .key("info")
        .w(360.0)
        .gap(8.0)
        .child(text(i.album.to_uppercase()).font(font::CAPS).color(p.text.alpha(0.7)))
        .child(text(i.title).font(rune::TextStyle::new(40.0, 700.0).lh(46.0).tracking(-0.6)).color(p.text))
        .child(text(i.artist).font(font::SECTION).color(p.text.alpha(0.78)))
        .child(
            row()
                .mt(28.0)
                .gap(10.0)
                .center_y()
                .child(text("0:50").live_text(&s.elapsed).font(font::TIME).color(p.text.alpha(0.7)))
                .child(el().w(SEEK_W).h(4.0).radius(2.0).fill(Color::WHITE.alpha(0.16)).child(el().absolute(0.0, 0.0).size(0.0, 4.0).radius(2.0).fill(p.text).live_rect(&s.progress)))
                .child(text("4:21").font(font::TIME).color(p.text.alpha(0.7))),
        )
        .child(
            row()
                .mt(14.0)
                .gap(18.0)
                .center_y()
                .child(icon_button(&icons::SHUFFLE, 34.0, p.text.alpha(0.7)))
                .child(icon_button(&icons::PREVIOUS, 38.0, p.text))
                .child(play_button(p, s.playing(), 56.0))
                .child(icon_button(&icons::NEXT, 38.0, p.text))
                .child(icon_button(&icons::REPEAT, 34.0, p.text.alpha(0.7))),
        )
}

pub fn view(s: &State, p: Palette) -> El {
    let pal = s.palette();
    let track = s.track;
    let backdrop = memo("stage-bg", format!("{track:?}"), || {
        el().key("bg")
            .absolute(0.0, 0.0)
            .size(W, H)
            .pixels_fit(s.arts.backdrop.get(&track).unwrap(), SurfaceFit::Fill)
            .child(el().absolute(0.0, 0.0).size(W, H).fill(p.bg.alpha(0.52)))
            .child(
                el().absolute(0.0, 0.0)
                    .size(W, H)
                    .radial_gradient([C[0] / W, C[1] / H], [900.0, 700.0], [(0.0, p.bg.alpha(0.0)), (0.5, p.bg.alpha(0.15)), (1.0, p.bg.alpha(0.7))]),
            )
    });
    let spectrum = canvas(ring(&s.sc.radial, pal.glow, pal.glow2)).key("ring").absolute(C[0] - CANVAS / 2.0, C[1] - CANVAS / 2.0).size(CANVAS, CANVAS);
    let disc_group = el()
        .key("disc-pos")
        .absolute(C[0] - DISC / 2.0, C[1] - DISC / 2.0)
        .size(DISC, DISC)
        .child(effect(GLOW).key("disc-glow").absolute(0.0, 0.0).size(DISC, DISC).radius(DISC / 2.0).effect_data([46.0, 0.6, 0.0, 0.0]).effect_bleed(110.0))
        .child(disc(s).absolute(0.0, 0.0).shadow(10.0, 40.0, Color::BLACK.alpha(0.5)))
        .child(sheen(DISC).absolute(0.0, 0.0));
    let top = row()
        .key("top")
        .absolute(0.0, 0.0)
        .w(W)
        .h(56.0)
        .px(20.0)
        .center_y()
        .child(icon_button(&icons::MINI, 32.0, p.text.alpha(0.8)))
        .child(spacer())
        .child(text("Playing from album").font(font::SMALL).color(p.text.alpha(0.6)))
        .child(spacer())
        .child(icon_button(&icons::CLOSE, 32.0, p.text.alpha(0.8)));
    let playing = s.playing();
    let info_el = memo("stage-info", playing, || info(s, p).absolute(690.0, 248.0));
    el().key("stage")
        .w_full()
        .h_full()
        .fill(p.bg)
        .child(backdrop)
        .child(effect(GRAIN).key("grain").absolute(0.0, 0.0).size(W, H).effect_data([0.025, 0.0, 0.0, 0.0]))
        .child(effect(RIPPLE).key("ripple").absolute(0.0, 0.0).size(W, H).effect_data([C[0], C[1], 0.45, 0.0]))
        .child(spectrum)
        .child(disc_group)
        .child(top)
        .child(info_el)
}

pub fn poll(s: &mut State, hops: usize, changed: bool, first: bool) -> bool {
    super::step_fx(s, 0.45);
    let mut rebuild = false;
    if first {
        s.sc.spin = super::Spin::new(16.0);
    } else {
        if s.playing() && s.sc.spin.started.is_none() {
            s.sc.spin.start(s.t);
            rebuild = true;
        }
        if changed && !s.playing() {
            s.sc.spin.stop(s.t, 2.4);
            rebuild = true;
        }
    }
    if hops > 0 {
        let next = s.frame.bands.map(|v| (v * 64.0).round() / 64.0);
        if next != s.sc.radial {
            s.sc.radial = next;
            rebuild = true;
        }
    }
    publish(&s.sc.levels[15], 0.0, s.t);
    rebuild
}
