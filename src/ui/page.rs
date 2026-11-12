//! A playlist, album, artist or Liked Songs: a header, then the tracks as virtual rows (a
//! playlist can hold thousands), and an artist's albums.

use std::borrow::Cow;

use rune::{Cursor, Cx, El, GrowthSpec, Key, KeyEvent, PointerPress, ScrollSpec, Tx, col, el, icon, row, text};

use super::theme::{Palette, ROW_H, font};
use super::widgets::{RowFacts, RowLook, art, card, empty_state, fallback_icon, heading, pill_button, track_header, track_row};
use super::menu::{self, At};
use super::{LIKED, Main, icons, viz};
use crate::model::{Command, ContextKind, ContextPage, ContextSummary, Loadable, Store};

const PAD: f32 = 20.0;
const ART: f32 = 176.0;

enum Shown {
    Loading(ContextSummary),
    Failed(ContextSummary, String),
    Ready { summary: ContextSummary, count: usize, total_ms: u64, truncated: bool, albums: Vec<ContextSummary> },
}

/// What the page is, before its tracks arrive: from the library lists, or a bare guess.
pub(super) fn known_summary(store: &Store, uri: &str) -> ContextSummary {
    if uri == LIKED {
        return ContextSummary { uri: uri.into(), name: "Liked Songs".into(), subtitle: String::new(), image_url: None, kind: ContextKind::Liked };
    }
    let lists = [store.playlists.ready(), store.albums.ready(), Some(&store.recents)];
    lists
        .into_iter()
        .flatten()
        .flat_map(|list| list.iter())
        .find(|item| item.uri == uri)
        .cloned()
        .unwrap_or_else(|| ContextSummary { uri: uri.into(), ..ContextSummary::default() })
}

fn shown(store: &Store, uri: &str) -> Shown {
    match store.pages.get(uri) {
        Some(Loadable::Ready(page)) => {
            let ContextPage { summary, tracks, albums, truncated } = page;
            let total_ms = tracks.iter().map(|t| u64::from(t.duration_ms)).sum();
            Shown::Ready { summary: summary.clone(), count: tracks.len(), total_ms, truncated: *truncated, albums: albums.clone() }
        }
        Some(Loadable::Failed(error)) => Shown::Failed(known_summary(store, uri), error.clone()),
        _ => Shown::Loading(known_summary(store, uri)),
    }
}

fn kind_label(kind: ContextKind) -> &'static str {
    match kind {
        ContextKind::Playlist => "PLAYLIST",
        ContextKind::Album => "ALBUM",
        ContextKind::Artist => "ARTIST",
        ContextKind::Liked => "COLLECTION",
        ContextKind::Show => "PODCAST",
        ContextKind::Other => "",
        ContextKind::Episodes => "COLLECTION",
    }
}

fn length(total_ms: u64) -> String {
    let minutes = total_ms / 60_000;
    if minutes >= 60 { format!("{} hr {} min", minutes / 60, minutes % 60) } else { format!("{minutes} min") }
}

fn look(kind: ContextKind) -> RowLook {
    match kind {
        ContextKind::Album => RowLook { art: false, album: false },
        ContextKind::Artist => RowLook { art: true, album: true },
        _ => RowLook { art: true, album: true },
    }
}

pub fn view(s: &Main, _cx: &mut Cx<Main>, p: Palette, uri: &str) -> El {
    let (shown, playing_here) = s.link.read(|store| {
        let playback = &store.playback;
        (shown(store, uri), playback.playing && playback.context_uri.as_deref() == Some(uri))
    });
    let mut content = col().key("page").w_full().pb(28.0);
    match shown {
        Shown::Loading(summary) => {
            content = content.child(header(s, p, &summary, String::new(), playing_here)).child(
                col().px(PAD + 10.0).py(16.0).child(text("Loading…").font(font::SMALL).color(p.faint)),
            );
        }
        Shown::Failed(summary, error) => {
            let retry_uri = uri.to_owned();
            content = content
                .child(header(s, p, &summary, String::new(), false))
                .child(empty_state("page-failed", &icons::OFFLINE, "Couldn't load this page", &error, p))
                .child(
                    row().justify_center().child(
                        pill_button("retry", "Try again", None, false, p)
                            .on_press(move |s: &mut Main| s.link.send(Command::LoadContext { uri: retry_uri.clone() })),
                    ),
                );
        }
        Shown::Ready { summary, count, total_ms, truncated, albums } => {
            let mut meta = vec![];
            if !summary.subtitle.is_empty() && summary.kind != ContextKind::Artist {
                meta.push(summary.subtitle.clone());
            }
            if count > 0 {
                let plus = if truncated { "+" } else { "" };
                meta.push(format!("{count}{plus} {}", if count == 1 { "song" } else { "songs" }));
                meta.push(length(total_ms));
            }
            content = content.child(header(s, p, &summary, meta.join("  ·  "), playing_here));
            if summary.kind == ContextKind::Artist {
                content = content.child(artist_body(s, p, uri, count, &albums));
            } else if count == 0 {
                let body = if summary.kind == ContextKind::Liked { "Songs you like show up here." } else { "Nothing here yet." };
                content = content.child(empty_state("no-tracks", &icons::NOTE, "No songs", body, p));
            } else {
                content = content.child(track_list(p, uri, count, look(summary.kind)));
                if truncated {
                    content = content.child(
                        text(format!("Showing the first {count} songs.")).font(font::SMALL).color(p.faint).px(PAD + 10.0).pt(12.0),
                    );
                }
            }
        }
    }
    col()
        .key(format!("page:{uri}"))
        .w_full()
        .h_full()
        .scroll(ScrollSpec { stick_bottom: false })
        .scroll_edge_fade(16.0)
        .child(content)
}

