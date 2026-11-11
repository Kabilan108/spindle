//! The settings popover, opened from the account row at the foot of the sidebar: audio
//! quality, volume normalisation and the zoom with its keys.

use rune::{Align, Color, Cursor, El, Side, Tx, col, el, row, text};

use super::Main;
use super::theme::{Palette, font};
use super::zoom::{self, STEPS, ZoomKey};
use crate::settings::{self, AudioQuality};

const W: f32 = 296.0;

fn caption(label: &str, p: Palette) -> El {
    text(label.to_owned()).font(font::CAPS).color(p.faint)
}

fn quality_choice(quality: AudioQuality, chosen: bool, p: Palette) -> El {
    let ink = if chosen { p.on_accent } else { p.muted };
    col()
        .key(format!("quality-{}", quality.kbps()))
        .role("radio")
        .label(format!("{}, {} kbit/s", quality.label(), quality.kbps()))
        .checked(chosen)
        .grow(1.0)
        .min_w(0.0)
        .py(6.0)
        .gap(1.0)
        .items_center()
        .radius(7.0)
        .fill(if chosen { p.accent } else { Color::TRANSPARENT })
        .hover(if chosen { Color::TRANSPARENT } else { p.hover })
        .press(p.press)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .on_press(move |_: &mut Main| settings::update(|s| s.audio_quality = quality))
        .child(text(quality.label()).font(font::BUTTON).color(ink))
        .child(text(format!("{} kbit/s", quality.kbps())).font(font::SMALL).color(if chosen { p.on_accent } else { p.faint }))
}

fn switch(key: &str, label: &str, on: bool, p: Palette, flip: fn(&mut Main)) -> El {
    let knob = el().size(16.0, 16.0).radius(8.0).fill(if on { p.on_accent } else { p.muted }).absolute(if on { 18.0 } else { 2.0 }, 2.0);
    let track = el().key("track").size(36.0, 20.0).radius(10.0).fill(if on { p.accent } else { p.raised }).child(knob);
    row()
        .key(key.to_owned())
        .role("switch")
        .label(label.to_owned())
        .checked(on)
        .h(36.0)
        .px(8.0)
        .mx(-8.0)
        .gap(10.0)
        .center_y()
        .radius(7.0)
        .hover(p.hover)
        .press(p.press)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .on_press(flip)
        .child(text(label.to_owned()).font(font::NAV).color(p.text).grow(1.0).min_w(0.0))
        .child(track)
}

fn step_button(key: &'static str, label: &'static str, glyph: &'static str, enabled: bool, press: ZoomKey, p: Palette) -> El {
    let b = el()
        .key(key)
        .role("button")
        .label(label)
        .size(28.0, 28.0)
        .radius(14.0)
        .center()
        .fill(p.raised)
        .child(text(glyph).font(font::SECTION).color(if enabled { p.text } else { p.faint }));
    if enabled {
        b.cursor(Cursor::Pointer).focusable(true).hover(p.hover).press(p.press).on_press_tx(move |s: &mut Main, tx: &mut Tx| s.zoom.press(press, tx))
    } else {
        b.opacity(0.5)
    }
}

fn keycap(label: &str, p: Palette) -> El {
    el().h(20.0).px(6.0).radius(4.0).center().fill(p.raised).child(text(label.to_owned()).font(font::CAPS).color(p.muted))
}

fn zoom_row(s: &Main, p: Palette) -> El {
    let level = s.zoom.level;
    let reset = row()
        .key("zoom-reset")
        .role("button")
        .label("Reset zoom to 100%")
        .h(28.0)
        .px(10.0)
        .center()
        .radius(14.0)
        .hover(p.hover)
        .press(p.press)
        .cursor(Cursor::Pointer)
        .focusable(true)
        .on_press_tx(|s: &mut Main, tx: &mut Tx| s.zoom.press(ZoomKey::Reset, tx))
        .child(text("Reset").font(font::SMALL).color(p.muted));
    row()
        .key("zoom-controls")
        .gap(8.0)
        .center_y()
        .child(text("Zoom").font(font::NAV).color(p.text).grow(1.0))
        .child(step_button("zoom-out", "Zoom out", "−", level > STEPS[0], ZoomKey::Out, p))
        .child(text(format!("{}%", zoom::percent(level))).key("zoom-percent").font(font::NUMBER).color(p.text).w(44.0).align(Align::Center))
        .child(step_button("zoom-in", "Zoom in", "+", level < STEPS[STEPS.len() - 1], ZoomKey::In, p))
        .child(reset)
}

fn shortcuts(p: Palette) -> El {
    let pair = |cap: &str, what: &str| row().gap(5.0).center_y().child(keycap(cap, p)).child(text(what.to_owned()).font(font::SMALL).color(p.faint));
    row().key("zoom-keys").gap(12.0).center_y().child(pair("Ctrl =", "in")).child(pair("Ctrl −", "out")).child(pair("Ctrl 0", "reset"))
}

/// The popover, anchored above the settings button. Attach it at the shell's root.
pub fn view(s: &Main, p: Palette) -> El {
    let now = settings::get();
    let launch = settings::at_launch();
    let mut audio = col()
        .key("audio-settings")
        .gap(10.0)
        .child(caption("AUDIO QUALITY", p))
        .child(row().key("quality").gap(4.0).padding(3.0).radius(10.0).fill(p.field).children(AudioQuality::ALL.map(|q| quality_choice(q, q == now.audio_quality, p))))
        .child(switch("normalize", "Even out loudness", now.normalize_volume, p, |_| settings::update(|s| s.normalize_volume = !s.normalize_volume)));
    if (now.audio_quality, now.normalize_volume) != (launch.audio_quality, launch.normalize_volume) {
        audio = audio.child(text("Applies the next time Baliset starts.").key("audio-restart").font(font::SMALL).color(p.accent).wrap());
    }
    col()
        .key("settings-panel")
        .role("dialog")
        .label("Settings")
        .w(W)
        .padding(16.0)
        .gap(16.0)
        .radius(12.0)
        .fill(p.raised)
        .border(1.0, p.line)
        .shadow(8.0, 32.0, p.shadow)
        .anchored("settings", Side::Top, Align::Start, 8.0)
        .modal(true, |s: &mut Main| s.settings_open = false)
        .child(audio)
        .child(el().h(1.0).w_full().fill(p.line))
        .child(col().key("zoom-settings").gap(10.0).child(zoom_row(s, p)).child(shortcuts(p)))
}
