//! The mini player: a small borderless window with the track, transport, a thin progress bar
//! and the latest recents as tiles. On Wayland the compositor places and floats it by title.

use std::cell::Cell;

use rune::{AppHandle, Cursor, Cx, El, Key, KeyEvent, Tx, Waker, col, el, row, spacer, text};

use super::art::Art;
use super::link::Link;
use super::meter::Progress;
use super::theme::{Palette, font};
use super::widgets::{art, fallback_icon, icon_button};
use super::{Main, chord, icons, is_plain_space};
use crate::model::{Command, ContextSummary, Track};

pub const MINI_TITLE: &str = "Spindle Mini Player";
pub const MINI_SIZE: (f32, f32) = (380.0, 208.0);
const PAD: f32 = 12.0;
const TILES: usize = 6;
const TILE_GAP: f32 = 8.0;

pub struct Mini {
    pub link: Link,
    art: Art,
    pub rev: u64,
    pub progress: Progress,
    me: Option<AppHandle<Mini>>,
    main: Option<AppHandle<Main>>,
    dark: Cell<bool>,
}

impl Mini {
    pub fn new(link: Link) -> Self {
        Self { link, art: Art::default(), rev: 0, progress: Progress::new(3.0, 0.0), me: None, main: None, dark: Cell::new(true) }
    }

    pub fn with_handles(mut self, me: AppHandle<Mini>, main: AppHandle<Main>) -> Self {
        self.me = Some(me);
        self.main = Some(main);
        self
    }

    fn visible(&self) -> bool {
        rune::is_headless() || SHOWN.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn store_changed(&mut self, tx: &mut Tx) {
        self.rev += 1;
        if tx.clock() < 1.0 {
            tx.instant();
        }
        self.resume(tx);
    }

    pub fn resume(&mut self, tx: &mut Tx) {
        let visible = self.visible();
        self.progress.sync(&self.link, visible, tx);
    }

    fn open_main(&self) {
        if let Some(main) = &self.main {
            main.open();
            main.update(|m, tx| m.resume(tx));
        }
    }
}

fn mini_playing(s: &mut Mini) {
    s.link.send_and_wait(Command::PlayPause, |store| store.playback.playing);
}

fn mini_idle(s: &mut Mini) {
    s.link.send_and_wait(Command::PlayContext { uri: "spotify:album:demo4".into(), track_uri: None }, |store| {
        store.recents.first().is_some_and(|r| r.uri == "spotify:album:demo4")
    });
    s.link.send_and_wait(Command::PlayPause, |store| !store.playback.playing);
}

pub const MINI_SCENES: &[super::Scene<Mini>] = &[("default", |_| {}), ("playing", mini_playing), ("switched", mini_idle)];

pub fn mini_setup(s: &mut Mini, waker: Waker) {
    s.art.set_waker(waker);
}

pub fn mini_poll(s: &mut Mini, clock: f64, tx: &mut Tx) -> bool {
    let visible = s.visible();
    s.progress.poll(&s.link, visible, clock, tx);
    false
}

pub fn mini_key(s: &mut Mini, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if is_plain_space(ev) {
        s.link.send(Command::PlayPause);
    } else if chord("mod+right", ev, tx) {
        s.link.send(Command::Next);
    } else if chord("mod+left", ev, tx) {
        s.link.send(Command::Previous);
    } else if chord("mod+w", ev, tx) || ev.key == Key::Escape {
        tx.close_window();
    } else if chord("mod+q", ev, tx) {
        tx.quit();
    } else {
        return false;
    }
    true
}

struct Facts {
    track: Option<Track>,
    playing: bool,
    liked: bool,
    recents: Vec<ContextSummary>,
}

pub fn mini_view(s: &Mini, cx: &mut Cx<Mini>) -> El {
    let p = Palette::new(cx.system_dark());
    s.dark.set(p.dark);
    s.art.begin_view();
    cx.focus_ring(2.0, 1.0, p.focus);
    let f = s.link.read(|store| {
        let playback = &store.playback;
        Facts {
            liked: playback.track.as_ref().is_some_and(|t| store.liked.contains(&t.uri)),
            track: playback.track.clone(),
            playing: playback.playing,
            recents: store.recents.iter().take(TILES).cloned().collect(),
        }
    });
    col()
        .key("mini")
        .w_full()
        .h_full()
        .padding(PAD)
        .gap(10.0)
        .fill(p.bg)
        .border(1.0, p.line)
        .on_key(mini_key)
        .child(top(s, p, &f))
        .child(controls(p, &f))
        .child(progress(s, p))
        .child(tiles(s, p, &f.recents))
}

fn top(s: &Mini, p: Palette, f: &Facts) -> El {
    let (title, artists) = match &f.track {
        Some(t) => (t.name.clone(), t.artist_line()),
        None => ("Nothing playing".to_owned(), "Pick something below".to_owned()),
    };
    let image = f.track.as_ref().and_then(|t| t.image_url.as_deref());
    row()
        .key("mini-top")
        .h(64.0)
        .gap(12.0)
        .center_y()
        .window_drag()
        .child(art(&s.art, &s.link, image, 64.0, 6.0, p, &icons::NOTE))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(3.0)
                .child(text(title).key("mini-title").font(font::CARD_TITLE).color(p.text).truncate())
                .child(text(artists).font(font::SMALL).color(p.muted).truncate()),
        )
        .child(
            col()
                .self_start()
                .gap(2.0)
                .child(icon_button("mini-open", "Open Spindle", &icons::EXPAND, 26.0, p.muted, p).on_press(|s: &mut Mini| s.open_main()))
                .child(icon_button("mini-hide", "Hide mini player", &icons::CLOSE, 26.0, p.muted, p).on_press_tx(|_: &mut Mini, tx: &mut Tx| tx.close_window())),
        )
}

