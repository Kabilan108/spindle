//! Search: one field, debounced, and results grouped as songs, artists, albums and playlists.

use std::sync::Arc;

use rune::{Cursor, Cx, El, ScrollSpec, col, el, icon, row, text, text_input};

use super::theme::{Palette, font};
use super::widgets::{RowFacts, RowLook, card, empty_state, heading, icon_button, track_row};
use super::{Main, icons};
use crate::model::{Command, ContextSummary, Loadable, SearchResults};

const PAD: f32 = 20.0;
const SONGS: usize = 8;
const CARDS: usize = 6;

fn field(s: &Main, p: Palette) -> El {
    let mut f = row()
        .key("search-box")
        .h(42.0)
        .max_w(560.0)
        .w_full()
        .px(14.0)
        .gap(10.0)
        .center_y()
        .radius(21.0)
        .fill(p.field)
        .cursor(Cursor::Text)
        .focus_within_ring(2.0, 1.0, p.focus)
        .child(icon(&icons::SEARCH, 18.0).color(p.muted))
        .child(
            text_input(&s.search, font::BODY)
                .key("search-field")
                .single_line()
                .grow(1.0)
                .min_w(0.0)
                .color(p.text)
                .placeholder("Songs, artists, albums, playlists", p.faint)
                .on_edit(|s: &mut Main| &mut s.search)
                .on_input_change(|s: &mut Main, tx| s.search_changed(tx)),
        );
    if !s.search.text.is_empty() {
        f = f.child(icon_button("search-clear", "Clear search", &icons::CLOSE, 26.0, p.muted, p).on_press(|s: &mut Main| s.clear_search()));
    }
    f
}

pub fn view(s: &Main, _cx: &mut Cx<Main>, p: Palette) -> El {
    let query = s.search.text.trim().to_owned();
    let results = s.link.read(|store| (store.search.query.clone(), store.search.results.clone()));
    let body = if query.is_empty() {
        empty_state("search-hint", &icons::SEARCH, "Find something to play", "Search your library and the catalogue by name.", p)
    } else {
        match results {
            (_, Loadable::Ready(found)) => results_view(s, p, found),
            (_, Loadable::Failed(error)) => empty_state("search-failed", &icons::OFFLINE, "Search didn't work", &error, p),
            (_, Loadable::Loading | Loadable::Idle) => col().px(8.0).py(16.0).child(text("Searching…").font(font::SMALL).color(p.faint)),
        }
    };
    col()
        .key("search-scroll")
        .w_full()
        .h_full()
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(16.0)
        .child(col().key("search").w_full().px(PAD).pt(24.0).pb(28.0).gap(12.0).child(row().px(8.0).child(field(s, p))).child(body))
}

fn results_view(s: &Main, p: Palette, found: SearchResults) -> El {
    let SearchResults { tracks, albums, artists, playlists } = found;
    if tracks.is_empty() && albums.is_empty() && artists.is_empty() && playlists.is_empty() {
        return empty_state("no-results", &icons::SEARCH, "No results", "Try other words, or check the spelling.", p);
    }
    let mut out = col().key("results").w_full();
    if !tracks.is_empty() {
        let uris: Arc<[String]> = tracks.iter().map(|t| t.uri.clone()).collect();
        let (playing_uri, playing, liked) = s.link.read(|store| {
            let playing_uri = store.playback.track.as_ref().map(|t| t.uri.clone());
            let liked: Vec<bool> = tracks.iter().map(|t| store.liked.contains(&t.uri)).collect();
            (playing_uri, store.playback.playing, liked)
        });
        let mut list = col().key("songs");
        for (i, track) in tracks.into_iter().take(SONGS).enumerate() {
            let uris = uris.clone();
            let facts = RowFacts {
                playing_this: playing_uri.as_deref() == Some(track.uri.as_str()),
                playing,
                liked: liked[i],
                selected: s.selected == Some(i),
                track,
            };
            let play = move |s: &mut Main| s.link.send(Command::PlayTracks { uris: uris.to_vec(), index: i });
            list = list.child(track_row(s, p, i, facts, RowLook { art: true, album: true }, play).key(format!("song{i}")));
        }
        out = out.child(section("Songs", p)).child(list);
    }
    for (title, items) in [("Artists", artists), ("Albums", albums), ("Playlists", playlists)] {
        if !items.is_empty() {
            out = out.child(section(title, p)).child(grid(s, p, title, &items));
        }
    }
    out
}

fn section(title: &str, p: Palette) -> El {
    el().px(8.0).pt(22.0).pb(8.0).child(heading(title, p))
}

fn grid(s: &Main, p: Palette, key: &str, items: &[ContextSummary]) -> El {
    let mut r = row().key(format!("grid-{key}")).flex_wrap();
    for item in items.iter().take(CARDS) {
        r = r.child(card(s, item, 172.0, p));
    }
    r
}