fn header(s: &Main, p: Palette, summary: &ContextSummary, meta: String, playing_here: bool) -> El {
    let cover = if summary.kind == ContextKind::Liked {
        el().size(ART, ART)
            .radius(8.0)
            .gradient(p.accent, p.accent.mix(p.bg, 0.55))
            .center()
            .child(icon(&icons::HEART_FILLED, 64.0).color(p.on_accent))
    } else {
        let radius = if summary.kind == ContextKind::Artist { ART / 2.0 } else { 8.0 };
        art(&s.art, &s.link, summary.image_url.as_deref(), ART, radius, p, fallback_icon(summary.kind))
    };
    let uri = summary.uri.clone();
    let (label, glyph) = if playing_here { ("Pause", &icons::PAUSE) } else { ("Play", &icons::PLAY) };
    let mut info = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(6.0)
        .child(text(kind_label(summary.kind)).font(font::CAPS).color(p.muted))
        .child(text(summary.name.clone()).key("page-title").font(font::PAGE_TITLE).color(p.text).max_lines(2));
    if !meta.is_empty() {
        info = info.child(text(meta).font(font::SMALL).color(p.muted).truncate());
    }
    let mut buttons = row()
        .mt(10.0)
        .gap(10.0)
        .center_y()
        .child(pill_button("page-play", label, Some(glyph), true, p).on_press(move |s: &mut Main| s.play_or_toggle(&uri)))
        .child(shuffle_button(s, p, &summary.uri))
        .children(radio_button(p, summary));
    if summary.kind == ContextKind::Artist && !summary.uri.is_empty() {
        let uri = summary.uri.clone();
        let followed = s.link.read(|store| store.follows(&uri));
        let (label, glyph) = if followed { ("Following", Some(&icons::CHECK)) } else { ("Follow", None) };
        buttons = buttons.child(
            pill_button("follow", label, glyph, false, p).checked(followed).on_press(move |s: &mut Main| s.link.send(Command::SetFollowed { uri: uri.clone(), followed: !followed })),
        );
    }
    info = info.child(buttons);
    let head = row()
        .key("page-header")
        .px(PAD + 8.0)
        .pt(28.0)
        .pb(32.0)
        .gap(24.0)
        .items_end()
        .child(cover.shadow(6.0, 20.0, p.shadow))
        .child(info);
    match viz::header_palette(s, summary.image_url.as_deref(), summary.kind) {
        Some(palette) => col().key("page-head").w_full().child(viz::header::ambient(s, p, &palette)).child(head),
        None => head,
    }
}

/// Shuffle beside Play: starts the page shuffled, or toggles shuffle while it plays.
fn shuffle_button(s: &Main, p: Palette, uri: &str) -> El {
    let on = s.link.read(|store| store.playback.shuffle && store.playback.context_uri.as_deref() == Some(uri));
    let uri = uri.to_owned();
    el().key("page-shuffle")
        .role("switch")
        .label("Shuffle")
        .checked(on)
        .size(36.0, 36.0)
        .radius(18.0)
        .center()
        .fill(if on { p.accent.alpha(0.12) } else { rune::Color::TRANSPARENT })
        .border(1.0, if on { p.accent } else { p.line })
        .cursor(Cursor::Pointer)
        .focusable(true)
        .hover(p.hover)
        .press(p.press)
        .on_press(move |s: &mut Main| s.shuffle_or_toggle(&uri))
        .child(icon(&icons::SHUFFLE, 18.0).color(if on { p.accent } else { p.muted }))
}

