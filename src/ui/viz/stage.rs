//! The Stage: the now-playing bar expanded into the whole window. The cover, blurred, fills it;
//! the cover itself turns as a disc inside a radial spectrum, glows with the bass and sends a
//! ripple out on strong kicks. Beside it are the track, the seek bar and the transport.
//!
//! The spectrum is 88 fixed strokes whose drawn length is a live trim, the glow and ripple are
//! live scale and opacity, so music never re-runs the view. The disc turns on a long committed
//! tween: one commit starts it, one eases it to a stop on pause.

use std::f32::consts::PI;

use rune::{Anim, AnimExt, Color, Curve, Cursor, Drag, Easing, El, LineCap, Live, Shape, Spring, SurfaceFit, TextStyle, Tx, canvas, col, el, icon, row, text};

use super::cover::{ArtPalette, Look};
use super::{COLLAPSE, playing_now};
use crate::engine::analysis::{AudioFrame, BANDS};
use crate::model::{Command, Repeat, Track};
use crate::ui::bar::{slider, toggle};
use crate::ui::meter::fmt_time;
use crate::ui::theme::{Palette, font};
use crate::ui::widgets::{icon_button, toast};
use crate::ui::{Main, icons};

const DISC: f32 = 300.0;
/// Where the spectrum's strokes start, and how long they grow.
const R0: f32 = 170.0;
const LEN: f32 = 62.0;
const PER_SIDE: usize = 44;
const BOX: f32 = 2.0 * (R0 + LEN + 8.0);
const GLOW: f32 = DISC + 260.0;
const STROKE: f32 = 3.4;
/// The trim a silent band keeps: a dot.
const DOT: f32 = 0.016;
/// How much the disc glows at rest, so a paused Stage still sits in light.
const REST_GLOW: f32 = 0.2;
/// A kick at least this strong sends a ripple.
const RIPPLE_ONSET: f32 = 0.45;
const SPIN_PERIOD: f32 = 16.0;
const INFO_W: f32 = 340.0;

/// A renderer-driven spin. Rune has no live rotation, so one commit starts a long linear turn
/// and one at pause eases it to a stop.
#[derive(Clone, Copy)]
struct Spin {
    /// When the current turn started (clock) and from what angle.
    started: Option<(f64, f32)>,
    target: f32,
    anim: Anim,
}

/// One long turn; the poll starts another before it runs out.
const RUN: f32 = 600.0;

impl Spin {
    fn omega() -> f32 {
        std::f32::consts::TAU / SPIN_PERIOD
    }

    fn angle(&self, clock: f64) -> f32 {
        match self.started {
            Some((t0, a0)) => a0 + Self::omega() * (clock - t0) as f32,
            None => self.target,
        }
    }

    fn start(&mut self, clock: f64) {
        let from = self.angle(clock);
        self.started = Some((clock, from));
        self.target = from + Self::omega() * RUN;
        self.anim = Anim::new(Curve::linear(RUN)).merge();
    }

    /// Eases to a stop over `secs`, leaving at the current speed: the curve starts at slope 2,
    /// so covering ω·T/2 starts at exactly ω.
    fn stop(&mut self, clock: f64, secs: f32) {
        let now = self.angle(clock);
        self.started = None;
        self.target = now + Self::omega() * secs / 2.0;
        self.anim = Anim::new(Curve::tween(secs, Easing::cubic(0.3, 0.6, 0.6, 1.0))).merge();
    }
}

pub struct Stage {
    /// Trim of each spectrum stroke, low bands first; both sides share one.
    bars: Vec<Live<[f32; 2]>>,
    glow: Live<f32>,
    swell: Live<f32>,
    ripple_scale: Live<f32>,
    ripple_alpha: Live<f32>,
    last_ripple: f64,
    spin: Spin,
}

