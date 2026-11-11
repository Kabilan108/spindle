//! Spindle visualization lab: one scene per concept, driven by a simulated audio signal and
//! rendered headlessly by Rune's own tooling. See README.md for the commands.

mod art;
mod chrome;
mod gfx;
#[allow(dead_code)]
mod icons;
mod scenes;
mod signal;
#[allow(dead_code)]
mod theme;

use std::collections::HashMap;
use std::sync::Mutex;

use rune::{AppSpec, Cx, El, Live, LiveText, Pixels, Tx};

use art::{ArtPalette, COVER_PX, Cover};
use signal::{AudioFrame, Feed, SimTrack};

/// Video clock seconds at which every scene pauses. The track plays from `START_POS`: a bar of
/// verse, a four-second build, then the drop.
pub const PAUSE_AT: f64 = 8.0;
pub const START_POS: f64 = 50.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scene {
    #[default]
    Signal,
    Groundline,
    Trace,
    Sleeve,
    Aura,
    EdgeLight,
    Header,
    Stage,
}

pub struct Arts {
    pixels: HashMap<Cover, Pixels>,
    palettes: HashMap<Cover, ArtPalette>,
    pub backdrop: HashMap<Cover, Pixels>,
}

impl Arts {
    fn new() -> Arts {
        let all = [Cover::CarrierWave, Cover::LowTide, Cover::ConcreteGarden, Cover::Gradient(0), Cover::Gradient(1), Cover::Gradient(2)];
        let mut arts = Arts { pixels: HashMap::new(), palettes: HashMap::new(), backdrop: HashMap::new() };
        for c in all {
            let rgba = c.render();
            arts.palettes.insert(c, ArtPalette::from_rgba(&rgba, COVER_PX, COVER_PX));
            let thumb = art::blurred_thumb(&rgba, COVER_PX, 96);
            let bd = Pixels::new();
            bd.set(96, 96, thumb);
            arts.backdrop.insert(c, bd);
            let px = Pixels::new();
            px.set(COVER_PX, COVER_PX, rgba);
            arts.pixels.insert(c, px);
        }
        arts
    }

    pub fn pixels(&self, c: Cover) -> &Pixels {
        &self.pixels[&c]
    }

    pub fn palette(&self, c: Cover) -> ArtPalette {
        self.palettes[&c]
    }
}

/// What each concept costs, counted while the scene runs and printed after a headless run.
#[derive(Default, Debug)]
pub struct Stats {
    pub commits: u32,
    pub live_sets: u32,
    pub last_change: f64,
    pub paused_commits: u32,
    pub paused_sets: u32,
}

pub static STATS: Mutex<Stats> = Mutex::new(Stats { commits: 0, live_sets: 0, last_change: 0.0, paused_commits: 0, paused_sets: 0 });

pub fn publish<T: rune::live::LiveValue>(live: &Live<T>, value: T, t: f64) {
    if live.set(value) {
        let mut st = STATS.lock().unwrap();
        st.live_sets += 1;
        st.last_change = st.last_change.max(t);
        if t >= PAUSE_AT {
            st.paused_sets += 1;
        }
    }
}

pub struct State {
    pub scene: Scene,
    pub t: f64,
    pub feed: Feed<SimTrack>,
    pub frame: AudioFrame,
    pub track: Cover,
    pub arts: Arts,
    pub progress: Live<[f32; 4]>,
    pub elapsed: LiveText,
    pub was_playing: bool,
    pub frames: u64,
    pub sc: scenes::SceneState,
}

impl State {
    pub fn playing(&self) -> bool {
        self.t < PAUSE_AT
    }

    pub fn position(&self) -> f64 {
        START_POS + self.t.min(PAUSE_AT)
    }

    pub fn palette(&self) -> ArtPalette {
        self.arts.palette(self.track)
    }
}

fn init() -> State {
    State {
        scene: Scene::Signal,
        t: 0.0,
        feed: Feed::new(SimTrack::default()),
        frame: AudioFrame::SILENT,
        track: Cover::CarrierWave,
        arts: Arts::new(),
        progress: Live::new([0.0, 0.0, 0.0, 3.0]),
        elapsed: LiveText::new("0:50"),
        was_playing: true,
        frames: 0,
        sc: scenes::SceneState::default(),
    }
}

fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    s.t = t;
    s.frames += 1;
    let hops = s.feed.advance(t, |c| (c < PAUSE_AT).then_some(START_POS + c));
    s.frame = s.feed.frame;
    let playing = s.playing();
    let changed = playing != s.was_playing;
    s.was_playing = playing;
    let pos = s.position();
    let secs = pos as u32;
    s.elapsed.set(format!("{}:{:02}", secs / 60, secs % 60));
    let rebuild = scenes::poll(s, hops, changed, tx) || changed;
    if rebuild {
        let mut st = STATS.lock().unwrap();
        st.commits += 1;
        st.last_change = st.last_change.max(t);
        if t >= PAUSE_AT {
            st.paused_commits += 1;
        }
    }
    rebuild
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    scenes::view(s, cx)
}

fn main() {
    let app = rune::App::new(AppSpec::<State> {
        scenes: &[
            ("signal", |s| s.scene = Scene::Signal),
            ("groundline", |s| s.scene = Scene::Groundline),
            ("trace", |s| s.scene = Scene::Trace),
            ("sleeve", |s| s.scene = Scene::Sleeve),
            ("aura", |s| {
                s.scene = Scene::Aura;
                s.track = Cover::LowTide;
            }),
            ("edge-light", |s| s.scene = Scene::EdgeLight),
            ("header", |s| {
                s.scene = Scene::Header;
                s.track = Cover::ConcreteGarden;
            }),
            ("stage", |s| {
                s.scene = Scene::Stage;
                s.track = Cover::LowTide;
            }),
        ],
        poll: Some(poll),
        ..AppSpec::new("Spindle viz lab", chrome::MINI_SIZE, init, view)
    })
    .compositor(gfx::Gfx::default());
    app.run();
}
