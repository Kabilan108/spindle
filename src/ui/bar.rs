//! The now-playing bar: the track, transport, a seek bar and volume.

use rune::{Align, Color, Cursor, Drag, El, PointerPress, Tx, col, el, row, text};

use super::meter::{Bar, fmt_time};
use super::theme::{BAR_H, Palette, SPRING, font};
use super::widgets::{art, icon_button};
use super::menu::{At, playing_target};
use super::{Main, icons};
use crate::model::{Command, Repeat, Track};

const SIDE_W: f32 = 240.0;

struct Facts {
    track: Option<Track>,
    playing: bool,
    shuffle: bool,
    repeat: Repeat,
    volume: f32,
    liked: bool,
}

pub fn view(s: &Main, p: Palette) -> El {
    let facts = s.link.read(|store| {
        let playback = &store.playback;
        Facts {
            liked: playback.track.as_ref().is_some_and(|t| store.liked.contains(&t.uri)),
            track: playback.track.clone(),
            playing: playback.playing,
            shuffle: playback.shuffle,
            repeat: playback.repeat,
            volume: playback.volume,
        }
    });
    row()
        .key("now-playing")
        .w_full()
        .h(BAR_H)
        .px(14.0)
        .gap(16.0)
        .center_y()
        .fill(p.bar)
        .border_sides([1.0, 0.0, 0.0, 0.0], p.line)
        .child(now(s, p, &facts))
        .child(transport(s, p, &facts))
        .child(extras(s, p, &facts))
}

fn now(s: &Main, p: Palette, f: &Facts) -> El {
    let Some(track) = &f.track else {
        return row()
            .key("now")
            .w(SIDE_W)
            .gap(12.0)
            .center_y()
            .child(art(&s.art, &s.link, None, 52.0, 6.0, p, &icons::NOTE))
            .child(text("Nothing playing").font(font::ROW_SUB).color(p.faint));
    };
    let album = track.album.as_ref().map(|a| a.uri.clone()).filter(|uri| !uri.is_empty());
    let episode = track.uri.starts_with("spotify:episode:");
    let mut cover = art(&s.art, &s.link, track.image_url.as_deref(), 52.0, 6.0, p, if episode { &icons::PODCAST } else { &icons::NOTE }).key("now-art");
    if let Some(album) = album.clone() {
        cover = cover.cursor(Cursor::Pointer).role("link").label(if episode { "Open show" } else { "Open album" }).on_press(move |s: &mut Main| s.open(&album));
    }
    let mut title = text(track.name.clone()).key("now-title").font(font::ROW_TITLE).color(p.text).truncate();
    if let Some(album) = album {
        title = title.cursor(Cursor::Pointer).on_press(move |s: &mut Main| s.open(&album));
    }
    let uri = track.uri.clone();
    let liked = f.liked;
    let heart = if liked { &icons::HEART_FILLED } else { &icons::HEART };
    let now = row()
        .key("now")
        .w(SIDE_W)
        .gap(12.0)
        .center_y()
        .on_secondary_press(|s: &mut Main, press: PointerPress, _: &mut Tx| {
            let target = s.link.read(playing_target);
            s.open_menu(target, At::from(press));
        })
        .child(cover)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(title)
                .child(text(track.artist_line()).key("now-artist").font(font::ROW_SUB).color(p.muted).truncate()),
        );
    // Liked Songs holds songs only.
    if episode {
        return now;
    }
    now.child(
        icon_button("now-like", if liked { "Remove from Liked Songs" } else { "Save to Liked Songs" }, heart, 30.0, if liked { p.accent } else { p.muted }, p)
            .on_press(move |s: &mut Main| s.set_liked(&uri, !liked)),
    )
}

/// A toggle's icon, in the accent with a dot under it while on.
pub(super) fn toggle(key: &str, label: &str, glyph: &'static rune::Icon, on: bool, p: Palette) -> El {
    let ink = if on { p.accent } else { p.muted };
    col()
        .key(key)
        .role("switch")
        .label(label)
        .checked(on)
        .size(32.0, 32.0)
        .radius(16.0)
        .center()
        .cursor(Cursor::Pointer)
        .focusable(true)
        .hover(p.hover)
        .press(p.press)
        .child(rune::icon(glyph, 17.0).color(ink))
        .child(el().size(4.0, 4.0).radius(2.0).fill(p.accent).absolute(14.0, 27.0).motion(SPRING).opacity(if on { 1.0 } else { 0.0 }))
}