impl Default for Stage {
    fn default() -> Self {
        Self {
            bars: (0..PER_SIDE).map(|_| Live::new([0.0, DOT])).collect(),
            glow: Live::new(REST_GLOW),
            swell: Live::new(1.0),
            ripple_scale: Live::new(3.0),
            ripple_alpha: Live::new(0.0),
            last_ripple: f64::NEG_INFINITY,
            spin: Spin { started: None, target: 0.0, anim: Anim::INSTANT },
        }
    }
}

/// Band `i` of `n` across the analyzer's bands, skipping the first two (sub-bass rumble) and
/// lifting the highs a little so the ring reads all the way round.
fn level_at(bands: &[f32; BANDS], i: usize, n: usize) -> f32 {
    let x = 2.0 + (BANDS as f32 - 3.0) * i as f32 / (n - 1) as f32;
    let (a, t) = (x.floor() as usize, x.fract());
    let v = bands[a.min(BANDS - 1)] * (1.0 - t) + bands[(a + 1).min(BANDS - 1)] * t;
    let v = if v > 0.0 { v + 0.1 * i as f32 / (n - 1) as f32 } else { 0.0 };
    ((v - 0.16) / 0.84).clamp(0.0, 1.0).powf(1.25)
}

impl Stage {
    pub fn apply(&mut self, frame: &AudioFrame, clock: f64, playing: bool) {
        for (k, bar) in self.bars.iter().enumerate() {
            let v = level_at(&frame.bands, k, PER_SIDE);
            bar.set([0.0, ((DOT + (1.0 - DOT) * v) * 128.0).round() / 128.0]);
        }
        let heat = if playing || frame.bass > 0.0 { 0.25 + 0.75 * frame.bass } else { REST_GLOW };
        self.glow.set((heat * 50.0).round() / 50.0);
        self.swell.set(((1.0 + 0.08 * frame.bass) * 100.0).round() / 100.0);
        if playing && frame.onset >= RIPPLE_ONSET && frame.since_onset == 0.0 && clock - self.last_ripple > 0.3 {
            self.last_ripple = clock;
            self.ripple_scale.pulse(1.0, 3.2, std::time::Duration::ZERO);
            self.ripple_alpha.pulse(0.55 * frame.onset, 0.0, std::time::Duration::ZERO);
        }
    }

    /// Starts, stops or renews the disc's turn. True when the view has to commit for it. While
    /// playing, the analysis polls often enough to renew the turn before it runs out.
    pub fn spin(&mut self, clock: f64, playing: bool) -> bool {
        match (self.spin.started, playing) {
            (None, true) => self.spin.start(clock),
            (Some(_), false) => self.spin.stop(clock, 2.4),
            (Some((t0, _)), true) => {
                if clock < t0 + f64::from(RUN) - 30.0 {
                    return false;
                }
                self.spin.start(clock);
            }
            (None, false) => return false,
        }
        true
    }

    #[cfg(test)]
    pub fn level(&self, k: usize) -> f32 {
        self.bars[k].get()[1]
    }

    /// The Stage closed: the disc rests where it was.
    pub fn rest(&mut self, clock: f64) {
        self.spin.target = self.spin.angle(clock);
        self.spin.started = None;
        self.spin.anim = Anim::INSTANT;
    }
}

struct Facts {
    track: Option<Track>,
    playing: bool,
    liked: bool,
    shuffle: bool,
    repeat: Repeat,
    from: Option<String>,
}

fn facts(s: &Main) -> Facts {
    s.link.read(|store| {
        let playback = &store.playback;
        let from = playback.context_uri.as_ref().and_then(|uri| {
            let known = store.pages.get(uri).and_then(|page| page.ready()).map(|page| page.summary.name.clone());
            known.or_else(|| store.recents.iter().find(|r| &r.uri == uri).map(|r| r.name.clone()))
        });
        Facts {
            liked: playback.track.as_ref().is_some_and(|t| store.liked.contains(&t.uri)),
            track: playback.track.clone(),
            playing: playing_now(playback),
            shuffle: playback.shuffle,
            repeat: playback.repeat,
            from,
        }
    })
}

