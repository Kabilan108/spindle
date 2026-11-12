//! The right-click menu on tracks and cards. One menu is open at a time; it is built at the
//! window's root, above everything, from what [`Main::menu`] says was pressed. The Menu key
//! and Shift+F10 open it too: on a track list for its selected row, elsewhere for the focused
//! row or card.

use rune::{Cx, El, PointerPress, Shadow, el};
use rune_kit::{Kit, Menu, MenuItem, context_menu};

use super::theme::{Palette, font};
use super::{Main, icons};
use crate::model::{Command, ContextKind, ContextSummary, Loadable, Store, Track};

pub const KEY: &str = "track-menu";

/// What "Play" does for the track the menu is about.
#[derive(Clone, Debug, PartialEq)]
pub enum Play {
    Context { uri: String, track_uri: String },
    Tracks { uris: Vec<String>, index: usize },
    /// It is in up next: skip to it.
    UpNext { uri: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackTarget {
    pub track: Track,
    pub play: Option<Play>,
    /// The editable playlist page it was opened on, and the row's index there.
    pub in_playlist: Option<(String, usize)>,
    /// Its index among the queued tracks of up next.
    pub in_queue: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Track(Box<TrackTarget>),
    Context(ContextSummary),
}

/// Where the menu opens: at a point in the window, or under a keyed element (the selected
/// row, when the keyboard opened it).
#[derive(Clone, Debug, PartialEq)]
pub enum At {
    Point(f32, f32),
    Below(String),
}

impl From<PointerPress> for At {
    fn from(press: PointerPress) -> Self {
        At::Point(press.x, press.y)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpenMenu {
    pub target: Target,
    pub at: At,
    /// Showing the playlists to add the track to, in place of the actions.
    pub picking: bool,
}

/// The Menu key, or Shift+F10: what opens a context menu from the keyboard.
pub fn is_menu_key(ev: &rune::KeyEvent) -> bool {
    let m = ev.mods;
    let plain = !m.cmd && !m.ctrl && !m.alt && !m.logo;
    match ev.key {
        rune::Key::ContextMenu => plain,
        rune::Key::Function(10) => plain && m.shift,
        _ => false,
    }
}

/// A page's row as a menu target, read from the store when the menu opens.
pub fn page_target(store: &Store, uri: &str, i: usize) -> Option<TrackTarget> {
    let page = store.pages.get(uri)?.ready()?;
    let track = page.tracks.get(i)?.clone();
    let editable = page.summary.kind == ContextKind::Playlist && store.editable.contains(uri);
    Some(TrackTarget {
        play: Some(Play::Context { uri: uri.to_owned(), track_uri: track.uri.clone() }),
        in_playlist: editable.then(|| (uri.to_owned(), i)),
        in_queue: None,
        track,
    })
}

pub fn search_target(store: &Store, i: usize) -> Option<TrackTarget> {
    let tracks = &store.search.results.ready()?.tracks;
    let track = tracks.get(i)?.clone();
    let uris = tracks.iter().map(|t| t.uri.clone()).collect();
    Some(TrackTarget { track, play: Some(Play::Tracks { uris, index: i }), in_playlist: None, in_queue: None })
}

/// A row of up next: `queued` rows first, then the context's.
pub fn up_next_target(store: &Store, queued: bool, i: usize) -> Option<TrackTarget> {
    let list = if queued { &store.up_next.queued } else { &store.up_next.upcoming };
    let track = list.get(i)?.clone();
    Some(TrackTarget { play: Some(Play::UpNext { uri: track.uri.clone() }), in_playlist: None, in_queue: queued.then_some(i), track })
}

pub fn playing_target(store: &Store) -> Option<TrackTarget> {
    let track = store.playback.track.clone()?;
    Some(TrackTarget { track, play: None, in_playlist: None, in_queue: None })
}

impl Main {
    pub fn open_menu(&mut self, target: Option<TrackTarget>, at: At) {
        if let Some(target) = target {
            self.menu = Some(OpenMenu { target: Target::Track(Box::new(target)), at, picking: false });
        }
    }

    pub fn open_card_menu(&mut self, summary: ContextSummary, at: At) {
        self.menu = Some(OpenMenu { target: Target::Context(summary), at, picking: false });
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    /// Open a track's radio: at once if it is known, else once the engine finds it.
    pub fn go_radio(&mut self, track_uri: &str) {
        let known = self.link.read(|store| store.radios.get(track_uri).and_then(Loadable::ready).cloned());
        match known {
            Some(radio) => self.open(&radio),
            None => {
                self.awaiting_radio = Some(track_uri.to_owned());
                self.link.send(Command::LoadRadio { track_uri: track_uri.to_owned() });
            }
        }
    }

    /// The radio asked for has been found, or hasn't: go to it, or stop waiting.
    pub(super) fn radio_arrived(&mut self) {
        let Some(seed) = self.awaiting_radio.clone() else { return };
        match self.link.read(|store| store.radios.get(&seed).cloned()) {
            Some(Loadable::Ready(radio)) => {
                self.awaiting_radio = None;
                self.open(&radio);
            }
            Some(Loadable::Failed(_)) => self.awaiting_radio = None,
            _ => {}
        }
    }

    fn play(&mut self, play: &Play) {
        match play {
            Play::Context { uri, track_uri } => self.play_context(uri, Some(track_uri.clone())),
            Play::Tracks { uris, index } => self.link.send(Command::PlayTracks { uris: uris.clone(), index: *index }),
            Play::UpNext { uri } => self.link.send(Command::SkipTo { uri: uri.clone() }),
        }
    }
}

/// rune-kit's menu in Baliset's colours and type.
fn kit(p: Palette) -> Kit {
    let mut k = Kit::neutral(p.dark);
    k.c.layer_01 = p.raised;
    k.c.layer_02 = p.raised;
    k.c.text_base = p.text;
    k.c.text_muted = p.muted;
    k.c.text_faint = p.faint;
    k.c.icon_base = p.muted;
    k.c.hover = p.press;
    k.c.border_muted = p.line;
    k.c.border_base = p.line;
    k.c.danger.fg = p.danger;
    k.ty.ui_compact = font::BODY;
    k.ty.small_medium = font::CAPS;
    k.elevation.floating = vec![
        Shadow { y: 6.0, blur: 24.0, color: p.shadow, ..Shadow::default() },
        Shadow { spread: 0.5, color: p.line, ..Shadow::default() },
    ];
    k
}

fn item(label: impl Into<String>, glyph: &'static rune::Icon, act: impl Fn(&mut Main) + 'static) -> MenuItem {
    MenuItem::new(label).icon(glyph).on_select(move |s: &mut Main| {
        s.close_menu();
        act(s);
    })
}

fn track_menu(menu: Menu, store: &Store, target: &TrackTarget, picking: bool) -> Menu {
    let track = &target.track;
    let uri = track.uri.clone();
    if picking {
        return playlist_picker(menu, store, &uri);
    }
    let mut menu = menu;
    if let Some(play) = target.play.clone() {
        menu = menu.item(item("Play", &icons::PLAY, move |s| s.play(&play)));
    }
    let queued = uri.clone();
    menu = menu.item(item("Add to queue", &icons::QUEUE, move |s| s.link.send(Command::AddToQueue { uri: queued.clone() })));
    if let Some(index) = target.in_queue {
        let unqueued = uri.clone();
        menu = menu.item(item("Remove from queue", &icons::MINUS, move |s| s.link.send(Command::RemoveFromQueue { index, uri: unqueued.clone() })));
    }
    let album = track.album.as_ref().map(|album| album.uri.clone()).filter(|album| !album.is_empty());
    if uri.starts_with("spotify:episode:") {
        if let Some(show) = album {
            menu = menu.separator().item(item("Go to show", &icons::PODCAST, move |s| s.open(&show)));
        }
        return menu;
    }
    let seed = uri.clone();
    menu = menu.item(item("Go to song radio", &icons::RADIO, move |s| s.go_radio(&seed))).separator();
    menu = menu.item(MenuItem::new("Add to playlist").icon(&icons::PLUS).on_select_tx(|s: &mut Main, tx: &mut rune::Tx| {
        if let Some(menu) = &mut s.menu {
            menu.picking = true;
        }
        tx.focus(format!("{KEY}-item2"), tx.focus_visible());
    }));
    let liked = store.liked.contains(&uri);
    let like_uri = uri.clone();
    let (label, glyph) = if liked { ("Remove from Liked Songs", &icons::HEART_FILLED) } else { ("Save to Liked Songs", &icons::HEART) };
    menu = menu.item(item(label, glyph, move |s| s.set_liked(&like_uri, !liked)));
    if let Some((playlist_uri, index)) = target.in_playlist.clone() {
        let track_uri = uri.clone();
        menu = menu.item(
            item("Remove from this playlist", &icons::MINUS, move |s| {
                s.link.send(Command::RemoveFromPlaylist { playlist_uri: playlist_uri.clone(), track_uri: track_uri.clone(), index });
            })
            .danger(true),
        );
    }
    let artists: Vec<(String, String)> = track.artists.iter().filter(|a| !a.uri.is_empty()).map(|a| (a.uri.clone(), a.name.clone())).take(3).collect();
    if album.is_some() || !artists.is_empty() {
        menu = menu.separator();
    }
    if let Some(album) = album {
        menu = menu.item(item("Go to album", &icons::DISC, move |s| s.open(&album)));
    }
    let several = artists.len() > 1;
    for (artist, name) in artists {
        let label = if several { format!("Go to {name}") } else { "Go to artist".to_owned() };
        menu = menu.item(item(label, &icons::PERSON, move |s| s.open(&artist)));
    }
    menu
}

fn playlist_picker(menu: Menu, store: &Store, track_uri: &str) -> Menu {
    let mut menu = menu.item(MenuItem::new("Back").icon(&icons::BACK).on_select_tx(|s: &mut Main, tx: &mut rune::Tx| {
        if let Some(menu) = &mut s.menu {
            menu.picking = false;
        }
        tx.focus(format!("{KEY}-item0"), tx.focus_visible());
    }));
    menu = menu.group_label("Add to playlist");
    let playlists = store.playlists.ready().map_or(&[][..], Vec::as_slice);
    let mut any = false;
    for playlist in playlists.iter().filter(|playlist| store.editable.contains(&playlist.uri)) {
        any = true;
        let (playlist_uri, track_uri) = (playlist.uri.clone(), track_uri.to_owned());
        menu = menu.item(item(playlist.name.clone(), &icons::LIBRARY, move |s| {
            s.link.send(Command::AddToPlaylist { playlist_uri: playlist_uri.clone(), track_uri: track_uri.clone() });
        }));
    }
    if !any {
        menu = menu.item(MenuItem::new("No playlists you can add to").disabled(true));
    }
    menu
}

fn context_kind_label(kind: ContextKind) -> &'static str {
    match kind {
        ContextKind::Album => "Go to album",
        ContextKind::Artist => "Go to artist",
        ContextKind::Liked => "Go to Liked Songs",
        _ => "Go to playlist",
    }
}

fn card_menu(menu: Menu, summary: &ContextSummary) -> Menu {
    let (play, shuffle, open) = (summary.uri.clone(), summary.uri.clone(), summary.uri.clone());
    let menu = menu
        .item(item("Play", &icons::PLAY, move |s| s.play_context(&play, None)))
        .item(item("Shuffle play", &icons::SHUFFLE, move |s| s.link.send(Command::PlayShuffled { uri: shuffle.clone() })));
    let menu = match artist_radio(summary) {
        Some(radio) => menu.item(item("Artist radio", &icons::RADIO, move |s| s.play_context(&radio, None))),
        None => menu,
    };
    menu.separator().item(item(context_kind_label(summary.kind), fallback(summary.kind), move |s| s.open(&open)))
}

fn fallback(kind: ContextKind) -> &'static rune::Icon {
    match kind {
        ContextKind::Album => &icons::DISC,
        ContextKind::Artist => &icons::PERSON,
        _ => &icons::LIBRARY,
    }
}

/// An artist's radio station, which plays as a context of its own.
pub fn artist_radio(summary: &ContextSummary) -> Option<String> {
    let id = summary.uri.strip_prefix("spotify:artist:").filter(|_| summary.kind == ContextKind::Artist)?;
    Some(format!("spotify:station:artist:{id}"))
}

/// The open menu's surface, for the root's overlay layer, plus the empty trigger it needs.
pub fn layer(s: &Main, cx: &mut Cx<Main>, p: Palette) -> Vec<El> {
    let Some(open) = &s.menu else { return Vec::new() };
    let point = match &open.at {
        At::Point(x, y) => (*x, *y),
        // Layout is measured inside the zoomed stage; the menu sits outside it.
        At::Below(key) => {
            let k = super::zoom::factor(s, cx);
            cx.window_layout(key).map_or((240.0, 200.0), |[x, y, _, h]| (x * k + 24.0, (y + h) * k))
        }
    };
    let anchor = el().key("track-menu-anchor").size(0.0, 0.0);
    let press = PointerPress { x: point.0, y: point.1 };
    let menu = context_menu(KEY, anchor, Some(press)).min_width(230.0).on_close(|s: &mut Main| s.close_menu());
    let menu = s.link.read(|store| match &open.target {
        Target::Track(target) => track_menu(menu, store, target, open.picking),
        Target::Context(summary) => card_menu(menu, summary),
    });
    let mut layer = Vec::new();
    let trigger = menu.build(&kit(p), &mut layer);
    layer.insert(0, trigger);
    layer
}
