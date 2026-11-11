//! 02 Trace: the mini player's progress bar becomes the track's loudness trace, drawn as it plays.
//! Played columns are one shape that changes only when a column completes (every 2.2 s on a
//! 4:21 track); the column being written is a live rect, the "pen".

use rune::{El, LineCap, Shape, Spring, canvas, col, el, memo};

use super::q;
use crate::chrome::{MINI_PAD, MINI_SIZE, cover, mini_controls, mini_tiles, mini_top};
use crate::signal::{BandSource, loudness_columns};
use crate::theme::Palette;
use crate::{START_POS, State, publish};

const COLS: usize = 118;
const H: f32 = 18.0;
const W: f32 = MINI_SIZE.0 - 2.0 * MINI_PAD;
const COL_W: f32 = 1.7;
const TILE: f32 = 48.0;

fn pitch() -> f32 {
    W / COLS as f32
}

fn half(v: f32) -> f32 {
    1.0 + (H / 2.0 - 1.5) * v.clamp(0.0, 1.0).powf(1.2)
}

fn trace_shape(trace: &[f32], current: usize, played: rune::Color, future: rune::Color) -> Shape {
    let mut sh = Shape::new();
    let mid = H / 2.0;
    for (c, v) in trace.iter().enumerate() {
        let x = pitch() * (c as f32 + 0.5);
        if c < current && *v >= 0.0 {
            let h = half(*v);
            sh = sh.line([x, mid - h + 0.85], [x, mid + h - 0.85], COL_W, LineCap::Round, played);
        } else if c > current {
            sh = sh.line([x, mid - 0.01], [x, mid + 0.01], 1.5, LineCap::Round, future);
        }
    }
    sh
}

pub fn view(s: &State, p: Palette) -> El {
    let pal = s.palette();
    let current = s.sc.trace_col;
    let trace = memo("trace-shape", (current, s.sc.trace.len()), || {
        canvas(trace_shape(&s.sc.trace, current, pal.glow.alpha(0.9), p.faint.alpha(0.7))).key("trace-shape").absolute(0.0, 0.0).size(W, H)
    });
    let strip = el()
        .key("trace")
        .w(W)
        .h(H)
        .child(trace)
        .child(el().key("pen").absolute(0.0, 0.0).size(0.0, 0.0).radius(1.0).fill(p.text).live_rect(s.sc.bars[0].smooth(Spring::new(0.08, 0.0))));
    col()
        .key("mini")
        .w_full()
        .h_full()
        .padding(MINI_PAD)
        .gap(8.0)
        .fill(p.bg)
        .border(1.0, p.line)
        .child(mini_top(s, p, cover(s, s.track, 64.0, 6.0)))
        .child(mini_controls(p, s.playing()))
        .child(strip.mt(-2.0))
        .child(mini_tiles(s, p, TILE).mt(-2.0))
}

pub fn poll(s: &mut State, hops: usize, first: bool) -> bool {
    let per = s.feed.source.duration() / COLS as f64;
    let col_now = (s.position() / per) as usize;
    let mut rebuild = false;
    if first {
        s.sc.trace = loudness_columns(&s.feed.source, COLS, START_POS);
        s.sc.trace_col = col_now;
        let partial = s.sc.trace[col_now].max(0.0);
        let hops_so_far = ((START_POS - col_now as f64 * per) / crate::signal::HOP) as u32;
        s.sc.trace_acc = (partial * hops_so_far as f32, hops_so_far);
        s.sc.trace[col_now] = -1.0;
        rebuild = true;
    }
    if hops > 0 && s.playing() {
        s.sc.trace_acc.0 += s.frame.loudness;
        s.sc.trace_acc.1 += 1;
    }
    if col_now != s.sc.trace_col {
        let (sum, n) = s.sc.trace_acc;
        s.sc.trace[s.sc.trace_col] = if n > 0 { sum / n as f32 } else { 0.0 };
        s.sc.trace_col = col_now;
        s.sc.trace_acc = (0.0, 0);
        rebuild = true;
    }
    let (sum, n) = s.sc.trace_acc;
    let running = if n > 0 { sum / n as f32 } else { 0.0 };
    let v = if s.playing() { s.frame.loudness.max(0.0) } else { running };
    let h = q(2.0 * half(v));
    let x = pitch() * (s.sc.trace_col as f32 + 0.5) - 1.1;
    publish(&s.sc.bars[0], [x, (H - h) / 2.0, 2.2, h], s.t);
    rebuild
}
