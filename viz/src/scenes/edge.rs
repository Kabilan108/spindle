//! 05 Edge light: the now-playing bar's top edge carries a line of light in the cover's colours
//! that brightens with loudness, with a faint bloom rising into the page; next to the title the
//! Spindle mark's three threads move with bass, mids and treble, and at rest settle back into the
//! mark. Live opacity and live rects only.

use std::f32::consts::FRAC_PI_2;

use rune::{El, Spring, el, row};

use super::q;
use crate::chrome::{bar, home_page, main_window, plain_seek};
use crate::theme::Palette;
use crate::{State, publish};

const MARK: f32 = 16.0;
/// The mark's own thread length at this size: at rest the glyph is exactly the Spindle mark.
const THREAD_REST: f32 = 6.6;

fn spool(s: &State, p: Palette) -> El {
    let flange = |y: f32| el().absolute(2.67, y).size(10.67, 2.13).radius(1.07).fill(p.accent);
    let mut m = el().key("spool").size(MARK, MARK).child(flange(2.0)).child(flange(11.87));
    for i in 0..3 {
        m = m.child(
            el().key(format!("thread{i}"))
                .absolute(0.0, 0.0)
                .size(0.0, 0.0)
                .radius(0.64)
                .fill(p.accent)
                .live_rect(s.sc.bars[i].smooth(Spring::new(0.09, 0.0))),
        );
    }
    m
}

pub fn view(s: &State, p: Palette) -> El {
    let pal = s.palette();
    let smooth = Spring::new(0.18, 0.0);
    let line = el()
        .key("edge-line")
        .pin(Some(0.0), Some(-1.0), Some(0.0), None)
        .h(2.0)
        .linear_gradient(
            FRAC_PI_2,
            [(0.0, pal.line.alpha(0.0)), (0.12, pal.line), (0.5, pal.line.mix(pal.line2, 0.5)), (0.88, pal.line2), (1.0, pal.line2.alpha(0.0))],
        )
        .live_opacity(s.sc.levels[0].smooth(smooth));
    let bloom = el()
        .key("edge-bloom")
        .pin(Some(0.0), Some(-44.0), Some(0.0), None)
        .h(44.0)
        .radial_gradient([0.45, 1.0], [520.0, 44.0], [(0.0, pal.line.alpha(0.2)), (0.5, pal.line.alpha(0.06)), (1.0, pal.line.alpha(0.0))])
        .live_opacity(s.sc.levels[1].smooth(Spring::new(0.3, 0.0)));
    let seek = plain_seek(s, p);
    let b = bar(s, p, Some(spool(s, p)), seek, vec![bloom, line]);
    main_window(s, p, true, row().grow(1.0).child(home_page(s, p)), b)
}

pub fn poll(s: &mut State, hops: usize) -> bool {
    let _ = hops;
    let f = s.frame;
    let playing = s.playing();
    let energies = [f.bass, f.mid, f.treble];
    for (i, e) in energies.iter().enumerate() {
        let w = if playing { q(1.5 + 9.5 * e) } else { THREAD_REST };
        let y = 5.73 + i as f32 * 2.27 - 0.64;
        publish(&s.sc.bars[i], [(MARK - w) / 2.0, y, w, 1.27], s.t);
    }
    let edge = if f.loudness > 0.0 { (0.15 + 0.85 * f.loudness).min(1.0) } else { 0.0 };
    publish(&s.sc.levels[0], (edge * 50.0).round() / 50.0, s.t);
    publish(&s.sc.levels[1], (f.loudness * f.loudness * 20.0).round() / 20.0, s.t);
    false
}
