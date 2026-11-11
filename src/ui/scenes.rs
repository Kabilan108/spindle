//! Named states for headless snapshots (`--scene NAME`), built on the demo engine. Each waits
//! for the engine's answer so the snapshot shows it.

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
        url: Some("https://accounts.spotify.com/authorize?response_type=code&client_id=spindle-demo&redirect_uri=http%3A%2F%2F127.0.0.1%3A8898%2Flogin&scope=streaming&state=f3k9".into()),
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
];