/// Spotify serves each cover at 64, 300 and 640 px; the library hands out the 300 px one.
fn larger(url: &str) -> Option<String> {
    url.contains("ab67616d00001e02").then(|| url.replace("ab67616d00001e02", "ab67616d0000b273"))
}

fn ring(s: &Main, palette: &ArtPalette) -> El {
    let c = BOX / 2.0;
    let mut ring = el().key("ring").absolute(0.0, 0.0).size(BOX, BOX);
    for (k, bar) in s.viz.stage.bars.iter().enumerate() {
        let colour = palette.glow2.mix(palette.glow, k as f32 / (PER_SIDE - 1) as f32).alpha(0.85);
        for (side, sign) in [("l", 1.0f32), ("r", -1.0)] {
            let a = sign * (PI - (k as f32 + 0.5) / PER_SIDE as f32 * PI);
            let (sx, cy) = (a.sin(), -a.cos());
            let shape = Shape::new().line([c + R0 * sx, c + R0 * cy], [c + (R0 + LEN) * sx, c + (R0 + LEN) * cy], STROKE, LineCap::Round, colour);
            ring = ring.child(canvas(shape).key(format!("bar{side}{k}")).absolute(0.0, 0.0).size(BOX, BOX).live_trim(bar.smooth(Spring::new(0.09, 0.0))));
        }
    }
    ring
}

fn disc(s: &Main, p: Palette, track: Option<&Track>) -> El {
    let small = track.and_then(|t| t.image_url.clone());
    let large = small.as_deref().and_then(larger);
    let pixels = large.as_deref().and_then(|url| s.art.pixels(&s.link, url)).or_else(|| small.as_deref().and_then(|url| s.art.pixels(&s.link, url)));
    let mut face = el().key("disc").size(DISC, DISC).radius(DISC / 2.0).fill(p.art_empty).center();
    face = match pixels {
        Some(pixels) => face.pixels_fit(&pixels, SurfaceFit::Fill),
        None => face.child(icon(&icons::NOTE, 72.0).color(p.faint)),
    };
    let spin = s.viz.stage.spin;
    face.rotate(spin.target)
        .rotate_motion(spin.anim)
        .child(
            el().size(34.0, 34.0)
                .radius(17.0)
                .fill(Color::BLACK.alpha(0.35))
                .border(1.0, Color::WHITE.alpha(0.18))
                .center()
                .child(el().size(9.0, 9.0).radius(4.5).fill(Color { r: 0.02, g: 0.02, b: 0.02, a: 1.0 })),
        )
}

fn sheen() -> El {
    el().size(DISC, DISC).radius(DISC / 2.0).linear_gradient(
        PI * 0.72,
        [
            (0.0, Color::WHITE.alpha(0.0)),
            (0.36, Color::WHITE.alpha(0.0)),
            (0.47, Color::WHITE.alpha(0.1)),
            (0.53, Color::WHITE.alpha(0.1)),
            (0.64, Color::WHITE.alpha(0.0)),
            (1.0, Color::WHITE.alpha(0.0)),
        ],
    )
}

