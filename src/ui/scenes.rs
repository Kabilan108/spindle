//! Named states for headless snapshots (`--scene NAME`), built on the demo engine. Each waits
//! for the engine's answer so the snapshot shows it.

use super::menu::{At, page_target};
use super::{LIKED, Main, Scene};
use crate::model::{Command, Loadable, SessionState};

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

fn track_menu(s: &mut Main) {
    playlist(s);
    let target = s.link.read(|store| page_target(store, "spotify:playlist:demo0", 5));
    s.open_menu(target, At::Below("trk5".into()));
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
    ("track-menu", track_menu),
    ("track-menu-playlists", track_menu_playlists),
    ("card-menu", card_menu),
    ("up-next", up_next),
    ("shuffled", shuffled),
    ("radio", radio),
];
