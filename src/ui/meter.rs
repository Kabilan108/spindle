//! The seek bar, the volume slider and the elapsed time, all drawn through live values: moving
//! them redraws the bar, never the view. While a track plays in a visible window a timer moves
//! them a few times a second; paused or hidden, nothing ticks.

use std::time::{Duration, Instant};

use rune::{Drag, Live, LiveText, Phase, Tx};

use super::link::Link;
use crate::model::Command;

/// A horizontal bar's fill and knob, positioned by fraction once its width is known.
pub struct Bar {
    pub fill: Live<[f32; 4]>,
    pub knob: Live<[f32; 2]>,
    thickness: f32,
    knob_size: f32,
    width: f32,
    fraction: f32,
}

impl Bar {
    pub fn new(thickness: f32, knob_size: f32) -> Self {
        Self {
            fill: Live::new([0.0, 0.0, 0.0, thickness]),
            knob: Live::new([-knob_size / 2.0, 0.0]),
            thickness,
            knob_size,
            width: 0.0,
            fraction: 0.0,
        }
    }

    pub fn set(&mut self, fraction: f32) {
        self.fraction = if fraction.is_finite() { fraction.clamp(0.0, 1.0) } else { 0.0 };
        self.push();
    }

    /// The bar's laid-out width, from `on_layout`.
    pub fn resize(&mut self, width: f32) {
        if (width - self.width).abs() > 0.25 {
            self.width = width;
            self.push();
        }
    }

