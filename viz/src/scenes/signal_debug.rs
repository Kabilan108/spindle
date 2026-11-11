//! A plain readout of the simulated analysis, for tuning the generator: bands, the three energies,
//! loudness and onsets.

use rune::{El, LineCap, Shape, canvas, col, el, row, text};

use crate::signal::{BANDS, SimTrack};
use crate::theme::{Palette, font};
use crate::State;

pub fn view(s: &State, p: Palette) -> El {
    let f = s.frame;
    let (w, h) = (356.0, 110.0);
    let mut shape = Shape::new();
    let pitch = w / BANDS as f32;
    for (i, v) in f.bands.iter().enumerate() {
        let x = pitch * (i as f32 + 0.5);
        shape = shape.line([x, h], [x, h - v * h], pitch * 0.7, LineCap::Butt, p.accent);
    }
    let meter = |label: &str, v: f32| {
        row()
            .gap(6.0)
            .center_y()
            .child(text(label.to_owned()).font(font::SMALL).color(p.muted).w(56.0))
            .child(el().w(200.0).h(6.0).fill(p.line).child(el().w(200.0 * v.clamp(0.0, 1.0)).h(6.0).fill(p.text)))
    };
    col()
        .w_full()
        .h_full()
        .padding(12.0)
        .gap(4.0)
        .fill(if f.since_onset < 0.06 { p.raised } else { p.bg })
        .child(text(format!("{:?}  pos {:.2}  onset {:.2} ago", SimTrack::section(s.position()), s.position(), f.since_onset)).font(font::SMALL).color(p.text))
        .child(canvas(shape).size(w, h))
        .child(meter("loud", f.loudness))
        .child(meter("bass", f.bass))
        .child(meter("mid", f.mid))
        .child(meter("treble", f.treble))
}