fn turntable(s: &Main, p: Palette, palette: &ArtPalette, track: Option<&Track>) -> El {
    let stage = &s.viz.stage;
    let mid = |size: f32| (BOX - size) / 2.0;
    let glow = palette.glow;
    el().key("turntable")
        .size(BOX, BOX)
        .child(
            el().key("glow")
                .absolute(mid(GLOW), mid(GLOW))
                .size(GLOW, GLOW)
                .radial_gradient([0.5, 0.5], GLOW / 2.0, [(0.0, glow.alpha(0.6)), (0.5, glow.alpha(0.5)), (0.62, glow.alpha(0.22)), (0.8, glow.alpha(0.06)), (1.0, glow.alpha(0.0))])
                .live_opacity(stage.glow.smooth(Spring::new(0.18, 0.0)).rest(Spring::new(0.8, 0.0)))
                .live_scale(stage.swell.smooth(Spring::new(0.12, 0.0))),
        )
        .child(
            el().key("ripple")
                .absolute(mid(DISC), mid(DISC))
                .size(DISC, DISC)
                .radius(DISC / 2.0)
                .border(1.2, glow.alpha(0.9))
                .live_scale(stage.ripple_scale.smooth(Spring::new(0.03, 0.0)).rest(Spring::new(1.3, 0.0)))
                .live_opacity(stage.ripple_alpha.smooth(Spring::new(0.04, 0.0)).rest(Spring::new(1.1, 0.0))),
        )
        .child(ring(s, palette))
        .child(disc(s, p, track).absolute(mid(DISC), mid(DISC)).shadow(10.0, 40.0, Color::BLACK.alpha(0.5)))
        .child(sheen().absolute(mid(DISC), mid(DISC)))
}

fn seek(s: &Main, p: Palette, total_ms: u32) -> El {
    let ink = p.text.alpha(0.72);
    row()
        .key("stage-seek")
        .w_full()
        .gap(10.0)
        .center_y()
        .child(text("0:00").live_text(&s.progress.elapsed).font(font::TIME).color(ink).w(40.0))
        .child(
            slider("stage-position", "Position", &s.progress.bar, p, 4.0, 12.0)
                .grow(1.0)
                .on_drag(|s: &mut Main, d: &mut Drag, tx: &mut Tx| {
                    s.progress.drag(&s.link, d, tx);
                    tx.live_only();
                })
                .on_layout_tx(|s: &mut Main, r: [f32; 4], tx: &mut Tx| {
                    s.progress.bar.resize(r[2]);
                    tx.live_only();
                }),
        )
        .child(text(fmt_time(total_ms)).key("stage-total").font(font::TIME).color(ink).w(40.0))
}

fn transport(p: Palette, f: &Facts) -> El {
    let (play_label, play_glyph) = if f.playing { ("Pause", &icons::PAUSE) } else { ("Play", &icons::PLAY) };
    let repeat_glyph = if f.repeat == Repeat::Track { &icons::REPEAT_ONE } else { &icons::REPEAT };
    let shuffle = f.shuffle;
    row()
        .key("stage-transport")
        .gap(14.0)
        .center_y()
        .child(toggle("stage-shuffle", "Shuffle", &icons::SHUFFLE, shuffle, p).on_press(move |s: &mut Main| s.link.send(Command::SetShuffle(!shuffle))))
        .child(icon_button("stage-previous", "Previous", &icons::PREVIOUS, 40.0, p.text, p).on_press(|s: &mut Main| s.link.send(Command::Previous)))
        .child(
            el().key("stage-play")
                .role("button")
                .label(play_label)
                .size(56.0, 56.0)
                .radius(28.0)
                .center()
                .fill(p.text)
                .cursor(Cursor::Pointer)
                .focusable(true)
                .hover(p.bg.alpha(0.12))
                .press(p.bg.alpha(0.2))
                .on_press(|s: &mut Main| s.link.send(Command::PlayPause))
                .child(icon(play_glyph, 26.0).color(p.bg)),
        )
        .child(icon_button("stage-next", "Next", &icons::NEXT, 40.0, p.text, p).on_press(|s: &mut Main| s.link.send(Command::Next)))
        .child(toggle("stage-repeat", "Repeat", repeat_glyph, f.repeat != Repeat::Off, p).on_press(|s: &mut Main| s.cycle_repeat()))
}