    pub fn fraction(&self) -> f32 {
        self.fraction
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    fn push(&self) {
        let x = (self.width * self.fraction).round();
        self.fill.set([0.0, 0.0, x, self.thickness]);
        self.knob.set([x - self.knob_size / 2.0, 0.0]);
    }
}

/// Where a pointer on a bar points, as a fraction of it.
pub fn drag_fraction(d: &Drag) -> f32 {
    if d.w <= 0.0 { 0.0 } else { (d.x / d.w).clamp(0.0, 1.0) }
}

pub fn fmt_time(ms: u32) -> String {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Seek {
    Idle,
    Dragging { position_ms: u32 },
    /// A seek went out; until the engine reports a position from after it, show this one.
    Sent { position_ms: u32, at: Instant },
}

const SENT_PATIENCE: Duration = Duration::from_secs(2);

pub struct Progress {
    pub bar: Bar,
    pub elapsed: LiveText,
    duration_ms: u32,
    seek: Seek,
    /// The next tick, on `Tx::clock`.
    due: Option<f64>,
}

impl Progress {
    pub fn new(thickness: f32, knob: f32) -> Self {
        Self { bar: Bar::new(thickness, knob), elapsed: LiveText::new("0:00"), duration_ms: 0, seek: Seek::Idle, due: None }
    }

    /// Show the engine's position now and, while it moves and someone can see it, plan the
    /// next tick.
    pub fn sync(&mut self, link: &Link, visible: bool, tx: &mut Tx) {
        let (engine_ms, duration_ms, moving, reported_at) = link.read(|store| {
            let playback = &store.playback;
            let duration = playback.track.as_ref().map_or(0, |t| t.duration_ms);
            (playback.position_now(), duration, playback.playing && !playback.loading, playback.position_at)
        });
        self.duration_ms = duration_ms;
        let position_ms = match self.seek {
            Seek::Dragging { position_ms } => {
                self.show(position_ms);
                self.due = None;
                return;
            }
            Seek::Sent { position_ms, at } if reported_at < at && at.elapsed() < SENT_PATIENCE => {
                let since = if moving { at.elapsed().as_millis() as u32 } else { 0 };
                position_ms.saturating_add(since).min(duration_ms)
            }
            _ => {
                self.seek = Seek::Idle;
                engine_ms
            }
        };
        self.show(position_ms);
        if moving && visible && duration_ms > 0 {
            let delay = self.next_tick_ms(position_ms);
            self.schedule(tx.clock() + f64::from(delay) / 1000.0, tx);
        } else {
            self.due = None;
        }
    }

    /// From `poll`: runs a due tick, or catches up when none is planned (the first frame, a
    /// window shown again).
    pub fn poll(&mut self, link: &Link, visible: bool, clock: f64, tx: &mut Tx) {
        if self.due.is_none_or(|due| clock + 0.004 >= due) {
            self.due = None;
            self.sync(link, visible, tx);
        }
    }

    /// Whether a tick is planned (for tests of the idle rule).
    #[cfg(test)]
    pub fn ticking(&self) -> bool {
        self.due.is_some()
    }

    pub fn drag(&mut self, link: &Link, d: &Drag, tx: &mut Tx) {
        let position_ms = (drag_fraction(d) * self.duration_ms as f32) as u32;
        match d.phase {
            Phase::Down | Phase::Move => {
                self.seek = Seek::Dragging { position_ms };
                self.show(position_ms);
                self.due = None;
            }
            Phase::Up if d.cancelled() => {
                self.seek = Seek::Idle;
                self.sync(link, true, tx);
            }
            Phase::Up => {
                if self.duration_ms > 0 {
                    link.send(Command::Seek { position_ms });
                    self.seek = Seek::Sent { position_ms, at: Instant::now() };
                } else {
                    self.seek = Seek::Idle;
                }
                self.show(position_ms);
            }
        }
    }

    fn show(&mut self, position_ms: u32) {
        let fraction = if self.duration_ms == 0 { 0.0 } else { position_ms as f32 / self.duration_ms as f32 };
        self.bar.set(fraction);
        self.elapsed.set(fmt_time(position_ms));
    }

    /// Ticks land on whole seconds, for the clock, and as often as the bar moves a pixel,
    /// at most four times a second.
    fn next_tick_ms(&self, position_ms: u32) -> u32 {
        let per_point = self.duration_ms as f32 / self.bar.width().max(1.0) / 2.0;
        let step = if per_point < 400.0 { 250 } else if per_point < 800.0 { 500 } else { 1000 };
        step - position_ms % step + 4
    }

    fn schedule(&mut self, due: f64, tx: &mut Tx) {
        if self.due.is_none_or(|current| due < current - 0.001) {
            self.due = Some(due);
            tx.wake_at(due);
        }
    }
}

pub struct Volume {
    pub bar: Bar,
    dragging: bool,
    sent_at: f64,
    before_mute: f32,
}

const VOLUME_EVERY: f64 = 0.08;

impl Default for Volume {
    fn default() -> Self {
        Self { bar: Bar::new(4.0, 12.0), dragging: false, sent_at: f64::NEG_INFINITY, before_mute: 0.7 }
    }
}

impl Volume {
    pub fn sync(&mut self, link: &Link) {
        if !self.dragging {
            self.bar.set(link.read(|store| store.playback.volume));
        }
    }

    pub fn drag(&mut self, link: &Link, d: &Drag, tx: &mut Tx) {
        let fraction = drag_fraction(d);
        match d.phase {
            Phase::Down | Phase::Move => {
                self.dragging = true;
                self.bar.set(fraction);
                if tx.clock() - self.sent_at >= VOLUME_EVERY {
                    self.sent_at = tx.clock();
                    link.send(Command::SetVolume(fraction));
                }
            }
            Phase::Up => {
                self.dragging = false;
                if d.cancelled() {
                    self.sync(link);
                } else {
                    link.send(Command::SetVolume(fraction));
                }
            }
        }
    }

    pub fn toggle_mute(&mut self, link: &Link) {
        let volume = link.read(|store| store.playback.volume);
        if volume > 0.0 {
            self.before_mute = volume;
            link.send(Command::SetVolume(0.0));
        } else {
            link.send(Command::SetVolume(self.before_mute.max(0.2)));
        }
    }
}
