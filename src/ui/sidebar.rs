//! The sidebar: the mark, Home, Search, Liked Songs, then the library in a virtual list:
//! playlists, podcasts or followed artists, whichever shelf is picked.

use std::borrow::Cow;

use rune::{Color, Cursor, El, GrowthSpec, Icon, ScrollSpec, col, el, icon, row, text};

use super::theme::{Palette, SIDEBAR_W, font};
use super::widgets::{art, fallback_icon, icon_button};
use super::{LIKED, Main, Route, icons};
use crate::model::{ContextKind, ContextSummary, EPISODES_URI, Loadable, SessionState, Store};

const PLAYLIST_ROW_H: f32 = 36.0;

/// Which part of the library the sidebar lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Shelf {
    #[default]
    Playlists,
    /// Your Episodes, then followed shows.
    Podcasts,
    Artists,
}

impl Shelf {
    const ALL: [Shelf; 3] = [Shelf::Playlists, Shelf::Podcasts, Shelf::Artists];

    fn label(self) -> &'static str {
        match self {
            Shelf::Playlists => "Playlists",
            Shelf::Podcasts => "Podcasts",
            Shelf::Artists => "Artists",
        }
    }

    fn list(self, store: &Store) -> &Loadable<Vec<ContextSummary>> {
        match self {
            Shelf::Playlists => &store.playlists,
            Shelf::Podcasts => &store.shows,
            Shelf::Artists => &store.artists,
        }
    }

    /// Rows ahead of the list's own: Your Episodes heads the podcasts.
    fn lead(self) -> usize {
        usize::from(self == Shelf::Podcasts)
    }

    fn entry(self, store: &Store, i: usize) -> Option<ContextSummary> {
        if self == Shelf::Podcasts && i == 0 {
            return Some(ContextSummary { uri: EPISODES_URI.into(), name: "Your Episodes".into(), subtitle: String::new(), image_url: None, kind: ContextKind::Episodes });
        }
        self.list(store).ready()?.get(i - self.lead()).cloned()
    }

    fn empty(self) -> &'static str {
        match self {
            Shelf::Playlists => "No playlists yet",
            Shelf::Podcasts => "No podcasts followed yet",
            Shelf::Artists => "No artists followed yet",
        }
    }
}

fn shelf_chip(shelf: Shelf, picked: bool, p: Palette) -> El {
    row()
        .key(format!("shelf-{}", shelf.label().to_lowercase()))
        .role("tab")
        .label(shelf.label())
        .checked(picked)
        .h(26.0)
        .px(9.0)
        .center()
        .radius(13.0)
        .fill(if picked { p.raised } else { Color::TRANSPARENT })
        .border(1.0, if picked { p.raised } else { p.line })
        .hover(p.hover)
        .press(p.press)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .on_press(move |s: &mut Main| s.shelf = shelf)
        .child(text(shelf.label()).font(font::SMALL).color(if picked { p.text } else { p.muted }))
}

fn nav_item(key: &str, glyph: &'static Icon, label: &str, selected: bool, p: Palette, go: fn(&mut Main)) -> El {
    let ink = if selected { p.text } else { p.muted };
    row()
        .key(key)
        .role("link")
        .label(label)
        .checked(selected)
        .h(34.0)
        .px(10.0)
        .gap(12.0)
        .center_y()
        .radius(6.0)
        .fill(if selected { p.selected } else { Color::TRANSPARENT })
        .hover(p.hover)
        .press(p.press)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .on_press(go)
        .child(icon(glyph, 18.0).color(ink))
        .child(text(label).font(font::NAV).color(ink).truncate())
}

pub fn view(s: &Main, p: Palette) -> El {
    let brand = row()
        .key("brand")
        .h(44.0)
        .px(18.0)
        .gap(10.0)
        .center_y()
        .window_drag()
        .child(icon(&icons::MARK, 18.0).color(p.accent))
        .child(text("Baliset").font(font::BRAND).color(p.text));
    let nav = col()
        .key("nav")
        .px(8.0)
        .gap(2.0)
        .child(nav_item("nav-home", &icons::HOME, "Home", s.route == Route::Home, p, Main::go_home))
        .child(nav_item("nav-search", &icons::SEARCH, "Search", s.route == Route::Search, p, |s| s.go_search(true)))
        .child(nav_item("nav-liked", &icons::HEART, "Liked Songs", s.route == Route::Context(LIKED.into()), p, |s| s.open(LIKED)));

    let shelf = s.shelf;
    let entries = s.link.read(|store| match shelf.list(store) {
        Loadable::Ready(list) => Ok(list.len() + shelf.lead()),
        Loadable::Failed(error) => Err(Some(error.clone())),
        Loadable::Idle | Loadable::Loading => Err(None),
    });
    let mut header = row().key("playlists-header").h(28.0).px(14.0).mt(14.0).mb(4.0).gap(6.0).center_y();
    for each in Shelf::ALL {
        header = header.child(shelf_chip(each, each == shelf, p));
    }
    let list = match entries {
        Ok(0) => quiet(shelf.empty(), p),
        Ok(n) if shelf != Shelf::Playlists => col()
            .key("library-scroll")
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .scroll_edge_fade(12.0)
            .child(
                col()
                    .key("library")
                    .px(8.0)
                    .pb(8.0)
                    .growth(GrowthSpec::default())
                    .virtual_rows(n, PLAYLIST_ROW_H, |s: &Main, i| Cow::Owned(format!("{:?}{i}", s.shelf)), shelf_row)
                    .virtual_rows_version((shelf, n)),
            ),
        Ok(n) => col()
            .key("playlist-scroll")
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .scroll_edge_fade(12.0)
            .child(
                col()
                    .key("playlists")
                    .px(8.0)
                    .pb(8.0)
                    .growth(GrowthSpec::default())
                    .virtual_rows(n, PLAYLIST_ROW_H, |_: &Main, i| Cow::Owned(format!("pl{i}")), playlist_row)
                    .virtual_rows_version(n),
            ),
        Err(Some(error)) => quiet(&format!("Couldn't load {}: {error}", shelf.label().to_lowercase()), p),
        Err(None) => quiet(&format!("Loading {}…", shelf.label().to_lowercase()), p),
    };
    col()
        .key("sidebar")
        .w(SIDEBAR_W)
        .h_full()
        .min_h(0.0)
        .fill(p.sidebar)
        .child(brand)
        .child(nav)
        .child(header)
        .child(list)
        .child(account(s, p))
}

