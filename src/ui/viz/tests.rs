use rune::{Env, Key, KeyEvent, Mods, Runtime};

use super::super::link::Link;
use super::super::{Main, main_key, main_poll, main_view};
use crate::engine::Engine;
use crate::model::{Command, Loadable};

const DT: f64 = 1.0 / 60.0;
const ALBUM: &str = "spotify:album:demo2";

fn window(prepare: impl FnOnce(&mut Main)) -> Runtime<Main> {
    let mut main = Main::new(Link::new(Engine::demo()));
    prepare(&mut main);
    let mut rt = Runtime::new(main, main_view, (1100.0, 720.0), Env::new(true, 1.0));
    rt.poll = Some(main_poll);
    rt.on_key = Some(main_key);
    rt.frame(0.0);
    frames(&mut rt, 10);
    rt
}

fn frames(rt: &mut Runtime<Main>, n: usize) {
    for _ in 0..n {
        rt.frame(DT);
    }
}

fn engine_answered(rt: &mut Runtime<Main>, done: impl Fn(&crate::model::Store) -> bool) {
    rt.state.link.wait_for(done);
    rt.update(|s, tx| s.store_changed(tx));
    frames(rt, 2);
}

fn play(rt: &mut Runtime<Main>) {
    rt.state.link.send(Command::PlayPause);
    engine_answered(rt, |store| store.playback.playing);
}

fn click(rt: &mut Runtime<Main>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

fn press(rt: &mut Runtime<Main>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    frames(rt, 2);
}

#[test]
fn the_bar_button_opens_the_stage_and_escape_closes_it() {
    let mut rt = window(|_| {});
    click(&mut rt, "stage-open");
    assert!(rt.state.viz.stage_open);
    assert!(rt.find("stage-play").is_some(), "the Stage carries the transport");
    assert!(rt.find("now-playing").is_none(), "the bar makes way");
    press(&mut rt, Key::Escape, Mods::default());
    assert!(!rt.state.viz.stage_open);
    assert!(rt.find("now-playing").is_some());
}

#[test]
fn ctrl_e_opens_and_closes_the_stage() {
    let mut rt = window(|_| {});
    let ctrl = Mods { ctrl: true, ..Mods::default() };
    press(&mut rt, Key::Text("e".into()), ctrl);
    assert!(rt.state.viz.stage_open);
    press(&mut rt, Key::Text("e".into()), ctrl);
    assert!(!rt.state.viz.stage_open);
}

#[test]
fn the_stage_buttons_drive_the_player() {
    let mut rt = window(|s| s.viz.open_stage());
    click(&mut rt, "stage-play");
    engine_answered(&mut rt, |store| store.playback.playing);
    assert!(rt.state.link.read(|store| store.playback.playing));
    click(&mut rt, "stage-close");
    assert!(!rt.state.viz.stage_open);
}

#[test]
fn a_paused_stage_rests() {
    let mut rt = window(|s| s.viz.open_stage());
    frames(&mut rt, 30);
    let commits = rt.stats.commits;
    let mut wanted_frame = false;
    for _ in 0..120 {
        wanted_frame |= rt.frame(DT);
    }
    assert!(!wanted_frame, "no frame asked for another");
    assert_eq!(rt.wake_deadline(), None, "no timer is set");
    assert_eq!(rt.stats.commits, commits, "the view didn't run again");
}

#[test]
fn a_playing_stage_moves_its_spectrum_without_running_the_view() {
    let mut rt = window(|s| s.viz.open_stage());
    play(&mut rt);
    frames(&mut rt, 30);
    let commits = rt.stats.commits;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..120 {
        rt.frame(DT);
        seen.insert(rt.state.viz.stage.level(3).to_bits());
    }
    assert!(seen.len() > 10, "the low bands moved through {} values in two seconds", seen.len());
    assert!(rt.wake_deadline().is_some_and(|due| due - rt.clock < 0.05), "the next analysis step is planned");
    assert_eq!(rt.stats.commits, commits, "music moves live values only");
}

#[test]
fn the_stage_settles_after_a_pause() {
    let mut rt = window(|s| s.viz.open_stage());
    play(&mut rt);
    frames(&mut rt, 60);
    rt.state.link.send(Command::PlayPause);
    engine_answered(&mut rt, |store| !store.playback.playing);
    frames(&mut rt, 4 * 60);
    assert!(rt.state.viz.analysis.settled());
    let mut wanted_frame = false;
    for _ in 0..60 {
        wanted_frame |= rt.frame(DT);
    }
    assert!(!wanted_frame, "nothing moves four seconds after the pause");
    assert_eq!(rt.wake_deadline(), None);
}

#[test]
fn nothing_is_analysed_without_a_visualizer_on_screen() {
    let mut rt = window(|_| {});
    play(&mut rt);
    frames(&mut rt, 60);
    assert!(rt.state.viz.stepped.is_none(), "home shows no visualizer");
}

#[test]
fn an_album_header_follows_the_music() {
    let mut rt = window(|s| s.open(ALBUM));
    engine_answered(&mut rt, |store| matches!(store.pages.get(ALBUM), Some(Loadable::Ready(_))));
    frames(&mut rt, 10);
    assert!(rt.find("ambient").is_some(), "an album page gets the ambient header");
    let still = rt.state.viz.header.pool_offset(0);
    play(&mut rt);
    frames(&mut rt, 120);
    assert!(rt.state.viz.stepped.is_some());
    assert_ne!(rt.state.viz.header.pool_offset(0), still, "the pools drift while playing");
}
