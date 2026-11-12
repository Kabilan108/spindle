//! Pieces the pages share: art, buttons, cards, track rows, headings, empty states.

use rune::{Align, Color, Cursor, El, Icon, SurfaceFit, Transition, col, el, icon, row, text};

use super::art::Art;
use super::icons;
use super::link::Link;
use super::meter::fmt_time;
use super::theme::{BAR_H, Palette, ROW_H, font};
use super::Main;
use crate::model::{Command, ContextKind, ContextSummary, NamedRef, Track};

/// A square of art, or a quiet placeholder while it loads or when there is none.
pub fn art(cache: &Art, link: &Link, url: Option<&str>, size: f32, radius: f32, p: Palette, fallback: &'static Icon) -> El {
    let base = el().size(size, size).radius(radius).fill(p.art_empty).center();
    match url.and_then(|url| cache.pixels(link, url)) {
        Some(pixels) => base.pixels_fit(&pixels, SurfaceFit::Fill),
        None => base.child(icon(fallback, (size * 0.4).round().max(10.0)).color(p.faint)),
    }
}

pub fn fallback_icon(kind: ContextKind) -> &'static Icon {
    match kind {
        ContextKind::Artist => &icons::PERSON,
        ContextKind::Liked => &icons::HEART_FILLED,
        ContextKind::Playlist => &icons::LIBRARY,
        ContextKind::Show => &icons::PODCAST,
        ContextKind::Episodes => &icons::BOOKMARK_FILLED,
        _ => &icons::NOTE,
    }
}

/// A round, borderless button around one icon.
pub fn icon_button(key: impl Into<String>, label: &str, glyph: &'static Icon, size: f32, color: Color, p: Palette) -> El {
    el().key(key)
        .role("button")
        .label(label)
        .size(size, size)
        .radius(size / 2.0)
        .center()
        .cursor(Cursor::Pointer)
        .focusable(true)
        .hover(p.hover)
        .press(p.press)
        .child(icon(glyph, (size * 0.56).round()).color(color))
}

/// A pill with a label, filled with the accent when `primary`.
pub fn pill_button(key: impl Into<String>, label: &str, glyph: Option<&'static Icon>, primary: bool, p: Palette) -> El {
    let (fill, ink) = if primary { (p.accent, p.on_accent) } else { (p.raised, p.text) };
    let mut b = row()
        .key(key)
        .role("button")
        .label(label)
        .h(36.0)
        .px(18.0)
        .gap(8.0)
        .center()
        .radius(18.0)
        .fill(fill)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .hover(p.hover)
        .press(p.press);
    if let Some(glyph) = glyph {
        b = b.child(icon(glyph, 16.0).color(ink));
    }
    b.child(text(label).font(font::BUTTON).color(ink))
}

pub fn heading(title: &str, p: Palette) -> El {
    text(title).font(font::SECTION).color(p.text)
}

/// A calm message where content would be.
pub fn empty_state(key: &str, glyph: &'static Icon, title: &str, body: &str, p: Palette) -> El {
    col().key(key)
        .w_full()
        .py(48.0)
        .gap(8.0)
        .items_center()
        .child(icon(glyph, 28.0).color(p.faint))
        .child(text(title).font(font::CARD_TITLE).color(p.text))
        .child(text(body).font(font::SMALL).color(p.muted).align(Align::Center).max_w(360.0))
}

pub fn toast(p: Palette, message: &str) -> El {
    row()
        .key("toast")
        .pin(Some(0.0), None, Some(0.0), Some(BAR_H + 16.0))
        .justify_center()
        .transition(Transition::offset(0.0, 10.0))
        .child(
            row()
                .key("toast-body")
                .h(40.0)
                .max_w(560.0)
                .pl(16.0)
                .pr(6.0)
                .gap(10.0)
                .center_y()
                .radius(20.0)
                .fill(p.toast)
                .shadow(6.0, 24.0, p.shadow)
                .role("status")
                .child(text(message).font(font::BODY).color(p.toast_text).truncate().shrink())
                .child(
                    icon_button("toast-dismiss", "Dismiss", &icons::CLOSE, 28.0, p.toast_text, p)
                        .hover(Color::WHITE.alpha(0.08))
                        .on_press(|s: &mut Main| s.link.send(Command::DismissNotice)),
                ),
        )
}