fn info(s: &Main, p: Palette, f: &Facts) -> El {
    let Some(track) = &f.track else {
        return col().key("stage-info").w(INFO_W).gap(8.0).child(text("Nothing playing").font(font::SECTION).color(p.muted));
    };
    let mut album = text(track.album.as_ref().map(|a| a.name.to_uppercase()).unwrap_or_default()).key("stage-album").font(font::CAPS).color(p.text.alpha(0.7)).truncate();
    if let Some(uri) = track.album.as_ref().map(|a| a.uri.clone()).filter(|uri| !uri.is_empty()) {
        album = album.cursor(Cursor::Pointer).role("link").on_press(move |s: &mut Main| {
            s.viz.close_stage();
            s.open(&uri);
        });
    }
    let (uri, liked) = (track.uri.clone(), f.liked);
    let heart = if liked { &icons::HEART_FILLED } else { &icons::HEART };
    let like = icon_button("stage-like", if liked { "Remove from Liked Songs" } else { "Save to Liked Songs" }, heart, 36.0, if liked { p.accent } else { p.text.alpha(0.75) }, p)
        .on_press(move |s: &mut Main| s.set_liked(&uri, !liked));
    col()
        .key("stage-info")
        .w(INFO_W)
        .gap(8.0)
        .child(album)
        .child(text(track.name.clone()).key("stage-title").font(TextStyle::new(34.0, 700.0).lh(40.0).tracking(-0.5)).color(p.text).max_lines(2))
        .child(
            row()
                .gap(12.0)
                .center_y()
                .child(text(track.artist_line()).key("stage-artist").font(font::SECTION).color(p.text.alpha(0.78)).truncate().grow(1.0).min_w(0.0))
                .child(like),
        )
        .child(el().h(20.0))
        .child(seek(s, p, track.duration_ms))
        .child(row().w_full().justify_center().pt(10.0).child(transport(p, f)))
}

fn top(p: Palette, from: Option<&str>) -> El {
    let label = from.map_or_else(String::new, |name| format!("Playing from {name}"));
    row()
        .key("stage-top")
        .w_full()
        .h(56.0)
        .px(16.0)
        .center_y()
        .child(
            icon_button("stage-close", "Close the full player (Esc)", &COLLAPSE, 36.0, p.text.alpha(0.85), p)
                .on_press(|s: &mut Main| s.viz.close_stage()),
        )
        .child(row().grow(1.0).min_w(0.0).justify_center().child(text(label).font(font::SMALL).color(p.text.alpha(0.62)).truncate()))
        .child(el().size(36.0, 36.0))
}

pub fn view(s: &Main, p: Palette) -> El {
    let f = facts(s);
    let look: Option<Look> = f.track.as_ref().and_then(|t| t.image_url.as_deref()).and_then(|url| s.viz.covers.look(&s.link, url, s.me.as_ref()));
    let palette = look.as_ref().map_or_else(ArtPalette::plain, |look| look.palette);
    let mut stage = col().key("stage").w_full().h_full().fill(p.bg).clip_rounded(0.0);
    if let Some(look) = &look {
        stage = stage.child(el().key("stage-backdrop").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).pixels_fit(&look.backdrop, SurfaceFit::Fill));
    }
    stage
        .child(el().key("stage-scrim").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(p.bg.alpha(0.5)))
        .child(
            el().key("stage-vignette")
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .radial_gradient([0.36, 0.52], [900.0, 700.0], [(0.0, p.bg.alpha(0.0)), (0.5, p.bg.alpha(0.15)), (1.0, p.bg.alpha(0.7))]),
        )
        .child(top(p, f.from.as_deref()))
        .child(
            row()
                .key("stage-body")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .px(32.0)
                .pb(40.0)
                .gap(56.0)
                .center_y()
                .justify_center()
                .child(turntable(s, p, &palette, f.track.as_ref()))
                .child(info(s, p, &f)),
        )
}

/// The whole window while the Stage is open. It is always dark: it sits on the cover.
pub fn shell(s: &Main) -> El {
    let p = Palette::new(true);
    let mut root = view(s, p);
    if let Some(notice) = s.link.read(|store| store.notice.clone()) {
        root = root.child(toast(p, &notice));
    }
    root
}