/// Who is signed in, and the way out. Signing out takes two presses, so a stray click
/// doesn't stop the music and forget the account.
fn account(s: &Main, p: Palette) -> El {
    let user = s.link.read(|store| match &store.session {
        SessionState::Online { user } => Some(user.clone()),
        _ => None,
    });
    let Some(user) = user else { return el().key("account") };
    let footer = row().key("account").h(44.0).px(18.0).gap(10.0).center_y();
    if s.confirming_sign_out {
        let choice = |key: &str, label: &str, ink: Color| {
            row().key(key).role("button").label(label).h(26.0).px(10.0).center().radius(13.0).hover(p.hover).press(p.press).cursor(Cursor::Pointer).focusable(true).child(text(label).font(font::SMALL).color(ink))
        };
        return footer
            .child(text("Sign out?").font(font::NAV).color(p.text).grow(1.0).min_w(0.0))
            .child(choice("sign-out-cancel", "Cancel", p.muted).on_press(|s: &mut Main| s.confirming_sign_out = false))
            .child(choice("sign-out-confirm", "Sign out", p.accent).on_press(Main::sign_out));
    }
    footer
        .child(icon(&icons::PERSON, 18.0).color(p.muted))
        .child(text(user).key("account-name").font(font::NAV).color(p.muted).truncate().grow(1.0).min_w(0.0))
        .child(icon_button("sign-out", "Sign out", &icons::SIGN_OUT, 28.0, p.muted, p).on_press(|s: &mut Main| s.confirming_sign_out = true))
}

fn quiet(message: &str, p: Palette) -> El {
    col().key("playlists-note").px(18.0).py(6.0).child(text(message.to_owned()).font(font::SMALL).color(p.faint).wrap())
}

fn playlist_row(s: &Main, i: usize) -> El {
    let p = Palette::new(s.dark.get());
    let Some(item) = s.link.read(|store| store.playlists.ready().and_then(|list| list.get(i).cloned())) else {
        return el().h(PLAYLIST_ROW_H);
    };
    let selected = s.route == Route::Context(item.uri.clone());
    let uri = item.uri.clone();
    row()
        .role("link")
        .label(item.name.clone())
        .checked(selected)
        .h(PLAYLIST_ROW_H)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .radius(6.0)
        .fill(if selected { p.selected } else { Color::TRANSPARENT })
        .hover(p.hover)
        .press(p.press)
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut Main| s.open(&uri))
        .child(art(&s.art, &s.link, item.image_url.as_deref(), 26.0, 4.0, p, fallback_icon(ContextKind::Playlist)))
        .child(text(item.name).font(font::NAV).color(if selected { p.text } else { p.muted }).truncate().grow(1.0).min_w(0.0))
}

/// A podcast or a followed artist. Artists are round; Your Episodes gets the saved mark.
fn shelf_row(s: &Main, i: usize) -> El {
    let p = Palette::new(s.dark.get());
    let shelf = s.shelf;
    let Some(item) = s.link.read(|store| shelf.entry(store, i)) else {
        return el().h(PLAYLIST_ROW_H);
    };
    let selected = s.route == Route::Context(item.uri.clone());
    let cover = match item.kind {
        ContextKind::Episodes => el().size(26.0, 26.0).radius(4.0).gradient(p.accent.mix(p.bg, 0.25), p.accent.mix(p.bg, 0.7)).center().child(icon(&icons::BOOKMARK_FILLED, 13.0).color(p.on_accent)),
        ContextKind::Artist => art(&s.art, &s.link, item.image_url.as_deref(), 26.0, 13.0, p, fallback_icon(item.kind)),
        _ => art(&s.art, &s.link, item.image_url.as_deref(), 26.0, 4.0, p, fallback_icon(item.kind)),
    };
    let uri = item.uri.clone();
    row()
        .role("link")
        .label(item.name.clone())
        .checked(selected)
        .h(PLAYLIST_ROW_H)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .radius(6.0)
        .fill(if selected { p.selected } else { Color::TRANSPARENT })
        .hover(p.hover)
        .press(p.press)
        .cursor(Cursor::Pointer)
        .on_press(move |s: &mut Main| s.open(&uri))
        .child(cover)
        .child(text(item.name).font(font::NAV).color(if selected { p.text } else { p.muted }).truncate().grow(1.0).min_w(0.0))
}
