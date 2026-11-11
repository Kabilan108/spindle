//! The tray icon (StatusNotifierItem on Linux). A primary click shows or hides the main window;
//! the menu drives playback. Its tooltip follows the playing track.

use std::sync::{Arc, Mutex, PoisonError};

use rune::{AppHandle, Tray, TrayEvent, TrayIcon, TrayItem, TraySpec};

use crate::model::{Command, Store};
use crate::ui::{Link, Main};

const SIZE: u32 = 64;

/// Starts the tray on its own thread, so a slow session bus never holds up a frame. Without a
/// tray (no bus, no host) the app carries on.
pub fn start(link: Link, main: AppHandle<Main>) {
    let spawned = std::thread::Builder::new().name("baliset-tray".into()).spawn(move || {
        let (tip, playing) = link.read(|store| (tooltip(store), store.playback.playing));
        let spec = TraySpec::new("baliset", "Baliset", icon()).tooltip(tip.clone()).menu(menu(playing));
        let events = link.clone();
        let tray = match Tray::new(spec, move |event| on_event(&events, &main, event)) {
            Ok(tray) => Arc::new(tray),
            Err(error) => {
                eprintln!("baliset: no tray icon: {error}");
                return;
            }
        };
        // The engine keeps this listener, and with it the tray, for the life of the process.
        let shown = Mutex::new((tip, playing));
        let reader = link.clone();
        link.subscribe(move || {
            let now = reader.read(|store| (tooltip(store), store.playback.playing));
            let mut shown = shown.lock().unwrap_or_else(PoisonError::into_inner);
            if shown.0 != now.0 {
                tray.set_tooltip(now.0.clone());
            }
            if shown.1 != now.1 {
                tray.set_menu(menu(now.1));
            }
            *shown = now;
        });
    });
    if let Err(error) = spawned {
        eprintln!("baliset: no tray icon: {error}");
    }
}

fn tooltip(store: &Store) -> String {
    match &store.playback.track {
        Some(track) if track.artists.is_empty() => track.name.clone(),
        Some(track) => format!("{} — {}", track.name, track.artist_line()),
        None => "Baliset".into(),
    }
}

fn menu(playing: bool) -> Vec<TrayItem> {
    vec![
        TrayItem::action("open", "Open Baliset"),
        TrayItem::Separator,
        TrayItem::action("play-pause", if playing { "Pause" } else { "Play" }),
        TrayItem::action("next", "Next"),
        TrayItem::action("previous", "Previous"),
        TrayItem::Separator,
        TrayItem::action("quit", "Quit"),
    ]
}

fn on_event(link: &Link, main: &AppHandle<Main>, event: TrayEvent) {
    match event {
        TrayEvent::Activate { token } => {
            if main.is_visible() {
                main.hide();
            } else {
                match token {
                    Some(token) => main.activate(token),
                    None => main.open(),
                }
                main.update(|m, tx| m.resume(tx));
            }
        }
        TrayEvent::Menu(id) => match id.as_str() {
            "open" => {
                main.open();
                main.update(|m, tx| m.resume(tx));
            }
            "play-pause" => link.send(Command::PlayPause),
            "next" => link.send(Command::Next),
            "previous" => link.send(Command::Previous),
            "quit" => main.quit(),
            _ => {}
        },
        _ => {}
    }
}

/// Baliset's mark, a zither frame with its strings, in the accent's brass, drawn at 4×4
/// samples a pixel.
pub fn icon() -> TrayIcon {
    TrayIcon::rgba(SIZE, SIZE, mark_pixels(SIZE))
}

fn mark_pixels(size: u32) -> Vec<u8> {
    // Round-capped strokes on a 256-unit grid, `[x0, y0, x1, y1, width]`: the frame's post,
    // base and sloping head, then four strings.
    const STROKES: [[f32; 5]; 7] = [
        [45.0, 42.0, 45.0, 213.0, 20.0],
        [45.0, 213.0, 215.0, 213.0, 20.0],
        [45.0, 42.0, 211.0, 132.0, 20.0],
        [76.0, 68.0, 76.0, 181.0, 15.0],
        [108.0, 85.0, 108.0, 181.0, 15.0],
        [140.0, 102.0, 140.0, 181.0, 15.0],
        [172.0, 119.0, 172.0, 181.0, 15.0],
    ];
    const INK: [u8; 3] = [232, 180, 90];
    let scale = 256.0 / size as f32;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let x = (px as f32 + (sx as f32 + 0.5) / 4.0) * scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / 4.0) * scale;
                    if STROKES.iter().any(|&[x0, y0, x1, y1, width]| on_stroke(x, y, x0, y0, x1, y1, width)) {
                        hits += 1;
                    }
                }
            }
            out.extend_from_slice(&INK);
            out.push((hits * 255 / 16) as u8);
        }
    }
    out
}

fn on_stroke(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, width: f32) -> bool {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let along = (((x - x0) * dx + (y - y0) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    let (cx, cy) = (x0 + along * dx, y0 + along * dy);
    (x - cx).powi(2) + (y - cy).powi(2) <= (width / 2.0).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_has_pixels_for_its_size_and_some_ink() {
        let pixels = mark_pixels(SIZE);
        assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
        let inked = pixels.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
        assert!(inked > 400 && inked < (SIZE * SIZE) as usize / 2, "{inked} inked pixels");
    }

    #[test]
    fn the_menu_names_the_next_play_action() {
        let label = |items: Vec<TrayItem>| match &items[2] {
            TrayItem::Action { label, .. } => label.clone(),
            _ => String::new(),
        };
        assert_eq!(label(menu(true)), "Pause");
        assert_eq!(label(menu(false)), "Play");
    }
}