/// A grid card: art over a name and a subtitle. Pressing it opens the page; the round button
/// that shows on hover plays it.
pub fn card(s: &Main, item: &ContextSummary, w: f32, p: Palette) -> El {
    let inner = w - 16.0;
    let round = item.kind == ContextKind::Artist;
    let radius = if round { inner / 2.0 } else { 6.0 };
    let (open_uri, play_uri, menu_item) = (item.uri.clone(), item.uri.clone(), item.clone());
    let play = el()
        .key("play")
        .role("button")
        .label(format!("Play {}", item.name))
        .size(40.0, 40.0)
        .radius(20.0)
        .center()
        .fill(p.accent)
        .shadow(4.0, 14.0, p.shadow)
        .cursor(Cursor::Pointer)
        .pin(None, None, Some(8.0), Some(8.0))
        .group_opacity(0.0, 1.0)
        .hover(Color::WHITE.alpha(0.12))
        .on_press(move |s: &mut Main| s.play_context(&play_uri, None))
        .child(icon(&icons::PLAY, 18.0).color(p.on_accent));
    let cover = el()
        .key("cover")
        .size(inner, inner)
        .child(art(&s.art, &s.link, item.image_url.as_deref(), inner, radius, p, fallback_icon(item.kind)).shadow(4.0, 16.0, p.shadow))
        .child(play);
    col()
        .key(format!("card:{}", item.uri))
        .role("button")
        .label(item.name.clone())
        .w(w)
        .padding(8.0)
        .gap(4.0)
        .radius(8.0)
        .hover(p.hover)
        .press(p.press)
        .hover_group()
        .cursor(Cursor::Pointer)
        .focusable(true)
        .on_press(move |s: &mut Main| s.open(&open_uri))
        .on_secondary_press(move |s: &mut Main, press: rune::PointerPress, _: &mut rune::Tx| s.open_card_menu(menu_item.clone(), press.into()))
        .child(cover.mb(6.0))
        .child(text(item.name.clone()).font(font::CARD_TITLE).color(p.text).truncate())
        .child(text(item.subtitle.clone()).font(font::SMALL).color(p.muted).truncate())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RowLook {
    pub art: bool,
    pub album: bool,
}

/// What a row's facts are, read under one lock.
pub struct RowFacts {
    pub track: Track,
    pub playing_this: bool,
    pub playing: bool,
    pub liked: bool,
    pub selected: bool,
}

/// A 40 pt track row. `play` runs on a double click (or Return on the list) and from the
/// number cell's play button.
pub fn track_row(s: &Main, p: Palette, i: usize, facts: RowFacts, look: RowLook, play: impl Fn(&mut Main) + Clone + 'static) -> El {
    let RowFacts { track: t, playing_this, playing, liked, selected } = facts;
    let title_color = if playing_this { p.accent } else { p.text };
    let number = if playing_this {
        el().child(icon(&icons::PLAYING, 14.0).color(p.accent)).group_opacity(1.0, 0.0)
    } else {
        text((i + 1).to_string()).font(font::NUMBER).color(p.faint).group_opacity(1.0, 0.0)
    };
    let overlay_icon = if playing_this && playing { &icons::PAUSE } else { &icons::PLAY };
    let overlay = {
        let play = play.clone();
        el().key("row-play")
            .role("button")
            .label(if playing_this && playing { "Pause" } else { "Play" })
            .size(28.0, 28.0)
            .absolute(0.0, 0.0)
            .center()
            .group_opacity(0.0, 1.0)
            .on_press(move |s: &mut Main| {
                if playing_this { s.link.send(Command::PlayPause) } else { play(s) }
            })
            .child(icon(overlay_icon, 13.0).color(p.text))
    };
    let number_cell = el().key("num").size(28.0, 28.0).center().child(number).child(overlay);

    let mut subtitle = row().gap(6.0).center_y().min_w(0.0);
    if t.explicit {
        subtitle = subtitle.child(
            el().h(14.0).px(4.0).radius(3.0).fill(p.raised).center().child(text("E").font(font::CAPS).color(p.muted)),
        );
    }
    subtitle = subtitle.child(artist_links(format!("artist{i}"), &t.artists, font::ROW_SUB, p.muted, p.text));
    let titles = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(1.0)
        .child(text(t.name.clone()).font(font::ROW_TITLE).color(title_color).truncate())
        .child(subtitle);

    let mut r = row()
        .h(ROW_H)
        .px(10.0)
        .gap(12.0)
        .center_y()
        .radius(6.0)
        .fill(if selected { p.selected } else { Color::TRANSPARENT })
        .hover(p.hover)
        .hover_group()
        .cursor(Cursor::Pointer)
        .role("row")
        .label(format!("{}, {}", t.name, t.artist_line()))
        .checked(selected)
        .on_press(move |s: &mut Main| s.selected = Some(i))
        .child(number_cell);
    if look.art {
        r = r.child(art(&s.art, &s.link, t.image_url.as_deref(), 32.0, 4.0, p, &icons::NOTE));
    }
    r = r.child(titles);
    if look.album {
        let album = t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        r = r.child(text(album).font(font::SMALL).color(p.muted).truncate().grow(0.7).min_w(0.0));
    }
    let uri = t.uri.clone();
    let heart = if liked { &icons::HEART_FILLED } else { &icons::HEART };
    let mut like = icon_button("like", if liked { "Remove from Liked Songs" } else { "Save to Liked Songs" }, heart, 28.0, if liked { p.accent } else { p.muted }, p)
        .on_press(move |s: &mut Main| s.set_liked(&uri, !liked));
    if !liked {
        like = like.group_opacity(0.0, 1.0);
    }
    r = r.child(like).child(text(fmt_time(t.duration_ms)).font(font::NUMBER).color(p.muted).w(44.0).align(Align::End));
    if t.playable {
        r.on_double_press(play)
    } else {
        r.opacity(0.45)
    }
}

/// The column labels over a track list.
pub fn track_header(p: Palette, look: RowLook) -> El {
    let label = |s: &'static str| text(s).font(font::CAPS).color(p.faint);
    let mut r = row().key("columns").h(32.0).px(10.0).gap(12.0).center_y().child(label("#").w(28.0).align(Align::Center));
    if look.art {
        r = r.child(el().w(32.0));
    }
    r = r.child(label("TITLE").grow(1.0).min_w(0.0));
    if look.album {
        r = r.child(label("ALBUM").grow(0.7).min_w(0.0));
    }
    r.child(el().w(28.0)).child(label("TIME").w(44.0).align(Align::End))
}

/// Artist names in a line, each one a link to its page. A name without a URI (a podcast's
/// show, a playlist's owner) is plain text.
pub fn artist_links(key: impl Into<String>, artists: &[NamedRef], style: rune::TextStyle, rest: Color, hovered: Color) -> El {
    let key = key.into();
    let mut line = row().min_w(0.0).shrink();
    for (i, artist) in artists.iter().enumerate() {
        if i > 0 {
            line = line.child(text(", ").font(style).color(rest));
        }
        let name_key = if i == 0 { key.clone() } else { format!("{key}-{i}") };
        let name = text(artist.name.clone()).key(name_key).font(style).color(rest).truncate().shrink();
        line = line.child(if artist.uri.is_empty() {
            name
        } else {
            let uri = artist.uri.clone();
            el().min_w(0.0)
                .shrink()
                .role("link")
                .label(artist.name.clone())
                .cursor(Cursor::Pointer)
                .hover_group()
                .on_press(move |s: &mut Main| s.open(&uri))
                .child(name.group_color(hovered))
        });
    }
    line
}
