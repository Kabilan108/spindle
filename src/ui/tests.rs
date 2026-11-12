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

#[test]
fn the_podcasts_shelf_lists_your_episodes_then_followed_shows() {
    let (mut rt, log) = window(|_| {});
    click(&mut rt, "shelf-podcasts");
    assert_eq!(rt.state.shelf, super::sidebar::Shelf::Podcasts);
    let labels: Vec<String> = rt.elements().into_iter().filter(|e| e.role == "link").filter_map(|e| e.label).collect();
    let at = labels.iter().position(|l| l == "Your Episodes").expect("Your Episodes is listed");
    assert_eq!(labels.get(at + 1).map(String::as_str), Some("Signal & Noise"));
    clear(&log);
    click(&mut rt, "Podcasts1");
    assert_eq!(rt.state.route, super::Route::Context("spotify:show:demo0".into()));
    assert_eq!(sent(&log), vec![Command::LoadContext { uri: "spotify:show:demo0".into() }]);
}

#[test]
fn an_episode_plays_inside_its_show_so_next_and_previous_work() {
    const SHOW: &str = "spotify:show:demo0";
    let (mut rt, log) = window(|s| s.open(SHOW));
    engine_answered(&mut rt, |store| matches!(store.pages.get(SHOW), Some(Loadable::Ready(_))));
    clear(&log);
    let (x, y) = rt.find("trk2").expect("the third episode is built");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.05);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(&mut rt, 2);
    let plays: Vec<Command> = sent(&log).into_iter().filter(|c| matches!(c, Command::PlayContext { .. })).collect();
    assert_eq!(plays, vec![Command::PlayContext { uri: SHOW.into(), track_uri: Some("spotify:episode:demo0x2".into()) }]);
    engine_answered(&mut rt, |store| store.playback.track.as_ref().is_some_and(|t| t.uri == "spotify:episode:demo0x2"));
    let artist = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("now-artist")).and_then(|e| e.text);
    assert_eq!(artist.as_deref(), Some("Signal & Noise"), "the bar names the show");
    assert!(rt.find("now-like").is_none(), "episodes don't go in Liked Songs");
}

#[test]
fn the_follow_button_unfollows_a_followed_show() {
    const SHOW: &str = "spotify:show:demo1";
    let (mut rt, log) = window(|s| s.open(SHOW));
    engine_answered(&mut rt, |store| matches!(store.pages.get(SHOW), Some(Loadable::Ready(_))));
    clear(&log);
    click(&mut rt, "follow");
    assert_eq!(sent(&log), vec![Command::SetFollowed { uri: SHOW.into(), followed: false }]);
    engine_answered(&mut rt, |store| !store.follows(SHOW));
    let label = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("follow")).and_then(|e| e.label);
    assert_eq!(label.as_deref(), Some("Follow"));
}

/// A menu item by its label.
fn element(rt: &Runtime<Main>, label: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.role == "menuitem" && e.label.as_deref() == Some(label))
}

fn click_label(rt: &mut Runtime<Main>, label: &str) {
    let e = element(rt, label).unwrap_or_else(|| panic!("nothing labelled {label}"));
    let [x, y, w, h] = e.rect;
    rt.pointer_down(x + w / 2.0, y + h / 2.0);
    rt.pointer_up(x + w / 2.0, y + h / 2.0);
    frames(rt, 2);
}

fn right_click(rt: &mut Runtime<Main>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    frames(rt, 20);
}

fn menu_open(rt: &Runtime<Main>) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some("track-menu-menu"))
}

fn playlist_window() -> (Runtime<Main>, Sent) {
    let (mut rt, log) = window(|s| s.open(PLAYLIST));
    engine_answered(&mut rt, |store| matches!(store.pages.get(PLAYLIST), Some(Loadable::Ready(_))));
    (rt, log)
}

fn track_at(rt: &Runtime<Main>, uri: &str, i: usize) -> String {
    rt.state.link.read(|store| store.pages[uri].ready().unwrap().tracks[i].uri.clone())
}

#[test]
fn right_clicking_a_row_selects_it_and_add_to_queue_queues_it() {
    let (mut rt, log) = playlist_window();
    let track = track_at(&rt, PLAYLIST, 4);
    clear(&log);
    right_click(&mut rt, "trk4");
    assert_eq!(rt.state.selected, Some(4));
    assert!(menu_open(&rt));
    for label in ["Play", "Add to queue", "Go to song radio", "Add to playlist", "Remove from this playlist", "Go to album", "Go to artist"] {
        assert!(element(&rt, label).is_some(), "the menu offers {label}");
    }
    click_label(&mut rt, "Add to queue");
    assert_eq!(sent(&log), vec![Command::AddToQueue { uri: track.clone() }]);
    engine_answered(&mut rt, |store| store.up_next.queued.len() == 1);
    frames(&mut rt, 20);
    assert!(!menu_open(&rt), "picking an item closes the menu");
    assert_eq!(rt.state.link.read(|store| store.up_next.queued[0].uri.clone()), track);
}

