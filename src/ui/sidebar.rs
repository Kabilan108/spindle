//! The sidebar: the mark, Home, Search, Liked Songs, then every playlist in a virtual list.

use std::borrow::Cow;

use rune::{Color, Cursor, El, GrowthSpec, Icon, ScrollSpec, col, el, icon, row, text};

use super::theme::{Palette, SIDEBAR_W, font};
use super::widgets::{art, fallback_icon, icon_button};
use super::{LIKED, Main, Route, icons};
use crate::model::{ContextKind, Loadable, SessionState};

const PLAYLIST_ROW_H: f32 = 36.0;

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
        .child(text("Spindle").font(font::BRAND).color(p.text));
    let nav = col()
        .key("nav")
        .px(8.0)
        .gap(2.0)
        .child(nav_item("nav-home", &icons::HOME, "Home", s.route == Route::Home, p, Main::go_home))
        .child(nav_item("nav-search", &icons::SEARCH, "Search", s.route == Route::Search, p, |s| s.go_search(true)))
        .child(nav_item("nav-liked", &icons::HEART, "Liked Songs", s.route == Route::Context(LIKED.into()), p, |s| s.open(LIKED)));

    let playlists = s.link.read(|store| match &store.playlists {
        Loadable::Ready(list) => Ok(list.len()),
        Loadable::Failed(error) => Err(Some(error.clone())),
        Loadable::Idle | Loadable::Loading => Err(None),
    });
    let count = playlists.as_ref().map_or(String::new(), |n| n.to_string());
    let header = row()
        .key("playlists-header")
        .h(28.0)
        .px(18.0)
        .mt(14.0)
        .center_y()
        .justify_between()
        .child(text("PLAYLISTS").font(font::CAPS).color(p.faint))
        .child(text(count).font(font::CAPS).color(p.faint));
    let list = match playlists {
        Ok(0) => quiet("No playlists yet", p),
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
        Err(Some(error)) => quiet(&format!("Couldn't load playlists: {error}"), p),
        Err(None) => quiet("Loading playlists…", p),
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
