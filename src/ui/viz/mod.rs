//! The visualizers: the ambient album header and the Stage, the full-window player.
//!
//! Both read the engine's audio analysis through [`poll`], which steps it about every 23 ms
//! while one of them is on screen in a visible window and something plays, and publishes the
//! result as live values. Paused and settled, hidden, or with neither on screen, nothing is
//! stepped and nothing wakes.

pub mod cover;
pub mod header;
pub mod stage;
#[cfg(test)]
mod tests;

use std::time::Duration;

use rune::{Icon, Tx};

use super::{Main, Route};
use crate::engine::analysis::{Analysis, HOP};
use crate::model::{ContextKind, Loadable, Playback};
use cover::{ArtPalette, Covers};
use header::Header;
use stage::Stage;

pub static EXPAND: Icon = Icon {
    name: "baliset-expand",
    view: 24.0,
    body: r#"<path d="M14 4.5h5.5V10M10 19.5H4.5V14M19.5 4.5l-6 6M4.5 19.5l6-6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

pub static COLLAPSE: Icon = Icon {
    name: "baliset-collapse",
    view: 24.0,
    body: r#"<path d="m6 9.5 6 6 6-6" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

/// Quarter-point steps, so a settled value repeats exactly and stops waking the window.
pub fn quarter(v: f32) -> f32 {
    (v * 4.0).round() / 4.0
}

/// Sound is coming out, or about to.
pub fn playing_now(playback: &Playback) -> bool {
    playback.playing && !playback.loading
}

pub struct Viz {
    pub stage_open: bool,
    pub header: Header,
    pub stage: Stage,
    pub covers: Covers,
    analysis: Analysis,
    /// The clock of the last analysis step.
    stepped: Option<f64>,
    /// The clock of the last poll, for the disc's turn when the Stage closes.
    clock: f64,
}

impl Viz {
    pub fn new(analysis: Analysis) -> Self {
        Self { stage_open: false, header: Header::default(), stage: Stage::default(), covers: Covers::default(), analysis, stepped: None, clock: 0.0 }
    }

    pub fn open_stage(&mut self) {
        self.stage_open = true;
    }

    pub fn close_stage(&mut self) {
        if self.stage_open {
            self.stage_open = false;
            self.stage.rest(self.clock);
        }
    }
}

/// The open page's summary when it gets the ambient header: a loaded album or playlist with a
/// cover.
fn header_page(s: &Main) -> Option<(String, ContextKind)> {
    let Route::Context(uri) = &s.route else { return None };
    s.link.read(|store| match store.pages.get(uri) {
        Some(Loadable::Ready(page)) if matches!(page.summary.kind, ContextKind::Album | ContextKind::Playlist) => {
            page.summary.image_url.clone().map(|url| (url, page.summary.kind))
        }
        _ => None,
    })
}

/// The palette for an album or playlist page's ambient header, once its cover is analysed.
pub fn header_palette(s: &Main, image_url: Option<&str>, kind: ContextKind) -> Option<ArtPalette> {
    if !matches!(kind, ContextKind::Album | ContextKind::Playlist) {
        return None;
    }
    s.viz.covers.look(&s.link, image_url?, s.me.as_ref()).map(|look| look.palette)
}

/// Steps the analysis and publishes it while a visualizer can be seen. True when the view must
/// commit (the disc starts or stops turning).
pub fn poll(s: &mut Main, clock: f64, tx: &mut Tx) -> bool {
    s.viz.clock = clock;
    let playing = s.link.read(|store| playing_now(&store.playback));
    let stage = s.viz.stage_open;
    let commit = stage && s.viz.stage.spin(clock, playing);
    let header = !stage && header_page(s).is_some();
    if !s.visible() || !(stage || header) {
        s.viz.stepped = None;
        return commit;
    }
    if let Some(frame) = s.viz.analysis.step(clock, playing) {
        let dt = s.viz.stepped.map_or(HOP, |last| (clock - last).min(0.1));
        s.viz.stepped = Some(clock);
        if stage {
            s.viz.stage.apply(&frame, clock, playing);
        } else {
            s.viz.header.apply(&frame, dt, playing);
        }
    }
    if playing || !s.viz.analysis.settled() {
        tx.wake_after(Duration::from_secs_f64(HOP));
    }
    commit
}
