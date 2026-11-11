use std::sync::{Arc, Mutex, PoisonError};

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use super::link::Link;
use super::{Main, main_key, main_poll, main_view};
use crate::engine::Engine;
use crate::model::{Command, Loadable};

const DT: f64 = 1.0 / 60.0;
const PLAYLIST: &str = "spotify:playlist:demo0";

type Sent = Arc<Mutex<Vec<Command>>>;

fn window(prepare: impl FnOnce(&mut Main)) -> (Runtime<Main>, Sent) {
    let (link, sent) = Link::recording(Engine::demo());
    let mut main = Main::new(link);
    prepare(&mut main);
    let mut rt = Runtime::new(main, main_view, (1100.0, 720.0), Env::new(true, 1.0));
    rt.poll = Some(main_poll);
    rt.on_key = Some(main_key);
    rt.frame(0.0);
    frames(&mut rt, 10);
    (rt, sent)
}

fn frames(rt: &mut Runtime<Main>, n: usize) {
    for _ in 0..n {
        rt.frame(DT);
    }
}

fn sent(log: &Sent) -> Vec<Command> {
    log.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn clear(log: &Sent) {
    log.lock().unwrap_or_else(PoisonError::into_inner).clear();
}

/// The engine answers on its own thread; the view sees it on a later commit.
fn engine_answered(rt: &mut Runtime<Main>, done: impl Fn(&crate::model::Store) -> bool) {
    rt.state.link.wait_for(done);
    rt.update(|s, tx| s.store_changed(tx));
    frames(rt, 2);
}

fn click(rt: &mut Runtime<Main>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

#[test]
fn double_clicking_a_row_plays_the_playlist_from_that_track() {
    let (mut rt, log) = window(|s| s.open(PLAYLIST));
    engine_answered(&mut rt, |store| matches!(store.pages.get(PLAYLIST), Some(Loadable::Ready(_))));
    let track = rt.state.link.read(|store| store.pages[PLAYLIST].ready().unwrap().tracks[3].uri.clone());
    clear(&log);

    let (x, y) = rt.find("trk3").expect("row 4 is built");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.05);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(&mut rt, 2);

    assert_eq!(rt.state.selected, Some(3), "the first click selects the row");
    let plays: Vec<Command> = sent(&log).into_iter().filter(|c| matches!(c, Command::PlayContext { .. })).collect();
    assert_eq!(plays, vec![Command::PlayContext { uri: PLAYLIST.into(), track_uri: Some(track) }]);
}

#[test]
fn return_on_the_track_list_plays_the_selected_row() {
    let (mut rt, log) = window(|s| s.open(PLAYLIST));
    engine_answered(&mut rt, |store| matches!(store.pages.get(PLAYLIST), Some(Loadable::Ready(_))));
    click(&mut rt, "trk1");
    rt.key(KeyEvent { key: Key::Down, mods: Mods::default() });
    frames(&mut rt, 2);
    assert_eq!(rt.state.selected, Some(2));
    clear(&log);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    frames(&mut rt, 2);
    let track = rt.state.link.read(|store| store.pages[PLAYLIST].ready().unwrap().tracks[2].uri.clone());
    assert_eq!(sent(&log), vec![Command::PlayContext { uri: PLAYLIST.into(), track_uri: Some(track) }]);
}

#[test]
fn the_play_button_toggles_playback() {
    let (mut rt, log) = window(|_| {});
    assert!(!rt.state.link.read(|store| store.playback.playing));
    clear(&log);

    click(&mut rt, "play-pause");
    engine_answered(&mut rt, |store| store.playback.playing);
    assert!(rt.state.link.read(|store| store.playback.playing));
    let label = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("play-pause")).and_then(|e| e.label);
    assert_eq!(label.as_deref(), Some("Pause"));

    click(&mut rt, "play-pause");
    engine_answered(&mut rt, |store| !store.playback.playing);
    assert!(!rt.state.link.read(|store| store.playback.playing));
    assert_eq!(sent(&log), vec![Command::PlayPause, Command::PlayPause]);
}

