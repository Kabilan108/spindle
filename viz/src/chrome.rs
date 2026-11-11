//! The parts of Baliset the concepts sit in: the mini player and the main window, rebuilt from
//! `src/ui/mini.rs`, `bar.rs`, `sidebar.rs` and `home.rs` closely enough to judge the concepts in
//! place. Nothing here is interactive.

use rune::{El, SurfaceFit, col, el, icon, row, spacer, text};

use crate::art::Cover;
use crate::icons;
use crate::theme::{BAR_H, Palette, SIDEBAR_W, font};
use crate::State;

pub const MINI_SIZE: (f32, f32) = (380.0, 208.0);
pub const MINI_PAD: f32 = 12.0;
pub const RECENTS: [Cover; 6] =
    [Cover::LowTide, Cover::Gradient(0), Cover::ConcreteGarden, Cover::Gradient(2), Cover::Gradient(1), Cover::CarrierWave];

pub fn cover(s: &State, c: Cover, size: f32, radius: f32) -> El {
    el().size(size, size).radius(radius).pixels_fit(s.arts.pixels(c), SurfaceFit::Fill)
}

pub fn icon_button(glyph: &'static rune::Icon, size: f32, color: rune::Color) -> El {
    el().size(size, size).radius(size / 2.0).center().child(icon(glyph, (size * 0.56).round()).color(color))
}

pub fn play_button(p: Palette, playing: bool, size: f32) -> El {
    let glyph = if playing { &icons::PAUSE } else { &icons::PLAY };
    el().key("play").size(size, size).radius(size / 2.0).center().fill(p.text).child(icon(glyph, (size * 0.47).round()).color(p.bg))
}

pub fn mini_frame(p: Palette) -> El {
    col().key("mini").w_full().h_full().padding(MINI_PAD).gap(10.0).fill(p.bg).border(1.0, p.line)
}

/// The mini player's top row: art, title and artist, the open and hide buttons.
pub fn mini_top(s: &State, p: Palette, art: El) -> El {
    let info = s.track.info();
    row()
        .key("mini-top")
        .h(64.0)
        .gap(12.0)
        .center_y()
        .child(art)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(3.0)
                .child(text(info.title).font(font::CARD_TITLE).color(p.text).truncate())
                .child(text(info.artist).font(font::SMALL).color(p.muted).truncate()),
        )
        .child(
            col()
                .self_start()
                .gap(2.0)
                .child(icon_button(&icons::EXPAND, 26.0, p.muted))
                .child(icon_button(&icons::CLOSE, 26.0, p.muted)),
        )
}

pub fn mini_controls(p: Palette, playing: bool) -> El {
    row()
        .key("mini-controls")
        .h(34.0)
        .center_y()
        .child(icon_button(&icons::HEART_FILLED, 30.0, p.accent))
        .child(spacer())
        .child(icon_button(&icons::PREVIOUS, 32.0, p.text))
        .child(play_button(p, playing, 34.0).mx(10.0))
        .child(icon_button(&icons::NEXT, 32.0, p.text))
        .child(spacer())
        .child(el().size(30.0, 30.0))
}

/// The thin progress bar; its fill is a live rect the poll moves, as `meter::Progress` does.
pub fn mini_progress(s: &State, p: Palette, width: f32) -> El {
    el().key("mini-progress")
        .w(width)
        .h(3.0)
        .radius(1.5)
        .fill(p.line)
        .child(el().key("mini-fill").absolute(0.0, 0.0).size(0.0, 3.0).radius(1.5).fill(p.accent).live_rect(&s.progress))
}

pub fn mini_tiles(s: &State, p: Palette, size: f32) -> El {
    let inner = MINI_SIZE.0 - 2.0 * MINI_PAD;
    let gap = (inner - size * RECENTS.len() as f32) / (RECENTS.len() as f32 - 1.0);
    let _ = p;
    row().key("mini-recents").gap(gap).children(RECENTS.iter().map(|c| cover(s, *c, size, 5.0)))
}

pub const MINI_TILE: f32 = 52.0;

fn nav(p: Palette, glyph: &'static rune::Icon, label: &str, selected: bool) -> El {
    row()
        .h(34.0)
        .px(10.0)
        .gap(12.0)
        .center_y()
        .radius(7.0)
        .fill(if selected { p.selected } else { rune::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 } })
        .child(icon(glyph, 18.0).color(if selected { p.text } else { p.muted }))
        .child(text(label.to_owned()).font(font::NAV).color(if selected { p.text } else { p.muted }))
}

pub fn sidebar(s: &State, p: Palette, home: bool) -> El {
    let playlists = [
        ("Deep Focus Without The Lyrics", Cover::Gradient(0)),
        ("Sunday Cooking", Cover::Gradient(2)),
        ("Late Night Drive", Cover::LowTide),
        ("Gym, Reluctantly", Cover::Gradient(1)),
        ("Songs My Sister Sent", Cover::ConcreteGarden),
        ("Discover Weekly", Cover::Gradient(0)),
        ("Rainy Afternoon Jazz-ish", Cover::CarrierWave),
    ];
    let mut list = col().gap(4.0).px(8.0);
    for (name, c) in playlists {
        list = list.child(
            row()
                .h(32.0)
                .px(8.0)
                .gap(10.0)
                .center_y()
                .child(cover(s, c, 24.0, 4.0))
                .child(text(name).font(font::NAV).color(p.muted).truncate()),
        );
    }
    col()
        .key("sidebar")
        .w(SIDEBAR_W)
        .h_full()
        .fill(p.sidebar)
        .border_sides([0.0, 1.0, 0.0, 0.0], p.line)
        .pt(14.0)
        .gap(4.0)
        .child(
            row()
                .h(30.0)
                .px(18.0)
                .gap(10.0)
                .center_y()
                .child(icon(&icons::MARK, 18.0).color(p.accent))
                .child(text("Baliset").font(font::BRAND).color(p.text)),
        )
        .child(
            col()
                .px(8.0)
                .pt(8.0)
                .gap(2.0)
                .child(nav(p, &icons::HOME, "Home", home))
                .child(nav(p, &icons::SEARCH, "Search", false))
                .child(nav(p, &icons::HEART, "Liked Songs", false)),
        )
        .child(
            row()
                .px(18.0)
                .pt(18.0)
                .pb(6.0)
                .child(text("PLAYLISTS").font(font::CAPS).color(p.faint))
                .child(spacer())
                .child(text("7").font(font::CAPS).color(p.faint)),
        )
        .child(list)
}

