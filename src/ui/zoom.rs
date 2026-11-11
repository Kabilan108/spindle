//! The window's zoom, as a browser's: Ctrl+= or Ctrl++ in, Ctrl+- out, Ctrl+0 back to 100%.
//!
//! The UI is laid out at the window's size divided by the zoom and drawn scaled up from the
//! top-left. Rune rasterizes text under a still scale at the scaled size, and hit testing,
//! scrolling, anchored overlays and text fields all follow the scale, so zoomed text is sharp
//! and everything works as at 100%. Zooming in stops where the layout would get smaller
//! than [`LAYOUT_MIN`], the least it still works at (it truncates and scrolls, but nothing
//! overlaps), so the indicator says when a small window shows less than the chosen level:
//! the default 1100×720 window goes to 150%, a maximised 1080p one to 200%.

use std::time::Duration;

use rune::{Chord, Cx, El, KeyEvent, Transition, Tx, col, row, text};

use super::theme::{Palette, font};
use super::Main;
use crate::settings;

pub const STEPS: [f32; 8] = [0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];
const SHOWN_FOR: f64 = 1.2;
/// The smallest size the layout is made for.
pub const LAYOUT_MIN: (f32, f32) = (720.0, 480.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomKey {
    In,
    Out,
    Reset,
}

pub fn zoom_key(ev: &KeyEvent, tx: &Tx) -> Option<ZoomKey> {
    let chord = |spec: &str| Chord::parse(spec).is_ok_and(|c| c.matches_tx(ev, tx));
    if chord("mod+equal") || chord("mod+plus") || chord("mod+shift+equal") {
        Some(ZoomKey::In)
    } else if chord("mod+minus") {
        Some(ZoomKey::Out)
    } else if chord("mod+0") {
        Some(ZoomKey::Reset)
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct Zoom {
    /// The chosen level, one of [`STEPS`].
    pub level: f32,
    /// When the indicator goes away, on the app clock.
    shown_until: Option<f64>,
}

impl Default for Zoom {
    fn default() -> Self {
        Self { level: 1.0, shown_until: None }
    }
}

fn nearest_step(level: f32) -> f32 {
    STEPS.iter().copied().min_by(|a, b| (a - level).abs().total_cmp(&(b - level).abs())).unwrap_or(1.0)
}

/// The zoom the window can show at `size`: `level`, unless zooming in that far would lay the
/// UI out smaller than [`LAYOUT_MIN`].
pub fn fitted(level: f32, size: [f32; 2]) -> f32 {
    if level <= 1.0 {
        return level;
    }
    let fits = (size[0] / LAYOUT_MIN.0).min(size[1] / LAYOUT_MIN.1);
    level.min(fits.max(1.0))
}

pub fn percent(level: f32) -> u32 {
    (level * 100.0).round() as u32
}

impl Zoom {
    /// The level saved last time.
    pub fn restored() -> Self {
        Self { level: nearest_step(settings::get().zoom), shown_until: None }
    }

    /// A level as a headless scene shows it, with the indicator up for good if asked.
    pub fn preview(level: f32, indicator: bool) -> Self {
        Self { level: nearest_step(level), shown_until: indicator.then_some(3600.0) }
    }

    pub fn press(&mut self, key: ZoomKey, tx: &mut Tx) {
        let at = STEPS.iter().position(|&step| step == self.level).unwrap_or(2);
        let level = match key {
            ZoomKey::In => STEPS[(at + 1).min(STEPS.len() - 1)],
            ZoomKey::Out => STEPS[at.saturating_sub(1)],
            ZoomKey::Reset => 1.0,
        };
        self.set(level, tx);
    }

    pub fn set(&mut self, level: f32, tx: &mut Tx) {
        self.level = nearest_step(level);
        settings::update(|s| s.zoom = self.level);
        self.shown_until = Some(tx.clock() + SHOWN_FOR);
        tx.wake_after(Duration::from_secs_f64(SHOWN_FOR));
        tx.instant();
    }

    pub fn showing(&self) -> bool {
        self.shown_until.is_some()
    }

    /// Hides the indicator once its time is up; true when that changed the view.
    pub fn poll(&mut self, clock: f64, tx: &mut Tx) -> bool {
        match self.shown_until {
            Some(until) if clock + 0.004 >= until => {
                self.shown_until = None;
                true
            }
            Some(until) => {
                tx.wake_at(until);
                false
            }
            None => false,
        }
    }
}

/// `root` holding `body` drawn at the zoom, and the indicator while it shows. `root` must be
/// keyed "root" and fill the window.
pub fn frame(s: &Main, cx: &mut Cx<Main>, p: Palette, root: El, body: El) -> El {
    let size = cx.layout_size("root");
    let k = size.map_or(s.zoom.level, |size| fitted(s.zoom.level, size));
    let stage = col().key("zoomed").absolute(0.0, 0.0).scale(k).origin(0.0, 0.0);
    let stage = match size {
        Some([w, h]) => stage.size(w / k, h / k),
        None => stage.w_full().h_full(),
    };
    let mut root = root.child(stage.child(body));
    if s.zoom.showing() {
        root = root.child(indicator(p, s.zoom.level, k));
    }
    root
}

fn indicator(p: Palette, level: f32, shown: f32) -> El {
    let label = if percent(shown) == percent(level) {
        format!("{}%", percent(level))
    } else {
        format!("{}% · {}% fits this window", percent(level), percent(shown))
    };
    row()
        .key("zoom-indicator")
        .pin(Some(0.0), Some(14.0), Some(0.0), None)
        .justify_center()
        .transition(Transition::opacity())
        .child(
            row()
                .key("zoom-level")
                .role("status")
                .label(format!("Zoom {label}"))
                .h(30.0)
                .px(14.0)
                .center()
                .radius(15.0)
                .fill(p.toast)
                .shadow(4.0, 18.0, p.shadow)
                .child(text(label).font(font::BUTTON).color(p.toast_text)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zooming_in_stops_where_the_layout_would_get_too_small() {
        assert_eq!(fitted(2.0, [LAYOUT_MIN.0 * 2.5, LAYOUT_MIN.1 * 2.5]), 2.0);
        assert_eq!(fitted(2.0, [LAYOUT_MIN.0 * 1.5, LAYOUT_MIN.1 * 3.0]), 1.5);
        assert_eq!(fitted(1.25, [LAYOUT_MIN.0, LAYOUT_MIN.1]), 1.0);
        assert_eq!(fitted(0.8, [LAYOUT_MIN.0, LAYOUT_MIN.1]), 0.8);
    }

    #[test]
    fn a_saved_level_snaps_to_the_nearest_step() {
        assert_eq!(nearest_step(1.3), 1.25);
        assert_eq!(nearest_step(9.0), 2.0);
        assert_eq!(nearest_step(0.1), 0.8);
    }
}