#[test]
fn space_toggles_playback_when_no_field_has_focus() {
    let (mut rt, log) = window(|_| {});
    clear(&log);
    rt.key(KeyEvent { key: Key::Text(" ".into()), mods: Mods::default() });
    frames(&mut rt, 2);
    assert_eq!(sent(&log), vec![Command::PlayPause]);
}

#[test]
fn typing_a_search_sends_one_search_after_the_pause() {
    let (mut rt, log) = window(|s| s.go_search(true));
    clear(&log);
    for c in ["h", "a", "r"] {
        rt.key(KeyEvent { key: Key::Text(c.into()), mods: Mods::default() });
        frames(&mut rt, 4);
    }
    assert_eq!(rt.state.search.text, "har", "the field took the typing, not the window");
    assert!(sent(&log).iter().all(|c| !matches!(c, Command::Search { .. })), "nothing goes out while typing");

    frames(&mut rt, 40);
    let searches: Vec<Command> = sent(&log).into_iter().filter(|c| matches!(c, Command::Search { .. })).collect();
    assert_eq!(searches, vec![Command::Search { query: "har".into() }]);
}

#[test]
fn dragging_the_seek_bar_sends_one_seek_on_release() {
    let (mut rt, log) = window(|_| {});
    let duration = rt.state.link.read(|store| store.playback.track.as_ref().unwrap().duration_ms);
    let seek = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("seek")).expect("seek bar");
    let [x, y, w, h] = seek.rect;
    clear(&log);

    let cy = y + h / 2.0;
    rt.pointer_down(x + w * 0.2, cy);
    frames(&mut rt, 1);
    for step in 1..=10 {
        rt.pointer_move(x + w * (0.2 + 0.05 * step as f32), cy);
        frames(&mut rt, 1);
    }
    assert!(sent(&log).is_empty(), "nothing goes out mid-drag");
    assert_eq!(rt.state.progress.elapsed.get().as_ref(), super::meter::fmt_time((duration as f32 * 0.7) as u32), "the time follows the drag");
    rt.pointer_up(x + w * 0.7, cy);
    frames(&mut rt, 2);

    let seeks = sent(&log);
    assert_eq!(seeks.len(), 1, "{seeks:?}");
    let Command::Seek { position_ms } = seeks[0] else { panic!("{seeks:?}") };
    let expected = duration as f32 * 0.7;
    assert!((position_ms as f32 - expected).abs() < duration as f32 * 0.01, "{position_ms} vs {expected}");
}

#[test]
fn a_paused_window_rests() {
    let (mut rt, _log) = window(|_| {});
    frames(&mut rt, 30);
    let commits = rt.stats.commits;
    let mut wanted_frame = false;
    for _ in 0..120 {
        wanted_frame |= rt.frame(DT);
        assert!(!main_poll(&mut rt.state, rt.clock, &mut rune::Tx::default()), "poll reports nothing new");
    }
    assert!(!wanted_frame, "no frame asked for another");
    assert_eq!(rt.wake_deadline(), None, "no timer is set");
    assert!(!rt.state.progress.ticking());
    assert_eq!(rt.stats.commits, commits, "the view didn't run again");
}

#[test]
fn a_playing_window_ticks_a_few_times_a_second_without_running_the_view() {
    let (mut rt, _log) = window(|_| {});
    click(&mut rt, "play-pause");
    engine_answered(&mut rt, |store| store.playback.playing);
    frames(&mut rt, 30);
    assert!(rt.state.progress.ticking());
    let due = rt.wake_deadline().expect("a tick is planned");
    assert!(due - rt.clock <= 1.01, "ticks come at least once a second");
    let commits = rt.stats.commits;
    frames(&mut rt, 120);
    assert_eq!(rt.stats.commits, commits, "ticks move live values only");
}

#[test]
fn art_is_kept_by_url_not_decoded_per_view() {
    let (mut rt, _log) = window(|_| {});
    rt.invalidate();
    frames(&mut rt, 60);
    let before = rt.state.art_entries();
    assert!(before > 0, "home shows art");
    for _ in 0..5 {
        rt.invalidate();
        frames(&mut rt, 3);
    }
    assert_eq!(rt.state.art_entries(), before, "the same views find the same handles");
}