#[test]
fn an_album_page_offers_no_removal() {
    let album = "spotify:album:demo1";
    let (mut rt, _log) = window(|s| s.open(album));
    engine_answered(&mut rt, |store| matches!(store.pages.get(album), Some(Loadable::Ready(_))));
    right_click(&mut rt, "trk0");
    assert!(element(&rt, "Add to playlist").is_some());
    assert!(element(&rt, "Remove from this playlist").is_none());
}

#[test]
fn the_menu_key_opens_the_selected_rows_menu_and_escape_hands_focus_back() {
    let (mut rt, log) = playlist_window();
    click(&mut rt, "trk2");
    let focused = |rt: &Runtime<Main>| rt.elements().into_iter().find(|e| e.focused).and_then(|e| e.key);
    assert_eq!(focused(&rt).as_deref(), Some("tracklist"));
    for (key, shift) in [(Key::ContextMenu, false), (Key::Function(10), true)] {
        rt.key(KeyEvent { key: key.clone(), mods: Mods { shift, ..Mods::default() } });
        frames(&mut rt, 20);
        assert!(menu_open(&rt), "{key:?} opens the menu");
        let menu = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("track-menu-menu")).unwrap();
        let row = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("trk2")).unwrap();
        assert!((menu.rect[1] - (row.rect[1] + row.rect[3])).abs() < 2.0 || menu.rect[1] + menu.rect[3] <= row.rect[1] + 1.0, "it opens at the row");
        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        frames(&mut rt, 20);
        assert!(!menu_open(&rt));
        assert_eq!(focused(&rt).as_deref(), Some("tracklist"), "focus goes back to the list");
    }
    rt.key(KeyEvent { key: Key::Down, mods: Mods::default() });
    frames(&mut rt, 2);
    clear(&log);
    rt.key(KeyEvent { key: Key::ContextMenu, mods: Mods::default() });
    frames(&mut rt, 20);
    assert_eq!(focused(&rt).as_deref(), Some("track-menu-item0"), "the menu takes focus on its first item");
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    frames(&mut rt, 2);
    let track = track_at(&rt, PLAYLIST, 3);
    assert_eq!(sent(&log), vec![Command::PlayContext { uri: PLAYLIST.into(), track_uri: Some(track) }], "Return picks the first item, Play");
}

#[test]
fn add_to_playlist_lists_editable_playlists_and_adds_to_the_one_picked() {
    let (mut rt, log) = playlist_window();
    let track = track_at(&rt, PLAYLIST, 1);
    right_click(&mut rt, "trk1");
    click_label(&mut rt, "Add to playlist");
    frames(&mut rt, 20);
    assert!(menu_open(&rt), "the menu stays open, showing playlists");
    assert!(element(&rt, "Sunday Cooking").is_some());
    assert!(element(&rt, "Songs My Sister Sent").is_some(), "collaborative playlists count");
    assert!(element(&rt, "Discover Weekly").is_none(), "Spotify's own don't");
    clear(&log);
    click_label(&mut rt, "Sunday Cooking");
    let target = "spotify:playlist:demo1";
    assert_eq!(sent(&log), vec![Command::AddToPlaylist { playlist_uri: target.into(), track_uri: track.clone() }]);
    frames(&mut rt, 20);
    assert!(!menu_open(&rt));

    rt.update(|s, _| s.open(target));
    engine_answered(&mut rt, |store| matches!(store.pages.get(target), Some(Loadable::Ready(_))));
    let tracks = rt.state.link.read(|store| store.pages[target].ready().unwrap().tracks.len());
    assert_eq!(track_at(&rt, target, tracks - 1), track, "the track lands at the end");
}

#[test]
fn remove_from_this_playlist_takes_the_row_out() {
    let (mut rt, log) = playlist_window();
    let (before, track) = (rt.state.link.read(|store| store.pages[PLAYLIST].ready().unwrap().tracks.len()), track_at(&rt, PLAYLIST, 3));
    right_click(&mut rt, "trk3");
    clear(&log);
    click_label(&mut rt, "Remove from this playlist");
    assert_eq!(sent(&log), vec![Command::RemoveFromPlaylist { playlist_uri: PLAYLIST.into(), track_uri: track.clone(), index: 3 }]);
    engine_answered(&mut rt, |store| store.pages[PLAYLIST].ready().is_some_and(|page| page.tracks.len() == before - 1));
    assert_ne!(track_at(&rt, PLAYLIST, 3), track);
}

#[test]
fn go_to_song_radio_opens_the_radio_playlist() {
    let (mut rt, _log) = playlist_window();
    let track = track_at(&rt, PLAYLIST, 0);
    right_click(&mut rt, "trk0");
    click_label(&mut rt, "Go to song radio");
    engine_answered(&mut rt, |store| store.radios.get(&track).is_some_and(|radio| radio.ready().is_some()));
    let radio = rt.state.link.read(|store| store.radios[&track].ready().cloned().unwrap());
    assert_eq!(rt.state.route, super::Route::Context(radio));
    assert_eq!(rt.state.awaiting_radio, None);
}

