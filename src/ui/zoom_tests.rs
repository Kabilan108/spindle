use std::sync::{Arc, Mutex, PoisonError};

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use super::link::Link;
use super::zoom::Zoom;
use super::{Main, main_key, main_poll, main_view};
use crate::engine::Engine;
use crate::model::{Command, Loadable};
use crate::settings::{self, AudioQuality};

const DT: f64 = 1.0 / 60.0;
const PLAYLIST: &str = "spotify:playlist:demo0";

type Sent = Arc<Mutex<Vec<Command>>>;

/// The tests that press zoom keys save the level to the shared settings; one at a time.
static SAVES_ZOOM: Mutex<()> = Mutex::new(());

fn window(size: (f32, f32), prepare: impl FnOnce(&mut Main)) -> (Runtime<Main>, Sent) {
    let (link, sent) = Link::recording(Engine::demo());
    let mut main = Main::new(link);
    prepare(&mut main);
    let mut rt = Runtime::new(main, main_view, size, Env::new(true, 1.0));
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

fn ctrl(key: &str, shift: bool) -> KeyEvent {
    KeyEvent { key: Key::Text(key.into()), mods: Mods { ctrl: true, cmd: true, shift, ..Mods::default() } }
}

fn press(rt: &mut Runtime<Main>, ev: KeyEvent) -> f32 {
    rt.key(ev);
    frames(rt, 2);
    rt.state.zoom.level
}

fn click(rt: &mut Runtime<Main>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(rt, 2);
}

fn rect(rt: &Runtime<Main>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no element keyed {key}")).rect
}

fn sent(log: &Sent) -> Vec<Command> {
    log.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn open_playlist(rt: &mut Runtime<Main>) {
    rt.state.open(PLAYLIST);
    rt.state.link.wait_for(|store| matches!(store.pages.get(PLAYLIST), Some(Loadable::Ready(_))));
    rt.update(|s, tx| s.store_changed(tx));
    frames(rt, 4);
}

#[test]
fn the_zoom_keys_step_like_a_browser_and_are_saved() {
    let _one = SAVES_ZOOM.lock().unwrap_or_else(PoisonError::into_inner);
    let (mut rt, _) = window((1920.0, 1200.0), |s| s.zoom = Zoom::preview(1.0, false));
    assert_eq!(rt.state.zoom.level, 1.0);
    assert_eq!(press(&mut rt, ctrl("=", false)), 1.1);
    assert_eq!(press(&mut rt, ctrl("+", true)), 1.25, "Ctrl++ is Ctrl+Shift+= on most layouts");
    assert_eq!(settings::get().zoom, 1.25, "the level is saved as it changes");
    assert!(rt.find("zoom-level").is_some(), "the indicator shows the new level");
    assert_eq!(press(&mut rt, ctrl("-", false)), 1.1);
    for _ in 0..10 {
        rt.key(ctrl("-", false));
    }
    frames(&mut rt, 2);
    assert_eq!(rt.state.zoom.level, 0.8, "zooming out stops at 80%");
    for _ in 0..10 {
        rt.key(ctrl("=", false));
    }
    frames(&mut rt, 2);
    assert_eq!(rt.state.zoom.level, 2.0, "zooming in stops at 200%");
    assert_eq!(press(&mut rt, ctrl("0", false)), 1.0);
    assert_eq!(settings::get().zoom, 1.0);

    frames(&mut rt, 120);
    assert!(rt.find("zoom-level").is_none(), "the indicator goes away by itself");
    let commits = rt.stats.commits;
    frames(&mut rt, 60);
    assert_eq!(rt.stats.commits, commits, "and then the window rests");
}

#[test]
fn the_zoom_keys_work_while_typing_a_search() {
    let _one = SAVES_ZOOM.lock().unwrap_or_else(PoisonError::into_inner);
    let (mut rt, _) = window((1100.0, 720.0), |s| {
        s.zoom = Zoom::preview(1.0, false);
        s.go_search(true);
    });
    rt.key(KeyEvent { key: Key::Text("a".into()), mods: Mods::default() });
    frames(&mut rt, 2);
    assert_eq!(press(&mut rt, ctrl("=", false)), 1.1);
    assert_eq!(rt.state.search.text, "a", "the field didn't take the key");
}

#[test]
fn a_zoomed_window_draws_bigger_and_still_takes_clicks_where_things_are_drawn() {
    let (plain, _) = window((1920.0, 1200.0), |_| {});
    let (mut rt, log) = window((1920.0, 1200.0), |s| s.zoom = Zoom::preview(2.0, false));
    let (a, b) = (rect(&plain, "nav-search"), rect(&rt, "nav-search"));
    assert!((b[2] / a[2] - 2.0).abs() < 0.01 && (b[3] / a[3] - 2.0).abs() < 0.01, "{a:?} vs {b:?}");

    open_playlist(&mut rt);
    let track = rt.state.link.read(|store| store.pages[PLAYLIST].ready().unwrap().tracks[3].uri.clone());
    let (x, y) = rt.find("trk3").expect("row 4 is built");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.05);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(&mut rt, 2);
    assert_eq!(rt.state.selected, Some(3));
    assert!(sent(&log).contains(&Command::PlayContext { uri: PLAYLIST.into(), track_uri: Some(track) }));
}

#[test]
fn a_zoomed_track_list_scrolls_and_builds_the_rows_it_reaches() {
    let (mut rt, _) = window((1100.0, 720.0), |s| s.zoom = Zoom::preview(1.25, false));
    open_playlist(&mut rt);
    let last = rt.state.link.read(|store| store.pages[PLAYLIST].ready().unwrap().tracks.len()) - 1;
    let (x, y) = rt.find("trk2").expect("an early row");
    for _ in 0..40 {
        rt.wheel(x, y, -120.0);
        rt.frame(DT);
    }
    frames(&mut rt, 60);
    let key = format!("trk{last}");
    let (_, y) = rt.find(&key).expect("the last row is built after scrolling");
    assert!(y < 720.0 - super::theme::BAR_H * 1.25, "and scrolled into view above the bar: {y}");
    click(&mut rt, &key);
    assert_eq!(rt.state.selected, Some(last), "and takes a click where it is drawn");
}

#[test]
fn a_small_window_zooms_only_as_far_as_the_layout_fits() {
    let (rt, _) = window((1100.0, 720.0), |s| s.zoom = Zoom::preview(2.0, false));
    let zoomed = rect(&rt, "zoomed");
    assert!((zoomed[2] - 1100.0).abs() < 0.5 && (zoomed[3] - 720.0).abs() < 0.5, "the zoomed UI fills the window exactly: {zoomed:?}");
    let shown = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("zoomed")).unwrap().scale;
    assert!((shown - 1.5).abs() < 0.01, "1100×720 shows 150%, not {shown}");
}

#[test]
fn the_settings_popover_opens_from_the_sidebar_and_works_zoomed() {
    let (mut rt, _) = window((1920.0, 1200.0), |s| s.zoom = Zoom::preview(1.5, false));
    click(&mut rt, "settings");
    assert!(rt.state.settings_open);
    let panel = rect(&rt, "settings-panel");
    let button = rect(&rt, "settings");
    assert!(panel[1] + panel[3] <= button[1] + 0.5, "the popover sits above its button: {panel:?} {button:?}");
    click(&mut rt, "quality-160");
    assert_eq!(settings::get().audio_quality, AudioQuality::Normal);
    assert!(rt.find("audio-restart").is_some(), "it says when the change applies");
    click(&mut rt, "quality-320");
    assert_eq!(settings::get().audio_quality, AudioQuality::VeryHigh);
    assert!(rt.find("audio-restart").is_none());
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    frames(&mut rt, 2);
    assert!(!rt.state.settings_open, "Escape closes it");
}
