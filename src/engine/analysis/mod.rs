//! Audio analysis for the visualizers: band levels, bass, mid, treble, loudness and kicks.
//!
//! Nothing runs on its own. The UI calls [`Analysis::step`] from its poll while a visualizer is
//! on screen and something plays, and maps the frame onto live values; the store and its
//! listeners never hear about it. A [`Source`] supplies band powers: the Spotify engine's sink
//! tap, or a simulated track in the demo.

mod analyzer;
mod sim;
mod spectrum;
#[cfg(feature = "spotify")]
pub(super) mod tap;

use std::sync::{Arc, Mutex, PoisonError};

pub use analyzer::{AudioFrame, BANDS, HOP};
use analyzer::Analyzer;

/// Where band powers come from.
pub(crate) trait Source: Send {
    /// Linear band powers of what the listener hears now, `dt` seconds after the last call.
    /// False when nothing is heard. `playing` is the store's play state.
    fn bands(&mut self, dt: f64, playing: bool, out: &mut [f32; BANDS]) -> bool;
}

struct Silent;

impl Source for Silent {
    fn bands(&mut self, _dt: f64, _playing: bool, _out: &mut [f32; BANDS]) -> bool {
        false
    }
}

/// The demo's stand-in for a sound card: a synthetic track that advances while the store says
/// it plays.
pub(crate) struct Simulated {
    track: sim::SimTrack,
    position: f64,
}

impl Default for Simulated {
    fn default() -> Self {
        // A bar of verse, then the build and the drop at 56 s.
        Self { track: sim::SimTrack::default(), position: 50.0 }
    }
}

impl Source for Simulated {
    fn bands(&mut self, dt: f64, playing: bool, out: &mut [f32; BANDS]) -> bool {
        if !playing {
            return false;
        }
        self.position = (self.position + dt) % sim::SimTrack::DURATION;
        self.track.bands_at(self.position, out);
        true
    }
}

/// Steps closer together than this are skipped; the display can't show them.
const MIN_STEP: f64 = 0.015;
/// After a longer gap (nothing was listening), or a clock that went back, the analysis restarts
/// rather than catching up.
const MAX_GAP: f64 = 0.5;

struct Feed {
    source: Box<dyn Source>,
    analyzer: Analyzer,
    frame: AudioFrame,
    clock: Option<f64>,
    power: [f32; BANDS],
}

/// A cheap handle; clones share one analysis.
#[derive(Clone)]
pub struct Analysis {
    feed: Arc<Mutex<Feed>>,
}

impl Analysis {
    pub(super) fn new() -> Self {
        let feed = Feed { source: Box::new(Silent), analyzer: Analyzer::default(), frame: AudioFrame::SILENT, clock: None, power: [0.0; BANDS] };
        Self { feed: Arc::new(Mutex::new(feed)) }
    }

    pub(super) fn set_source(&self, source: Box<dyn Source>) {
        self.feed.lock().unwrap_or_else(PoisonError::into_inner).source = source;
    }

    /// Analyse what is heard at `clock` (seconds on the caller's clock). `None` when the last
    /// step was too recent to bother.
    pub fn step(&self, clock: f64, playing: bool) -> Option<AudioFrame> {
        let mut feed = self.feed.lock().unwrap_or_else(PoisonError::into_inner);
        let dt = match feed.clock {
            Some(last) if (0.0..MIN_STEP).contains(&(clock - last)) => return None,
            Some(last) if (MIN_STEP..=MAX_GAP).contains(&(clock - last)) => clock - last,
            _ => HOP,
        };
        feed.clock = Some(clock);
        let Feed { source, analyzer, power, .. } = &mut *feed;
        let heard = source.bands(dt, playing, power);
        let frame = analyzer.step(heard.then_some(&*power), dt as f32);
        feed.frame = frame;
        Some(frame)
    }

    /// Every level has fallen to zero: nothing more will move until something plays.
    pub fn settled(&self) -> bool {
        self.feed.lock().unwrap_or_else(PoisonError::into_inner).frame.is_silent()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulated() -> Analysis {
        let analysis = Analysis::new();
        analysis.set_source(Box::new(Simulated::default()));
        analysis
    }

    #[test]
    fn a_playing_track_moves_the_levels_and_kicks() {
        let analysis = simulated();
        let mut onsets = 0;
        let mut loudest: f32 = 0.0;
        for hop in 0..(12.0 / HOP) as usize {
            let frame = analysis.step(hop as f64 * HOP, true).expect("a hop apart");
            loudest = loudest.max(frame.loudness);
            onsets += usize::from(frame.onset > 0.0);
        }
        assert!(loudest > 0.3, "loudness peaked at {loudest}");
        assert!(onsets >= 8, "{onsets} kicks in 12 s of a 120 BPM track");
    }

    #[test]
    fn levels_settle_to_zero_after_a_pause() {
        let analysis = simulated();
        let mut clock = 0.0;
        for _ in 0..100 {
            clock += HOP;
            analysis.step(clock, true);
        }
        assert!(!analysis.settled());
        for _ in 0..(3.0 / HOP) as usize {
            clock += HOP;
            analysis.step(clock, false);
        }
        assert!(analysis.settled(), "still moving three seconds after the pause");
    }

    #[test]
    fn steps_closer_than_a_frame_are_skipped() {
        let analysis = simulated();
        assert!(analysis.step(1.0, true).is_some());
        assert!(analysis.step(1.005, true).is_none());
        assert!(analysis.step(1.03, true).is_some());
        assert!(analysis.step(0.2, true).is_some(), "a new clock starts over");
    }
}
