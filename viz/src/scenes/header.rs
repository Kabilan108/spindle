//! 06 Ambient header: the album page's header sits in slow pools of the cover's colours that
//! drift with the music and breathe with the bass. Paused, they stop where they are and keep the
//! page tinted. Live offsets, scales and opacities; the view never re-runs.

use rune::{Color, El, Spring, col, el, effect, icon, row, text};

use crate::art::Cover;
use crate::chrome::{bar, cover, main_window, plain_seek};
use crate::gfx::GRAIN;
use crate::icons;
use crate::signal::HOP;
use crate::theme::{Palette, font};
use crate::{State, publish};

const BLOB: f32 = 620.0;
const HEADER_H: f32 = 330.0;
const BLOBS: [(f32, f32); 4] = [(-60.0, -260.0), (300.0, -300.0), (560.0, -180.0), (120.0, -120.0)];

fn tracks(p: Palette) -> El {
    let rows = [
        ("Rebar Bloom", "3:44", true),
        ("Third Mainland", "4:37", false),
        ("Danfo Dub", "3:10", true),
        ("Generator Hum", "4:03", false),
        ("Harmattan", "2:36", true),
    ];
    let mut list = col().px(36.0).gap(0.0);
    list = list.child(
        row()
            .h(30.0)
            .px(12.0)
            .center_y()
            .child(text("#").font(font::CAPS).color(p.faint).w(36.0))
            .child(text("TITLE").font(font::CAPS).color(p.faint).grow(1.0))
            .child(text("TIME").font(font::CAPS).color(p.faint)),
    );
    for (i, (name, time, liked)) in rows.iter().enumerate() {
        let playing = i == 0;
        list = list.child(
            row()
                .h(48.0)
                .px(12.0)
                .radius(6.0)
                .center_y()
                .fill(if playing { p.selected } else { Color::TRANSPARENT })
                .child(text(format!("{}", i + 1)).font(font::NUMBER).color(if playing { p.accent } else { p.faint }).w(36.0))
                .child(
                    col()
                        .grow(1.0)
                        .gap(2.0)
                        .child(text(*name).font(font::ROW_TITLE).color(if playing { p.accent } else { p.text }))
                        .child(text("Okonkwo Sound System").font(font::ROW_SUB).color(p.muted)),
                )
                .child(if *liked { icon(&icons::HEART_FILLED, 16.0).color(p.accent) } else { el().size(16.0, 16.0) })
                .child(text(*time).font(font::NUMBER).color(p.muted).w(60.0)),
        );
    }
    list
}

pub fn view(s: &State, p: Palette) -> El {
    let pal = s.palette();
    let colours = [pal.glow, pal.glow2, pal.swatches[0], pal.vivid];
    let mut ambient = el().key("ambient").pin(Some(0.0), Some(0.0), Some(0.0), None).h(HEADER_H + 140.0);
    for (i, (x, y)) in BLOBS.iter().enumerate() {
        let c = colours[i];
        let [l, a, b, _] = c.to_oklab();
        let tone = Color::from_oklab([l.min(0.62), a * 0.9, b * 0.9, 1.0]);
        ambient = ambient.child(
            el().key(format!("blob{i}"))
                .absolute(*x, *y)
                .size(BLOB, BLOB)
                .radial_gradient(
                    [0.5, 0.5],
                    BLOB / 2.0,
                    [(0.0, tone.alpha(0.42)), (0.3, tone.alpha(0.30)), (0.55, tone.alpha(0.13)), (0.8, tone.alpha(0.035)), (1.0, tone.alpha(0.0))],
                )
                .live_offset(s.sc.offsets[i].smooth(Spring::new(0.35, 0.0)))
                .live_scale(s.sc.levels[i].smooth(Spring::new(0.3, 0.0)))
                .live_opacity(s.sc.levels[4 + i].smooth(Spring::new(0.4, 0.0))),
        );
    }
    ambient = ambient
        .child(
            el().key("fade")
                .pin(Some(0.0), None, Some(0.0), Some(0.0))
                .h(220.0)
                .gradient(p.bg.alpha(0.0), p.bg),
        )
        .child(effect(GRAIN).key("grain").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).effect_data([0.02, 0.0, 0.0, 0.0]));
    let info = s.track.info();
    let header = row()
        .key("page-header")
        .px(44.0)
        .pt(28.0)
        .pb(32.0)
        .gap(24.0)
        .items_end()
        .child(cover(s, Cover::ConcreteGarden, 160.0, 8.0).shadow(6.0, 24.0, p.shadow))
        .child(
            col()
                .grow(1.0)
                .gap(6.0)
                .child(text("ALBUM").font(font::CAPS).color(p.text.alpha(0.75)))
                .child(text(info.album).font(font::PAGE_TITLE).color(p.text))
                .child(text("Okonkwo Sound System  ·  8 songs  ·  28 min").font(font::SMALL).color(p.text.alpha(0.78)))
                .child(
                    row().mt(10.0).child(
                        row()
                            .h(36.0)
                            .px(18.0)
                            .gap(8.0)
                            .radius(18.0)
                            .center_y()
                            .fill(p.accent)
                            .child(icon(if s.playing() { &icons::PAUSE } else { &icons::PLAY }, 14.0).color(p.on_accent))
                            .child(text(if s.playing() { "Pause" } else { "Play" }).font(font::BUTTON).color(p.on_accent)),
                    ),
                ),
        );
    let page = col().key("album").grow(1.0).min_w(0.0).h_full().clip_rounded(0.0).child(ambient).child(header).child(tracks(p));
    main_window(s, p, false, page, bar(s, p, None, plain_seek(s, p), vec![]))
}

pub fn poll(s: &mut State, hops: usize) -> bool {
    let f = s.frame;
    let playing = s.playing();
    if playing && hops > 0 {
        s.sc.drift += hops as f64 * HOP * (0.25 + 1.1 * f.loudness as f64);
    }
    let d = s.sc.drift as f32;
    let phases = [(0.31, 0.23, 0.0), (0.27, 0.35, 1.7), (0.37, 0.29, 3.1), (0.21, 0.33, 4.4)];
    let energies = [f.bass, f.mid, f.treble, f.loudness];
    for i in 0..4 {
        let (fx, fy, ph) = phases[i];
        let off = [70.0 * (d * fx + ph).sin(), 46.0 * (d * fy + ph * 1.3).cos()];
        publish(&s.sc.offsets[i], [super::q(off[0]), super::q(off[1])], s.t);
        let scale = 1.0 + 0.12 * energies[i];
        publish(&s.sc.levels[i], (scale * 100.0).round() / 100.0, s.t);
        let o = 0.7 + 0.3 * f.loudness;
        publish(&s.sc.levels[4 + i], (o * 50.0).round() / 50.0, s.t);
    }
    false
}