fn radio_button(p: Palette, summary: &ContextSummary) -> Option<El> {
    let station = menu::artist_radio(summary)?;
    Some(pill_button("page-radio", "Radio", Some(&icons::RADIO), false, p).on_press(move |s: &mut Main| s.play_context(&station, None)))
}

fn row_facts(store: &Store, uri: &str, i: usize, selected: Option<usize>) -> Option<RowFacts> {
    let track = store.pages.get(uri)?.ready()?.tracks.get(i)?.clone();
    let playback = &store.playback;
    let playing_this = playback.track.as_ref().is_some_and(|t| t.uri == track.uri);
    Some(RowFacts {
        playing_this,
        playing: playback.playing,
        liked: store.liked.contains(&track.uri),
        selected: selected == Some(i),
        track,
    })
}

fn page_row(s: &Main, uri: &str, i: usize, look: RowLook) -> El {
    let p = Palette::new(s.dark.get());
    let Some(facts) = s.link.read(|store| row_facts(store, uri, i, s.selected)) else {
        return el().h(ROW_H);
    };
    let (context, track_uri) = (uri.to_owned(), facts.track.uri.clone());
    let menu_uri = uri.to_owned();
    track_row(s, p, i, facts, look, move |s: &mut Main| s.play_context(&context, Some(track_uri.clone()))).on_secondary_press(
        move |s: &mut Main, press: PointerPress, _: &mut Tx| {
            s.selected = Some(i);
            let target = s.link.read(|store| menu::page_target(store, &menu_uri, i));
            s.open_menu(target, At::from(press));
        },
    )
}

fn track_list(p: Palette, uri: &str, count: usize, look: RowLook) -> El {
    let (rows_uri, key_uri) = (uri.to_owned(), uri.to_owned());
    col()
        .key("tracklist")
        .px(PAD)
        .role("list")
        .label("Tracks")
        .focusable(true)
        .on_key(move |s: &mut Main, ev: &KeyEvent, tx: &mut Tx| list_key(s, ev, tx, &key_uri, count))
        .child(track_header(p, look).fill(p.bg).sticky_top(0.0).stuck_shadow(rune::Shadow { y: 1.0, color: p.line, ..rune::Shadow::default() }))
        .child(
            col()
                .key("tracks")
                .growth(GrowthSpec::default())
                .virtual_rows(count, ROW_H, |_: &Main, i| Cow::Owned(format!("trk{i}")), move |s: &Main, i| page_row(s, &rows_uri, i, look))
                .virtual_rows_version((uri.to_owned(), count)),
        )
}

/// Up and Down move the selection, Return plays it. Space goes on to the window.
pub(super) fn list_key(s: &mut Main, ev: &KeyEvent, tx: &mut Tx, uri: &str, count: usize) -> bool {
    if count == 0 || ev.mods.cmd || ev.mods.alt {
        return false;
    }
    if menu::is_menu_key(ev) {
        let i = s.selected.unwrap_or(0).min(count - 1);
        s.selected = Some(i);
        tx.scroll_into_view(format!("trk{i}"));
        let target = s.link.read(|store| menu::page_target(store, uri, i));
        s.open_menu(target, At::Below(format!("trk{i}")));
        return true;
    }
    let next = match ev.key {
        Key::Down => s.selected.map_or(0, |i| (i + 1).min(count - 1)),
        Key::Up => s.selected.map_or(0, |i| i.saturating_sub(1)),
        Key::Home => 0,
        Key::End => count - 1,
        Key::Enter => {
            let Some(i) = s.selected else { return false };
            let track_uri = s.link.read(|store| store.pages.get(uri)?.ready()?.tracks.get(i).map(|t| t.uri.clone()));
            if let Some(track_uri) = track_uri {
                s.play_context(uri, Some(track_uri));
            }
            return true;
        }
        _ => return false,
    };
    s.selected = Some(next);
    tx.scroll_into_view(format!("trk{next}"));
    true
}

fn artist_body(s: &Main, p: Palette, uri: &str, count: usize, albums: &[ContextSummary]) -> El {
    let look = RowLook { art: true, album: true };
    let mut body = col().key("artist").w_full();
    if count > 0 {
        body = body.child(heading("Popular", p).px(PAD + 8.0).pb(8.0));
        let mut list = col().key("popular").px(PAD).focus_group();
        for i in 0..count {
            list = list.child(page_row(s, uri, i, look).key(format!("trk{i}")).focusable(true));
        }
        body = body.child(list);
    }
    if !albums.is_empty() {
        let mut grid = row().key("discography").px(PAD).flex_wrap();
        for album in albums {
            grid = grid.child(card(s, album, 188.0, p));
        }
        body = body.child(heading("Albums", p).px(PAD + 8.0).pt(28.0).pb(8.0)).child(grid);
    }
    body
}