fn controls(p: Palette, f: &Facts) -> El {
    let (label, glyph) = if f.playing { ("Pause", &icons::PAUSE) } else { ("Play", &icons::PLAY) };
    let like = match &f.track {
        Some(t) => {
            let (uri, liked) = (t.uri.clone(), f.liked);
            let heart = if liked { &icons::HEART_FILLED } else { &icons::HEART };
            icon_button("mini-like", if liked { "Remove from Liked Songs" } else { "Save to Liked Songs" }, heart, 30.0, if liked { p.accent } else { p.muted }, p)
                .on_press(move |s: &mut Mini| s.link.send(Command::SetLiked { uri: uri.clone(), liked: !liked }))
        }
        None => el().size(30.0, 30.0),
    };
    let play = el()
        .key("mini-play")
        .role("button")
        .label(label)
        .size(34.0, 34.0)
        .radius(17.0)
        .center()
        .fill(p.text)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .hover(p.bg.alpha(0.12))
        .press(p.bg.alpha(0.2))
        .on_press(|s: &mut Mini| s.link.send(Command::PlayPause))
        .child(rune::icon(glyph, 16.0).color(p.bg));
    row()
        .key("mini-controls")
        .h(34.0)
        .center_y()
        .child(like)
        .child(spacer())
        .child(icon_button("mini-previous", "Previous", &icons::PREVIOUS, 32.0, p.text, p).on_press(|s: &mut Mini| s.link.send(Command::Previous)))
        .child(play.mx(10.0))
        .child(icon_button("mini-next", "Next", &icons::NEXT, 32.0, p.text, p).on_press(|s: &mut Mini| s.link.send(Command::Next)))
        .child(spacer())
        .child(el().size(30.0, 30.0))
}

fn progress(s: &Mini, p: Palette) -> El {
    el().key("mini-progress")
        .w_full()
        .h(3.0)
        .radius(1.5)
        .fill(p.line)
        .role("progressbar")
        .on_layout_tx(|s: &mut Mini, r: [f32; 4], tx: &mut Tx| {
            s.progress.bar.resize(r[2]);
            tx.live_only();
        })
        .child(el().key("mini-fill").absolute(0.0, 0.0).size(0.0, 3.0).radius(1.5).fill(p.accent).live_rect(&s.progress.bar.fill))
}

fn tiles(s: &Mini, p: Palette, recents: &[ContextSummary]) -> El {
    let size = ((MINI_SIZE.0 - 2.0 * PAD - TILE_GAP * (TILES as f32 - 1.0)) / TILES as f32).floor();
    let mut strip = row().key("mini-recents").gap(TILE_GAP);
    if recents.is_empty() {
        return strip.h(size).center_y().child(text("Recently played shows up here.").font(font::SMALL).color(p.faint));
    }
    for item in recents {
        let uri = item.uri.clone();
        strip = strip.child(
            art(&s.art, &s.link, item.image_url.as_deref(), size, 5.0, p, fallback_icon(item.kind))
                .key(format!("tile:{}", item.uri))
                .role("button")
                .label(format!("Play {}", item.name))
                .help(item.name.clone())
                .cursor(Cursor::Pointer)
                .focusable(true)
                .hover(p.hover)
                .press_scale(0.95)
                .on_press(move |s: &mut Mini| s.link.send(Command::PlayContext { uri: uri.clone(), track_uri: None })),
        );
    }
    strip
}

/// Whether the mini player is on screen, tracked here because `AppHandle::is_visible` goes
/// stale for a secondary window once Wayland has destroyed and recreated it.
static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Record that the window opened or closed by some other route (launch flag, its own close).
pub fn set_shown(shown: bool) {
    SHOWN.store(shown, std::sync::atomic::Ordering::Release);
}

/// Show the mini player, with focus when a tray host handed over an activation token.
pub fn show(mini: &AppHandle<Mini>, token: Option<String>) {
    set_shown(true);
    match token {
        Some(token) => mini.activate(token),
        None => mini.open(),
    }
    mini.update(|m, tx| m.resume(tx));
}

/// Show the mini player if it's hidden, hide it if it's showing.
pub fn toggle(mini: &AppHandle<Mini>, token: Option<String>) {
    if SHOWN.swap(false, std::sync::atomic::Ordering::AcqRel) {
        mini.hide();
    } else {
        show(mini, token);
    }
}
