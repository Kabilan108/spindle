//! Named states for headless snapshots (`--scene NAME`), built on the demo engine. Each waits
//! for the engine's answer so the snapshot shows it.

use super::sidebar::Shelf;
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
    ("show", show),
    ("serial", serial),
    ("episodes", episodes),
    ("artists", artists),
    ("episode-playing", episode_playing),
];
