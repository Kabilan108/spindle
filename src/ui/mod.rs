//! Baliset's windows: the main window and what it is built from.

mod art;
mod bar;
mod home;
pub mod icons;
mod link;
mod meter;
mod page;
mod scenes;
mod search;
mod session;
mod sidebar;
pub mod theme;
mod viz;
mod widgets;

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::time::Duration;

use rune::{AppHandle, Chord, Cx, El, Key, KeyEvent, TextEdit, Tx, Waker, col, el, row};

pub use link::Link;
pub use scenes::MAIN_SCENES;

use crate::model::{Command, LIKED_URI, Loadable, Repeat, SessionState};
use art::Art;
use meter::{Progress, Volume};
use theme::Palette;

/// A named state for headless snapshots.
pub type Scene<S> = (&'static str, fn(&mut S));

pub const MAIN_TITLE: &str = "Baliset";
pub const MAIN_SIZE: (f32, f32) = (1100.0, 720.0);
pub const MAIN_MIN: (f32, f32) = (860.0, 560.0);

const SEARCH_DEBOUNCE: f64 = 0.25;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Home,
    Search,
    /// A playlist, album, artist or Liked Songs, by URI.
    Context(String),
}

/// How the home grid is cut into rows; the view works it out from the pane's width and the
/// virtual rows read it back.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HomeGrid {
    pub cols: usize,
    pub card_w: f32,
    pub recent_rows: usize,
    pub album_rows: usize,
}

pub struct Main {
    pub link: Link,
    art: Art,
    /// Bumped on every store change, so the view runs again.
    pub rev: u64,
    pub route: Route,
    pub search: TextEdit,
    search_due: Option<f64>,
    pub progress: Progress,
    pub volume: Volume,
    /// The selected row of the page on screen.
    pub selected: Option<usize>,
    me: Option<AppHandle<Main>>,
    /// Headless scenes show session states the demo engine never reaches.
    pub preview_session: Option<SessionState>,
    dark: Cell<bool>,
    home: Cell<HomeGrid>,
    /// The sidebar's sign-out button was pressed once and is asking to be pressed again.
    pub confirming_sign_out: bool,
    pub viz: viz::Viz,
}

impl Main {
    pub fn new(link: Link) -> Self {
        let mut volume = Volume::default();
        volume.sync(&link);
        Self {
            viz: viz::Viz::new(link.analysis()),
            link,
            art: Art::default(),
            rev: 0,
            route: Route::Home,
            search: TextEdit::new(""),
            search_due: None,
            progress: Progress::new(4.0, 12.0),
            volume,
            selected: None,
            me: None,
            preview_session: None,
            dark: Cell::new(true),
            home: Cell::new(HomeGrid::default()),
            confirming_sign_out: false,
        }
    }

    /// This window's own handle, for visibility.
    pub fn with_handle(mut self, me: AppHandle<Main>) -> Self {
        self.me = Some(me);
        self
    }

    fn visible(&self) -> bool {
        rune::is_headless() || self.me.as_ref().is_none_or(AppHandle::is_visible)
    }

    /// The engine changed something.
    pub fn store_changed(&mut self, tx: &mut Tx) {
        self.rev += 1;
        if tx.clock() < 1.0 {
            tx.instant();
        }
        let visible = self.visible();
        self.progress.sync(&self.link, visible, tx);
        self.volume.sync(&self.link);
    }

    /// The window came back: catch the bar up and start ticking again.
    pub fn resume(&mut self, tx: &mut Tx) {
        let visible = self.visible();
        self.progress.sync(&self.link, visible, tx);
        self.volume.sync(&self.link);
    }

    pub fn open(&mut self, uri: &str) {
        if uri.is_empty() {
            return;
        }
        let needs_load = self.link.read(|store| matches!(store.pages.get(uri), None | Some(Loadable::Failed(_) | Loadable::Idle)));
        if needs_load {
            self.link.send(Command::LoadContext { uri: uri.to_owned() });
        }
        if self.route != Route::Context(uri.to_owned()) {
            self.route = Route::Context(uri.to_owned());
            self.selected = None;
        }
    }

    /// Sign out of Spotify on this device: stops playback and forgets the saved sign-in.
    pub fn sign_out(&mut self) {
        self.confirming_sign_out = false;
        self.route = Route::Home;
        self.selected = None;
        self.link.send(Command::Logout);
    }

    pub fn go_home(&mut self) {
        self.route = Route::Home;
        self.selected = None;
    }

    pub fn go_search(&mut self, focus: bool) {
        if self.route != Route::Search {
            self.route = Route::Search;
            self.selected = None;
        }
        if focus {
            self.search.focused = true;
        }
    }

    pub fn play_context(&self, uri: &str, track_uri: Option<String>) {
        self.link.send(Command::PlayContext { uri: uri.to_owned(), track_uri });
    }

