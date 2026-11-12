//! Named states for headless snapshots (`--scene NAME`), built on the demo engine. Each waits
//! for the engine's answer so the snapshot shows it.

use super::sidebar::Shelf;
use super::menu::{At, page_target};
use super::{LIKED, Main, Scene};
use crate::model::{Command, EPISODES_URI, Loadable, SessionState};

fn open(s: &mut Main, uri: &str) {
    s.open(uri);
    s.link.wait_for(|store| matches!(store.pages.get(uri), Some(Loadable::Ready(_) | Loadable::Failed(_))));
}

fn playlist(s: &mut Main) {
    open(s, "spotify:playlist:demo0");
    s.selected = Some(5);
}

fn album(s: &mut Main) {
    open(s, "spotify:album:demo2");
}

fn artist(s: &mut Main) {
    open(s, "spotify:artist:demo3");
}

fn liked(s: &mut Main) {
    open(s, LIKED);
}

fn search(s: &mut Main) {
    s.go_search(false);
    s.search.replace_text("har");
    s.link.send_and_wait(Command::Search { query: "har".into() }, |store| store.search.query == "har");
}

fn search_empty(s: &mut Main) {
    s.go_search(true);
}

fn playing(s: &mut Main) {
    s.link.send_and_wait(Command::PlayPause, |store| store.playback.playing);
}

fn album_playing(s: &mut Main) {
    open(s, "spotify:album:demo2");
    s.link.send_and_wait(Command::PlayContext { uri: "spotify:album:demo2".into(), track_uri: None }, |store| store.playback.playing);
}

fn stage(s: &mut Main) {
    let album = "spotify:album:demo0";
    s.link.send_and_wait(Command::PlayContext { uri: album.into(), track_uri: Some("spotify:track:demo0x2".into()) }, |store| store.playback.playing);
    s.viz.open_stage();
}

fn stage_paused(s: &mut Main) {
    s.viz.open_stage();
}

fn sign_in(s: &mut Main) {
    s.preview_session = Some(SessionState::NeedsLogin {
        url: Some("https://accounts.spotify.com/authorize?response_type=code&client_id=baliset-demo&redirect_uri=http%3A%2F%2F127.0.0.1%3A8898%2Flogin&scope=streaming&state=f3k9".into()),
        error: Some("The sign-in page didn't answer in time. Try again.".into()),
    });
}

fn sign_in_start(s: &mut Main) {
    s.preview_session = Some(SessionState::NeedsLogin { url: None, error: None });
}

fn connecting(s: &mut Main) {
    s.preview_session = Some(SessionState::Connecting);
}

fn offline(s: &mut Main) {
    s.preview_session = Some(SessionState::Offline { error: "network unreachable".into() });
    open(s, "spotify:album:demo0");
}

fn toast(s: &mut Main) {
    s.link.send_and_wait(Command::PlayContext { uri: "spotify:playlist:gone".into(), track_uri: None }, |store| store.notice.is_some());
}

fn settings(s: &mut Main) {
    s.settings_open = true;
}

fn zoomed(s: &mut Main, level: f32) {
    s.zoom = super::zoom::Zoom::preview(level, false);
    open(s, "spotify:playlist:demo0");
}

fn zoom_indicator(s: &mut Main) {
    s.zoom = super::zoom::Zoom::preview(2.0, true);
}

fn show(s: &mut Main) {
    s.shelf = Shelf::Podcasts;
    open(s, "spotify:show:demo0");
}

fn serial(s: &mut Main) {
    s.shelf = Shelf::Podcasts;
    open(s, "spotify:show:demo2");
    s.selected = Some(1);
}

fn episodes(s: &mut Main) {
    s.shelf = Shelf::Podcasts;
    open(s, EPISODES_URI);
}

fn artists(s: &mut Main) {
    s.shelf = Shelf::Artists;
    open(s, "spotify:artist:demo3");
}

/// A two-hour episode paused past its first hour.
fn episode_playing(s: &mut Main) {
    open(s, "spotify:show:demo0");
    let episode = "spotify:episode:demo0x2";
    s.link.send_and_wait(Command::PlayContext { uri: "spotify:show:demo0".into(), track_uri: Some(episode.into()) }, |store| store.playback.track.as_ref().is_some_and(|t| t.uri == episode));
    s.link.send_and_wait(Command::PlayPause, |store| !store.playback.playing);
    s.link.send_and_wait(Command::Seek { position_ms: 3_723_000 }, |store| store.playback.position_ms == 3_723_000);
}

fn track_menu(s: &mut Main) {
    playlist(s);
    let target = s.link.read(|store| page_target(store, "spotify:playlist:demo0", 5));
    s.open_menu(target, At::Below("trk5".into()));
}

fn track_menu_zoomed(s: &mut Main) {
    s.zoom = super::zoom::Zoom::preview(1.25, false);
    track_menu(s);
}

fn track_menu_playlists(s: &mut Main) {
    track_menu(s);
    if let Some(menu) = &mut s.menu {
        menu.picking = true;
    }
}

fn card_menu(s: &mut Main) {
    let album = s.link.read(|store| store.albums.ready().and_then(|albums| albums.first().cloned()));
    if let Some(album) = album {
        s.open_card_menu(album, At::Point(330.0, 230.0));
    }
}

fn up_next(s: &mut Main) {
    s.link.send(Command::AddToQueue { uri: "spotify:track:demo1x2".into() });
    s.link.send_and_wait(Command::AddToQueue { uri: "spotify:track:demo4x0".into() }, |store| store.up_next.queued.len() == 2);
    s.link.send_and_wait(Command::PlayPause, |store| store.playback.playing);
    s.queue_open = true;
    open(s, "spotify:playlist:demo0");
}

fn shuffled(s: &mut Main) {
    s.link.send_and_wait(Command::PlayShuffled { uri: "spotify:album:demo2".into() }, |store| store.playback.shuffle);
    album(s);
}

fn radio(s: &mut Main) {
    s.link.send_and_wait(Command::LoadRadio { track_uri: "spotify:track:demo3x1".into() }, |store| !store.radios.is_empty());
    s.go_radio("spotify:track:demo3x1");
    let uri = s.link.read(|store| store.radios.values().find_map(|radio| radio.ready().cloned())).unwrap_or_default();
    open(s, &uri);
}

pub const MAIN_SCENES: &[Scene<Main>] = &[
    ("home", |_| {}),
    ("playlist", playlist),
    ("album", album),
    ("artist", artist),
    ("liked", liked),
    ("search", search),
    ("search-empty", search_empty),
    ("playing", playing),
    ("sign-in", sign_in),
    ("sign-in-start", sign_in_start),
    ("connecting", connecting),
    ("offline", offline),
    ("toast", toast),
    ("settings", settings),
    ("zoom-80", |s| zoomed(s, 0.8)),
    ("zoom-125", |s| zoomed(s, 1.25)),
    ("zoom-200", |s| zoomed(s, 2.0)),
    ("zoom-indicator", zoom_indicator),
    ("show", show),
    ("serial", serial),
    ("episodes", episodes),
    ("artists", artists),
    ("episode-playing", episode_playing),
    ("track-menu", track_menu),
    ("track-menu-zoomed", track_menu_zoomed),
    ("track-menu-playlists", track_menu_playlists),
    ("card-menu", card_menu),
    ("up-next", up_next),
    ("shuffled", shuffled),
    ("radio", radio),
    ("album-playing", album_playing),
    ("stage", stage),
    ("stage-paused", stage_paused),
];