pub fn home_page(s: &State, p: Palette) -> El {
    let card = |c: Cover, title: &'static str, sub: &'static str| {
        col()
            .w(150.0)
            .gap(6.0)
            .child(cover(s, c, 150.0, 6.0))
            .child(text(title).font(font::CARD_TITLE).color(p.text).truncate())
            .child(text(sub).font(font::SMALL).color(p.muted).truncate())
    };
    let shelf = |title: &'static str, cards: Vec<El>| {
        col().gap(14.0).child(text(title).font(font::SECTION).color(p.text)).child(row().gap(16.0).children(cards))
    };
    col()
        .key("home")
        .grow(1.0)
        .min_h(0.0)
        .px(28.0)
        .pt(26.0)
        .gap(28.0)
        .child(shelf(
            "Recently played",
            vec![
                card(Cover::Gradient(0), "Deep Focus Without…", "kabilan"),
                card(Cover::ConcreteGarden, "Concrete Garden", "Okonkwo Sound System"),
                card(Cover::LowTide, "Low Tide Radio", "Marlowe & The Estuary"),
                card(Cover::Gradient(2), "Late Night Drive", "kabilan"),
                card(Cover::CarrierWave, "Carrier Wave", "Velvet Antenna"),
            ],
        ))
        .child(shelf(
            "Your albums",
            vec![
                card(Cover::Gradient(1), "Sunday Cooking", "kabilan"),
                card(Cover::LowTide, "Low Tide Radio", "Marlowe & The Estuary"),
                card(Cover::CarrierWave, "Carrier Wave", "Velvet Antenna"),
                card(Cover::ConcreteGarden, "Concrete Garden", "Okonkwo Sound System"),
                card(Cover::Gradient(0), "Discover Weekly", "Spotify"),
            ],
        ))
}

/// The now-playing bar. `now_extra` sits after the artist line (an indicator), `seek` is the
/// seek bar, and `under` is drawn behind everything (a bloom, a light edge).
pub fn bar(s: &State, p: Palette, now_extra: Option<El>, seek: El, under: Vec<El>) -> El {
    let info = s.track.info();
    let mut title = row().gap(8.0).center_y().child(text(info.title).font(font::ROW_TITLE).color(p.text).truncate());
    if let Some(extra) = now_extra {
        title = title.child(extra);
    }
    let now = row()
        .w(240.0)
        .gap(12.0)
        .center_y()
        .child(cover(s, s.track, 52.0, 6.0))
        .child(col().grow(1.0).min_w(0.0).gap(2.0).child(title).child(text(info.artist).font(font::ROW_SUB).color(p.muted).truncate()))
        .child(icon_button(&icons::HEART_FILLED, 28.0, p.accent));
    let transport = col()
        .grow(1.0)
        .gap(4.0)
        .center()
        .child(
            row()
                .gap(14.0)
                .center_y()
                .child(icon_button(&icons::SHUFFLE, 28.0, p.muted))
                .child(icon_button(&icons::PREVIOUS, 30.0, p.text))
                .child(play_button(p, s.playing(), 32.0))
                .child(icon_button(&icons::NEXT, 30.0, p.text))
                .child(icon_button(&icons::REPEAT, 28.0, p.muted)),
        )
        .child(
            row()
                .gap(10.0)
                .center_y()
                .child(text("0:47").live_text(&s.elapsed).font(font::TIME).color(p.faint))
                .child(seek)
                .child(text("4:21").font(font::TIME).color(p.faint)),
        );
    let extras = row()
        .w(200.0)
        .gap(10.0)
        .center_y()
        .child(spacer())
        .child(icon_button(&icons::MINI, 28.0, p.muted))
        .child(icon_button(&icons::VOLUME, 28.0, p.muted))
        .child(el().w(84.0).h(4.0).radius(2.0).fill(p.line).child(el().w(60.0).h(4.0).radius(2.0).fill(p.accent)));
    let mut out = row()
        .key("now-playing")
        .w_full()
        .h(BAR_H)
        .px(14.0)
        .gap(16.0)
        .center_y()
        .fill(p.bar)
        .border_sides([1.0, 0.0, 0.0, 0.0], p.line);
    for u in under {
        out = out.child(u);
    }
    out.child(now).child(transport).child(extras)
}

pub const SEEK_W: f32 = 420.0;

pub fn plain_seek(s: &State, p: Palette) -> El {
    el().w(SEEK_W).h(4.0).radius(2.0).fill(p.line).child(el().absolute(0.0, 0.0).size(0.0, 4.0).radius(2.0).fill(p.accent).live_rect(&s.progress))
}

pub fn main_window(s: &State, p: Palette, home: bool, page: El, bar: El) -> El {
    col()
        .key("main")
        .w_full()
        .h_full()
        .fill(p.bg)
        .child(row().grow(1.0).min_h(0.0).child(sidebar(s, p, home)).child(page))
        .child(bar)
}
