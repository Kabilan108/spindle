//! One module per concept. Each has a `view` (run only when the scene asks for a commit) and a
//! `poll` that publishes live values or compositor numbers and says whether to commit.

mod aura;
mod edge;
mod groundline;
mod header;
mod signal_debug;
mod sleeve;
mod stage;
mod trace;

use rune::{Anim, AnimExt, Curve, Cx, Easing, El, Live, Tx};

use crate::signal::BANDS;
use crate::theme::Palette;
use crate::{Scene, State};

/// Compositor-side numbers, advanced every frame by the poll; the compositor reads them.
#[derive(Default)]
pub struct FxState {
    /// Effect clock for drifting fills: runs faster when the music is louder, stops when paused.
    pub time: f32,
    pub speed: f32,
    /// Glow heat 0..1 (rune-fx cools a glow toward 0 instead of cutting it).
    pub heat: f32,
    /// Beat ripples: (start time, strength).
    pub shots: Vec<(f32, f32)>,
}

/// A renderer-driven spin: one commit starts a long linear turn, one commit at pause eases it to
/// a stop. Rune has no live rotation, so this is how a disc turns without a commit per frame.
#[derive(Clone, Copy)]
pub struct Spin {
    pub period: f32,
    pub started: Option<(f64, f32)>,
    pub target: f32,
    pub anim: Anim,
}

impl Spin {
    pub fn new(period: f32) -> Spin {
        Spin { period, started: None, target: 0.0, anim: Anim::INSTANT }
    }

    fn omega(&self) -> f32 {
        std::f32::consts::TAU / self.period
    }

    /// The angle the renderer is presenting at `t` while spinning.
    pub fn angle(&self, t: f64) -> f32 {
        match self.started {
            Some((t0, a0)) => a0 + self.omega() * (t - t0) as f32,
            None => self.target,
        }
    }

    pub fn start(&mut self, t: f64) {
        const RUN: f32 = 240.0;
        let a0 = self.target;
        self.started = Some((t, a0));
        self.target = a0 + self.omega() * RUN;
        self.anim = Anim::new(Curve::linear(RUN)).merge();
    }

    /// Ease to a stop over `secs`, leaving at the current angular velocity. The curve's initial
    /// slope is 2, so travelling ω·T/2 starts at exactly ω.
    pub fn stop(&mut self, t: f64, secs: f32) {
        let now = self.angle(t);
        self.started = None;
        self.target = now + self.omega() * secs / 2.0;
        self.anim = Anim::new(Curve::tween(secs, Easing::cubic(0.3, 0.6, 0.6, 1.0))).merge();
    }
}

pub struct SceneState {
    pub inited: bool,
    pub bars: Vec<Live<[f32; 4]>>,
    pub levels: Vec<Live<f32>>,
    pub offsets: Vec<Live<[f32; 2]>>,
    pub fx: FxState,
    pub spin: Spin,
    pub trace: Vec<f32>,
    pub trace_col: usize,
    pub trace_acc: (f32, u32),
    pub drift: f64,
    pub radial: [f32; BANDS],
    pub last_fx_t: f64,
}

impl Default for SceneState {
    fn default() -> Self {
        SceneState {
            inited: false,
            bars: (0..96).map(|_| Live::new([0.0; 4])).collect(),
            levels: (0..16).map(|_| Live::new(0.0)).collect(),
            offsets: (0..8).map(|_| Live::new([0.0; 2])).collect(),
            fx: FxState::default(),
            spin: Spin::new(1.8),
            trace: Vec::new(),
            trace_col: 0,
            trace_acc: (0.0, 0),
            drift: 0.0,
            radial: [0.0; BANDS],
            last_fx_t: 0.0,
        }
    }
}

/// Smoothly follow `target` with separate rise and fall time constants; lands exactly on 0.
pub fn follow(v: f32, target: f32, dt: f32, rise: f32, fall: f32) -> f32 {
    let tau = if target > v { rise } else { fall };
    let next = v + (target - v) * (1.0 - (-dt / tau.max(1e-4)).exp());
    if target == 0.0 && next < 1e-3 { 0.0 } else { next }
}

/// Quarter-point steps, so a settled value repeats exactly and stops waking the window.
pub fn q(v: f32) -> f32 {
    (v * 4.0).round() / 4.0
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let _ = cx;
    let p = Palette::new(true);
    match s.scene {
        Scene::Signal => signal_debug::view(s, p),
        Scene::Groundline => groundline::view(s, p),
        Scene::Trace => trace::view(s, p),
        Scene::Sleeve => sleeve::view(s, p),
        Scene::Aura => aura::view(s, p),
        Scene::EdgeLight => edge::view(s, p),
        Scene::Header => header::view(s, p),
        Scene::Stage => stage::view(s, p),
    }
}

pub fn poll(s: &mut State, hops: usize, changed: bool, tx: &mut Tx) -> bool {
    let progress_w = match s.scene {
        Scene::EdgeLight | Scene::Header => crate::chrome::SEEK_W,
        Scene::Stage => stage::SEEK_W,
        _ => crate::chrome::MINI_SIZE.0 - 2.0 * crate::chrome::MINI_PAD,
    };
    let h = if matches!(s.scene, Scene::EdgeLight | Scene::Header | Scene::Stage) { 4.0 } else { 3.0 };
    let frac = (s.position() / 261.0) as f32;
    crate::publish(&s.progress, [0.0, 0.0, q(progress_w * frac), h], s.t);
    let first = !s.sc.inited;
    s.sc.inited = true;
    match s.scene {
        Scene::Signal => hops > 0,
        Scene::Groundline => groundline::poll(s, hops),
        Scene::Trace => trace::poll(s, hops, first),
        Scene::Sleeve => sleeve::poll(s, changed, first, tx),
        Scene::Aura => aura::poll(s),
        Scene::EdgeLight => edge::poll(s, hops),
        Scene::Header => header::poll(s, hops),
        Scene::Stage => stage::poll(s, hops, changed, first),
    }
}

/// Advance the compositor numbers by one frame: drift speed follows loudness, glow heat follows
/// the bass, strong onsets start ripples. Everything settles to zero after a pause.
pub fn step_fx(s: &mut State, ripple_threshold: f32) {
    let dt = (s.t - s.sc.last_fx_t).clamp(0.0, 0.1) as f32;
    s.sc.last_fx_t = s.t;
    let playing = s.playing();
    let fr = s.frame;
    let fx = &mut s.sc.fx;
    let speed_target = if playing { 0.4 + 1.6 * fr.loudness } else { 0.0 };
    fx.speed = follow(fx.speed, speed_target, dt, 0.3, 0.45);
    if !playing && fx.speed < 0.01 {
        fx.speed = 0.0;
    }
    fx.time += dt * fx.speed;
    let heat_target = if playing { (0.25 + 0.75 * fr.bass).min(1.0) } else { 0.0 };
    fx.heat = follow(fx.heat, heat_target, dt, 0.04, if playing { 0.22 } else { 0.5 });
    if !playing && fx.heat < 0.02 {
        fx.heat = 0.0;
    }
    let t = s.t as f32;
    if playing && fr.onset >= ripple_threshold && fr.since_onset == 0.0 && fx.shots.last().is_none_or(|l| t - l.0 > 0.2) {
        fx.shots.push((t, fr.onset));
    }
    fx.shots.retain(|sh| t - sh.0 < crate::gfx::RIPPLE_LIFE);
    if fx.speed > 0.0 || fx.heat > 0.0 || !fx.shots.is_empty() {
        let mut st = crate::STATS.lock().unwrap();
        st.last_change = st.last_change.max(s.t);
    }
}