    /// Plays the context, or pauses and resumes it when it's the one playing.
    pub fn play_or_toggle(&self, uri: &str) {
        let current = self.link.read(|store| store.playback.context_uri.as_deref() == Some(uri) && store.playback.track.is_some());
        if current { self.link.send(Command::PlayPause) } else { self.play_context(uri, None) }
    }

    pub fn set_liked(&self, uri: &str, liked: bool) {
        self.link.send(Command::SetLiked { uri: uri.to_owned(), liked });
    }

    fn search_changed(&mut self, tx: &mut Tx) {
        self.search_due = Some(tx.clock() + SEARCH_DEBOUNCE);
        tx.wake_after(Duration::from_secs_f64(SEARCH_DEBOUNCE));
    }

    fn clear_search(&mut self) {
        self.search.replace_text("");
        self.search_due = None;
        self.link.send(Command::Search { query: String::new() });
    }

    fn cycle_repeat(&self) {
        let next = match self.link.read(|store| store.playback.repeat) {
            Repeat::Off => Repeat::Context,
            Repeat::Context => Repeat::Track,
            Repeat::Track => Repeat::Off,
        };
        self.link.send(Command::SetRepeat(next));
    }

    #[cfg(test)]
    pub(crate) fn art_entries(&self) -> usize {
        self.art.len()
    }
}

pub fn main_setup(s: &mut Main, waker: Waker) {
    s.art.set_waker(waker);
}

pub fn main_poll(s: &mut Main, clock: f64, tx: &mut Tx) -> bool {
    if let Some(due) = s.search_due {
        if clock + 0.004 >= due {
            s.search_due = None;
            s.link.send(Command::Search { query: s.search.text.trim().to_owned() });
        } else {
            tx.wake_at(due);
        }
    }
    let visible = s.visible();
    s.progress.poll(&s.link, visible, clock, tx);
    viz::poll(s, clock, tx)
}

fn chord(spec: &str, ev: &KeyEvent, tx: &Tx) -> bool {
    Chord::parse(spec).is_ok_and(|c| c.matches_tx(ev, tx))
}

fn is_plain_space(ev: &KeyEvent) -> bool {
    let mods = ev.mods;
    matches!(&ev.key, Key::Text(t) if t == " ") && !(mods.cmd || mods.ctrl || mods.alt || mods.logo)
}

/// The window's shortcuts. Text fields keep their own keys; everything else lands here.
pub fn main_key(s: &mut Main, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if is_plain_space(ev) {
        s.link.send(Command::PlayPause);
    } else if chord("mod+l", ev, tx) || chord("mod+k", ev, tx) {
        s.go_search(true);
    } else if chord("mod+right", ev, tx) {
        s.link.send(Command::Next);
    } else if chord("mod+left", ev, tx) {
        s.link.send(Command::Previous);
    } else if matches!(ev.key, Key::Escape) && s.viz.stage_open {
        s.viz.close_stage();
    } else if chord("mod+e", ev, tx) {
        if s.viz.stage_open { s.viz.close_stage() } else { s.viz.open_stage() }
    } else if chord("mod+w", ev, tx) {
        tx.close_window();
    } else if chord("mod+q", ev, tx) {
        tx.quit();
    } else {
        return false;
    }
    true
}

pub fn main_view(s: &Main, cx: &mut Cx<Main>) -> El {
    let p = Palette::new(cx.system_dark());
    s.dark.set(p.dark);
    s.art.begin_view();
    cx.focus_ring(2.0, 1.0, p.focus);
    let session = s.preview_session.clone().unwrap_or_else(|| s.link.read(|store| store.session.clone()));
    let body = match &session {
        SessionState::Starting | SessionState::Connecting => session::connecting(p, matches!(session, SessionState::Connecting)),
        SessionState::NeedsLogin { url, error } => session::sign_in(p, url.as_deref(), error.as_deref()),
        SessionState::Online { .. } | SessionState::Offline { .. } => shell(s, cx, p, &session),
    };
    col().key("root").w_full().h_full().fill(p.bg).on_key(main_key).child(body)
}

fn shell(s: &Main, cx: &mut Cx<Main>, p: Palette, session: &SessionState) -> El {
    if s.viz.stage_open {
        return viz::stage::shell(s);
    }
    let pane = match &s.route {
        Route::Home => home::view(s, cx, p),
        Route::Search => search::view(s, cx, p),
        Route::Context(uri) => page::view(s, cx, p, uri),
    };
    let mut root = col().key("shell").w_full().h_full();
    if let SessionState::Offline { error } = session {
        root = root.child(session::offline_banner(p, error));
    }
    root = root
        .child(
            row()
                .key("panes")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(sidebar::view(s, p))
                .child(el().w(1.0).h_full().fill(p.line))
                .child(col().key("pane").grow(1.0).min_w(0.0).h_full().min_h(0.0).child(pane)),
        )
        .child(bar::view(s, p));
    if let Some(notice) = s.link.read(|store| store.notice.clone()) {
        root = root.child(widgets::toast(p, &notice));
    }
    root
}

/// The URI the sidebar's Liked Songs entry opens.
pub const LIKED: &str = LIKED_URI;
