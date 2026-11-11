//! Home: recently played, then saved albums, as a card grid cut into virtual rows so a library
//! of hundreds of albums builds only the rows on screen.

use std::borrow::Cow;

use rune::{Cx, El, GrowthSpec, ScrollSpec, col, row, text};

use super::theme::{Palette, SIDEBAR_W, font};
use super::widgets::{card, empty_state, heading};
use super::{HomeGrid, MAIN_SIZE, Main, icons};
use crate::model::Loadable;

const PAD: f32 = 20.0;
const MIN_CARD: f32 = 156.0;
const MAX_CARD: f32 = 200.0;

fn grid(width: f32, recents: usize, albums: usize) -> HomeGrid {
    let avail = (width - 2.0 * PAD).max(MIN_CARD);
    let cols = ((avail / MIN_CARD).floor() as usize).max(1);
    let card_w = (avail / cols as f32).floor().min(MAX_CARD);
    HomeGrid { cols, card_w, recent_rows: recents.div_ceil(cols), album_rows: albums.div_ceil(cols) }
}

enum Item {
    Heading(&'static str),
    Recents(usize),
    Albums(usize),
    NoRecents,
    AlbumStatus,
}

fn item(g: HomeGrid, i: usize) -> Item {
    let recent_rows = g.recent_rows.max(1);
    match i {
        0 => Item::Heading("Recently played"),
        i if i <= recent_rows => {
            if g.recent_rows == 0 { Item::NoRecents } else { Item::Recents(i - 1) }
        }
        i if i == recent_rows + 1 => Item::Heading("Your albums"),
        i => {
            if g.album_rows == 0 { Item::AlbumStatus } else { Item::Albums(i - recent_rows - 2) }
        }
    }
}

pub fn view(s: &Main, cx: &mut Cx<Main>, _p: Palette) -> El {
    let width = cx.layout_size("home-scroll").map_or(MAIN_SIZE.0 - SIDEBAR_W, |size| size[0]);
    let (recents, albums) = s.link.read(|store| (store.recents.len(), store.albums.ready().map_or(0, Vec::len)));
    let g = grid(width, recents, albums);
    s.home.set(g);
    let count = 2 + g.recent_rows.max(1) + g.album_rows.max(1);
    let estimate = g.card_w + 60.0;
    col()
        .key("home-scroll")
        .w_full()
        .h_full()
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(16.0)
        .child(
            col()
                .key("home")
                .px(PAD)
                .pb(28.0)
                .growth(GrowthSpec::default())
                .virtual_rows(count, estimate, |_: &Main, i| Cow::Owned(format!("home{i}")), home_row)
                .virtual_rows_version((g.cols, g.recent_rows, g.album_rows, g.card_w.to_bits())),
        )
}

fn home_row(s: &Main, i: usize) -> El {
    let p = Palette::new(s.dark.get());
    let g = s.home.get();
    match item(g, i) {
        Item::Heading(title) => row().px(8.0).pt(if i == 0 { 22.0 } else { 26.0 }).pb(10.0).child(heading(title, p)),
        Item::Recents(r) => {
            let items = s.link.read(|store| store.recents.iter().skip(r * g.cols).take(g.cols).cloned().collect::<Vec<_>>());
            cards(s, &items, g, p)
        }
        Item::Albums(r) => {
            let items = s.link.read(|store| {
                store.albums.ready().map(|list| list.iter().skip(r * g.cols).take(g.cols).cloned().collect::<Vec<_>>()).unwrap_or_default()
            });
            cards(s, &items, g, p)
        }
        Item::NoRecents => empty_state("no-recents", &icons::NOTE, "Nothing played yet", "What you play shows up here.", p),
        Item::AlbumStatus => {
            let status = s.link.read(|store| match &store.albums {
                Loadable::Failed(error) => Some(error.clone()),
                Loadable::Ready(_) => None,
                Loadable::Idle | Loadable::Loading => Some(String::new()),
            });
            match status {
                None => empty_state("no-albums", &icons::LIBRARY, "No saved albums", "Albums you save show up here.", p),
                Some(error) if error.is_empty() => col().px(8.0).py(12.0).child(text("Loading your albums…").font(font::SMALL).color(p.faint)),
                Some(error) => empty_state("albums-failed", &icons::OFFLINE, "Couldn't load your albums", &error, p),
            }
        }
    }
}

fn cards(s: &Main, items: &[crate::model::ContextSummary], g: HomeGrid, p: Palette) -> El {
    let mut r = row().pb(4.0);
    for item in items {
        r = r.child(card(s, item, g.card_w, p));
    }
    r
}