#[test]
fn the_shuffle_button_starts_the_page_shuffled_then_toggles_shuffle() {
    let album = "spotify:album:demo2";
    let (mut rt, log) = window(|s| s.open(album));
    engine_answered(&mut rt, |store| matches!(store.pages.get(album), Some(Loadable::Ready(_))));
    let ring = |rt: &Runtime<Main>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("page-shuffle")).map(|e| e.fill);
    let off = ring(&rt);
    clear(&log);
    click(&mut rt, "page-shuffle");
    assert_eq!(sent(&log), vec![Command::PlayShuffled { uri: album.into() }]);
    engine_answered(&mut rt, |store| store.playback.shuffle && store.playback.context_uri.as_deref() == Some(album));
    assert_ne!(ring(&rt), off, "the button shows it's on while this album plays shuffled");
    clear(&log);
    click(&mut rt, "page-shuffle");
    assert_eq!(sent(&log), vec![Command::SetShuffle(false)]);
}

#[test]
fn the_queue_button_shows_up_next_and_a_queued_row_plays_on_double_click() {
    let (mut rt, log) = window(|_| {});
    rt.state.link.send(Command::AddToQueue { uri: "spotify:track:demo1x2".into() });
    engine_answered(&mut rt, |store| store.up_next.queued.len() == 1);
    assert!(rt.find("upnext").is_none());
    click(&mut rt, "queue-toggle");
    frames(&mut rt, 10);
    assert!(rt.find("upnext-playing").is_some());
    assert!(rt.find("upnext-next0").is_some(), "the context's next tracks show");
    clear(&log);
    let (x, y) = rt.find("upnext-queued0").expect("the queued track shows");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.05);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    frames(&mut rt, 2);
    assert_eq!(sent(&log), vec![Command::SkipTo { uri: "spotify:track:demo1x2".into() }]);
    engine_answered(&mut rt, |store| store.up_next.queued.is_empty());
    assert_eq!(rt.state.link.read(|store| store.playback.track.as_ref().map(|t| t.uri.clone())).as_deref(), Some("spotify:track:demo1x2"));

    right_click(&mut rt, "upnext-next0");
    assert!(element(&rt, "Play").is_some() && element(&rt, "Remove from queue").is_none(), "context rows aren't queued");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    frames(&mut rt, 10);
    click(&mut rt, "upnext-close");
    assert!(rt.find("upnext").is_none());
}

#[test]
fn a_cards_menu_plays_shuffles_and_opens() {
    let (mut rt, log) = window(|_| {});
    let album = rt.state.link.read(|store| store.recents[1].clone());
    right_click(&mut rt, &format!("card:{}", album.uri));
    for label in ["Play", "Shuffle play", "Go to album"] {
        assert!(element(&rt, label).is_some(), "{label}");
    }
    clear(&log);
    click_label(&mut rt, "Shuffle play");
    assert_eq!(sent(&log), vec![Command::PlayShuffled { uri: album.uri }]);
}

#[test]
fn the_now_playing_track_has_a_menu_without_play() {
    let (mut rt, _log) = window(|_| {});
    right_click(&mut rt, "now-title");
    assert!(menu_open(&rt));
    assert!(element(&rt, "Play").is_none());
    assert!(element(&rt, "Add to queue").is_some() && element(&rt, "Go to artist").is_some());
}

#[test]
fn an_albums_artist_and_a_rows_artist_open_the_artist_page() {
    const ALBUM: &str = "spotify:album:demo0";
    let (mut rt, _log) = window(|s| s.open(ALBUM));
    engine_answered(&mut rt, |store| matches!(store.pages.get(ALBUM), Some(Loadable::Ready(_))));
    let artist = rt.state.link.read(|store| store.pages[ALBUM].ready().unwrap().tracks[0].artists[0].uri.clone());
    click(&mut rt, "page-owner");
    assert_eq!(rt.state.route, super::Route::Context(artist.clone()));
    rt.update(|s, _| s.open(ALBUM));
    frames(&mut rt, 2);
    click(&mut rt, "artist1");
    assert_eq!(rt.state.route, super::Route::Context(artist));
}

#[test]
fn an_episodes_menu_offers_its_show_and_nothing_songs_only() {
    const SHOW: &str = "spotify:show:demo0";
    let (mut rt, _log) = window(|s| s.open(SHOW));
    engine_answered(&mut rt, |store| matches!(store.pages.get(SHOW), Some(Loadable::Ready(_))));
    right_click(&mut rt, "trk1");
    assert!(menu_open(&rt));
    assert!(element(&rt, "Add to queue").is_some() && element(&rt, "Go to show").is_some());
    assert!(element(&rt, "Go to song radio").is_none() && element(&rt, "Add to playlist").is_none() && element(&rt, "Save to Liked Songs").is_none());
}
