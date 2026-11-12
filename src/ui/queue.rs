//! Up next: a panel at the right of the window with the playing track, what was added to
//! the queue, and what the context plays after that. The now-playing bar's queue button
//! opens and closes it.

use rune::{El, PointerPress, ScrollSpec, Tx, col, el, row, text};

use super::menu::{At, playing_target, up_next_target};
use super::page::known_summary;
use super::theme::{Palette, font};
use super::widgets::{RowFacts, RowLook, empty_state, icon_button, track_row};
use super::{Main, icons};
use crate::model::{Command, Track};

pub const PANEL_W: f32 = 340.0;
const LOOK: RowLook = RowLook { art: true, album: false };

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Playing,
    Queued,
    Upcoming,
}

struct Facts {
    playing: Option<Track>,
    is_playing: bool,
    queued: Vec<Track>,
    upcoming: Vec<Track>,
    from: Option<String>,
    liked: Vec<bool>,
}

fn facts(s: &Main) -> Facts {
    s.link.read(|store| {
        let playing = store.playback.track.clone();
        let from = store.playback.context_uri.as_deref().map(|uri| known_summary(store, uri).name).filter(|name| !name.is_empty());
        let all = playing.iter().chain(&store.up_next.queued).chain(&store.up_next.upcoming);
        let liked = all.map(|track| store.liked.contains(&track.uri)).collect();
        Facts { playing, is_playing: store.playback.playing, queued: store.up_next.queued.clone(), upcoming: store.up_next.upcoming.clone(), from, liked }
    })
}

fn section(title: String, p: Palette) -> El {
    el().px(10.0).pt(18.0).pb(6.0).child(text(title).font(font::CAPS).color(p.muted).truncate())
}

fn queue_row(s: &Main, p: Palette, section: Section, i: usize, track: Track, liked: bool, is_playing: bool) -> El {
    let playing_this = section == Section::Playing;
    let facts = RowFacts { track: track.clone(), playing_this, playing: is_playing, liked, selected: false };
    let uri = track.uri.clone();
    let play = move |s: &mut Main| s.link.send(Command::SkipTo { uri: uri.clone() });
    let key = match section {
        Section::Playing => "upnext-playing".to_owned(),
        Section::Queued => format!("upnext-queued{i}"),
        Section::Upcoming => format!("upnext-next{i}"),
    };
    track_row(s, p, i, facts, LOOK, play)
        .key(key)
        .focusable(true)
        .on_secondary_press(move |s: &mut Main, press: PointerPress, _: &mut Tx| {
            let target = s.link.read(|store| match section {
                Section::Playing => playing_target(store),
                Section::Queued => up_next_target(store, true, i),
                Section::Upcoming => up_next_target(store, false, i),
            });
            s.open_menu(target, At::from(press));
        })
}

pub fn view(s: &Main, p: Palette) -> El {
    let f = facts(s);
    let header = row()
        .key("upnext-header")
        .h(52.0)
        .pl(18.0)
        .pr(10.0)
        .center_y()
        .child(text("Up next").font(font::SECTION).color(p.text).grow(1.0))
        .child(icon_button("upnext-close", "Close up next", &icons::CLOSE, 30.0, p.muted, p).on_press(|s: &mut Main| s.queue_open = false));
    let mut body = col().key("upnext-rows").w_full().px(8.0).pb(20.0).focus_group();
    let mut liked = f.liked.into_iter();
    match f.playing {
        None => body = body.child(empty_state("upnext-empty", &icons::QUEUE, "Nothing playing", "Play something to see what comes next.", p)),
        Some(track) => {
            body = body.child(section("Now playing".into(), p)).child(queue_row(s, p, Section::Playing, 0, track, liked.next().unwrap_or(false), f.is_playing));
        }
    }
    if !f.queued.is_empty() {
        body = body.child(section("Next in queue".into(), p));
        for (i, track) in f.queued.into_iter().enumerate() {
            body = body.child(queue_row(s, p, Section::Queued, i, track, liked.next().unwrap_or(false), f.is_playing));
        }
    }
    if !f.upcoming.is_empty() {
        let title = f.from.map_or_else(|| "Next up".to_owned(), |name| format!("Next from: {name}"));
        body = body.child(section(title, p));
        for (i, track) in f.upcoming.into_iter().enumerate() {
            body = body.child(queue_row(s, p, Section::Upcoming, i, track, liked.next().unwrap_or(false), f.is_playing));
        }
    }
    col()
        .key("upnext")
        .role("complementary")
        .label("Up next")
        .w(PANEL_W)
        .h_full()
        .fill(p.sidebar)
        .border_sides([0.0, 0.0, 0.0, 1.0], p.line)
        .child(header)
        .child(col().key("upnext-scroll").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).scroll_edge_fade(16.0).child(body))
}