fn transport(s: &Main, p: Palette, f: &Facts) -> El {
    let has_track = f.track.is_some();
    let (play_label, play_glyph) = if f.playing { ("Pause", &icons::PAUSE) } else { ("Play", &icons::PLAY) };
    let repeat_glyph = if f.repeat == Repeat::Track { &icons::REPEAT_ONE } else { &icons::REPEAT };
    let repeat_label = match f.repeat {
        Repeat::Off => "Repeat: off",
        Repeat::Context => "Repeat: all",
        Repeat::Track => "Repeat: one",
    };
    let shuffle = f.shuffle;
    let play = el()
        .key("play-pause")
        .role("button")
        .label(play_label)
        .size(36.0, 36.0)
        .radius(18.0)
        .center()
        .fill(p.text)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .hover(p.bg.alpha(0.12))
        .press(p.bg.alpha(0.2))
        .motion(SPRING)
        .on_press(|s: &mut Main| s.link.send(Command::PlayPause))
        .child(rune::icon(play_glyph, 18.0).color(p.bg));
    let buttons = row()
        .key("transport")
        .gap(10.0)
        .center_y()
        .child(toggle("shuffle", "Shuffle", &icons::SHUFFLE, shuffle, p).on_press(move |s: &mut Main| s.link.send(Command::SetShuffle(!shuffle))))
        .child(icon_button("previous", "Previous", &icons::PREVIOUS, 32.0, p.text, p).on_press(|s: &mut Main| s.link.send(Command::Previous)))
        .child(play)
        .child(icon_button("next", "Next", &icons::NEXT, 32.0, p.text, p).on_press(|s: &mut Main| s.link.send(Command::Next)))
        .child(toggle("repeat", repeat_label, repeat_glyph, f.repeat != Repeat::Off, p).on_press(|s: &mut Main| s.cycle_repeat()));
    let total = f.track.as_ref().map_or(0, |t| t.duration_ms);
    let seek = row()
        .key("seek-row")
        .w_full()
        .gap(10.0)
        .center_y()
        .child(text("0:00").live_text(&s.progress.elapsed).font(font::TIME).color(p.muted).w(42.0).align(Align::End))
        .child(
            slider("seek", "Position", &s.progress.bar, p, 4.0, 12.0)
                .grow(1.0)
                .on_drag(|s: &mut Main, d: &mut Drag, tx: &mut Tx| {
                    s.progress.drag(&s.link, d, tx);
                    tx.live_only();
                })
                .on_layout_tx(|s: &mut Main, r: [f32; 4], tx: &mut Tx| {
                    s.progress.bar.resize(r[2]);
                    tx.live_only();
                }),
        )
        .child(text(fmt_time(total)).key("total").font(font::TIME).color(p.muted).w(42.0));
    let mut center = col().key("center").grow(1.0).min_w(0.0).max_w(640.0).gap(4.0).items_center().child(buttons).child(seek);
    if !has_track {
        center = center.opacity(0.4).inert();
    }
    row().grow(1.0).min_w(0.0).justify_center().child(center)
}

/// A thin bar with a fill and a knob, both moved by live values. The knob shows on hover.
pub(super) fn slider(key: &str, label: &str, bar: &Bar, p: Palette, thickness: f32, knob: f32) -> El {
    let hit = 16.0;
    el().key(key)
        .role("slider")
        .label(label)
        .range_value(f64::from(bar.fraction()), 0.0, 1.0)
        .h(hit)
        .hover_group()
        .cursor(Cursor::Pointer)
        .child(
            el().key("track")
                .w_full()
                .h(thickness)
                .absolute(0.0, (hit - thickness) / 2.0)
                .radius(thickness / 2.0)
                .fill(p.line)
                .child(el().key("fill").absolute(0.0, 0.0).size(0.0, thickness).radius(thickness / 2.0).fill(p.accent).live_rect(&bar.fill)),
        )
        .child(
            el().key("knob")
                .size(knob, knob)
                .absolute(0.0, (hit - knob) / 2.0)
                .radius(knob / 2.0)
                .fill(Color::WHITE)
                .shadow(1.0, 4.0, p.shadow)
                .group_opacity(0.0, 1.0)
                .live_offset(&bar.knob),
        )
}

fn extras(s: &Main, p: Palette, f: &Facts) -> El {
    let volume_glyph = if f.volume <= 0.0 { &icons::VOLUME_OFF } else { &icons::VOLUME };
    row()
        .key("extras")
        .w(SIDE_W)
        .gap(6.0)
        .center_y()
        .justify_end()
        .child(toggle("queue-toggle", "Up next", &icons::QUEUE, s.queue_open, p).on_press(|s: &mut Main| s.queue_open = !s.queue_open))
        .child(
            icon_button("stage-open", "Open the full player (Ctrl+E)", &super::viz::EXPAND, 32.0, p.muted, p)
                .on_press(|s: &mut Main| s.viz.open_stage()),
        )
        .child(
            icon_button("mute", if f.volume <= 0.0 { "Unmute" } else { "Mute" }, volume_glyph, 32.0, p.muted, p)
                .on_press(|s: &mut Main| s.volume.toggle_mute(&s.link)),
        )
        .child(
            slider("volume", "Volume", &s.volume.bar, p, 4.0, 12.0)
                .w(92.0)
                .on_drag(|s: &mut Main, d: &mut Drag, tx: &mut Tx| {
                    s.volume.drag(&s.link, d, tx);
                    tx.live_only();
                })
                .on_layout_tx(|s: &mut Main, r: [f32; 4], tx: &mut Tx| {
                    s.volume.bar.resize(r[2]);
                    tx.live_only();
                }),
        )
}
