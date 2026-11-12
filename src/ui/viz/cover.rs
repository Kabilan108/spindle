//! What the visualizers take from a cover: a few colours and a blurred backdrop.
//!
//! The engine's image cache holds the encoded cover. It's decoded once more here, shrunk, and
//! clustered (k-means in OKLab over a 24×24 thumbnail), about a millisecond on a worker thread,
//! once per cover. Every colour is clamped in OKLCH so it stays readable on Baliset's dark
//! surfaces whatever the art is.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use rune::{AppHandle, Color, Pixels, oklch};

use super::super::Main;
use super::super::link::Link;

const PALETTE_PX: u32 = 24;
pub const BACKDROP_PX: u32 = 40;
const KEEP: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArtPalette {
    /// Swatches by population, most first.
    pub swatches: [Color; 5],
    /// Light enough to read as light on dark: glows, bars, rings.
    pub glow: Color,
    pub glow2: Color,
}

impl ArtPalette {
    /// Before a cover is in: Baliset's own brass.
    pub fn plain() -> Self {
        let brass = oklch(0.62, 0.11, 70.0);
        Self { swatches: [brass, oklch(0.5, 0.06, 40.0), brass, oklch(0.45, 0.05, 90.0), brass], glow: oklch(0.8, 0.115, 75.0), glow2: oklch(0.74, 0.09, 45.0) }
    }

    /// Colours for large pools behind the page's text: the swatches, kept deep on a dark page
    /// and pale on a light one.
    pub fn pools(&self, dark: bool) -> [Color; 4] {
        [self.glow, self.glow2, self.swatches[0], self.swatches[1]].map(|c| {
            let (l, ch, h) = lch(c);
            let l = if dark { l.min(0.62) } else { l.max(0.82) };
            oklch(l, (ch * 0.9).max(0.03), h)
        })
    }
}

fn srgb_decode(v: u8) -> f32 {
    let s = f32::from(v) / 255.0;
    if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

fn srgb_encode(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

fn lch(c: Color) -> (f32, f32, f32) {
    let [l, a, b, _] = c.to_oklab();
    (l, (a * a + b * b).sqrt(), b.atan2(a).to_degrees().rem_euclid(360.0))
}

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let w = [2.0, 1.0, 1.0];
    (0..3).map(|i| w[i] * (a[i] - b[i]).powi(2)).sum()
}

fn palette(thumb: &image::RgbaImage) -> ArtPalette {
    let samples: Vec<[f32; 3]> = thumb
        .pixels()
        .map(|p| {
            let c = Color { r: srgb_decode(p[0]), g: srgb_decode(p[1]), b: srgb_decode(p[2]), a: 1.0 };
            let [l, a, b, _] = c.to_oklab();
            [l, a, b]
        })
        .collect();
    let k = 5;
    let mut centres = vec![samples[0]];
    while centres.len() < k {
        let nearest = |s: &[f32; 3]| centres.iter().map(|c| dist(s, c)).fold(f32::MAX, f32::min);
        let far = samples.iter().max_by(|p, q| nearest(p).total_cmp(&nearest(q))).copied().unwrap_or(samples[0]);
        centres.push(far);
    }
    let mut counts = vec![0usize; k];
    for _ in 0..12 {
        let mut sums = vec![[0.0f32; 3]; k];
        counts = vec![0; k];
        for s in &samples {
            let j = (0..k).min_by(|a, b| dist(s, &centres[*a]).total_cmp(&dist(s, &centres[*b]))).unwrap_or(0);
            for c in 0..3 {
                sums[j][c] += s[c];
            }
            counts[j] += 1;
        }
        for j in 0..k {
            if counts[j] > 0 {
                centres[j] = sums[j].map(|v| v / counts[j] as f32);
            }
        }
    }
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|a, b| counts[*b].cmp(&counts[*a]));
    let total = samples.len() as f32;
    let swatch = |j: usize| Color::from_oklab([centres[j][0], centres[j][1], centres[j][2], 1.0]);
    let score = |j: usize| lch(swatch(j)).1 * (counts[j] as f32 / total).sqrt();
    let vivid_j = (0..k).max_by(|a, b| score(*a).total_cmp(&score(*b))).unwrap_or(0);
    let vivid = swatch(vivid_j);
    let (_, _, vh) = lch(vivid);
    let second = (0..k)
        .filter(|j| *j != vivid_j && counts[*j] as f32 / total > 0.04)
        .filter(|j| {
            let (_, c, h) = lch(swatch(*j));
            let dh = (h - vh).abs();
            c > 0.04 && dh.min(360.0 - dh) > 50.0
        })
        .max_by(|a, b| score(*a).total_cmp(&score(*b)))
        .map(swatch)
        .unwrap_or_else(|| {
            let (l, c, h) = lch(vivid);
            oklch(l, c, (h + 40.0) % 360.0)
        });
    let tone = |c: Color, l: f32, cmax: f32| {
        let (_, ch, h) = lch(c);
        oklch(l, ch.clamp(0.06, cmax), h)
    };
    ArtPalette { swatches: std::array::from_fn(|i| swatch(order[i])), glow: tone(vivid, 0.78, 0.17), glow2: tone(second, 0.74, 0.15) }
}

/// A heavily blurred cover, small: the GPU's bilinear filter stretches it over the window, so
/// a full-window backdrop costs no blur passes.
fn backdrop(thumb: &image::RgbaImage) -> Vec<u8> {
    let n = BACKDROP_PX as i32;
    let mut lin: Vec<[f32; 3]> = thumb.pixels().map(|p| [srgb_decode(p[0]), srgb_decode(p[1]), srgb_decode(p[2])]).collect();
    for _ in 0..3 {
        let src = lin.clone();
        for y in 0..n {
            for x in 0..n {
                let mut acc = [0.0f32; 3];
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let s = src[((y + dy).clamp(0, n - 1) * n + (x + dx).clamp(0, n - 1)) as usize];
                        for c in 0..3 {
                            acc[c] += s[c];
                        }
                    }
                }
                lin[(y * n + x) as usize] = acc.map(|v| v / 25.0);
            }
        }
    }
    lin.into_iter().flat_map(|p| [srgb_encode(p[0]), srgb_encode(p[1]), srgb_encode(p[2]), 255]).collect()
}

/// Decodes a cover and works out its look. Runs off the UI thread.
pub fn analyse(bytes: &[u8]) -> Option<(ArtPalette, Vec<u8>)> {
    let cover = image::load_from_memory(bytes).ok()?.to_rgba8();
    if cover.width() == 0 || cover.height() == 0 {
        return None;
    }
    let small = image::imageops::thumbnail(&cover, PALETTE_PX, PALETTE_PX);
    let blur = image::imageops::thumbnail(&cover, BACKDROP_PX, BACKDROP_PX);
    Some((palette(&small), backdrop(&blur)))
}

#[derive(Clone)]
pub struct Look {
    pub palette: ArtPalette,
    pub backdrop: Pixels,
}

enum Slot {
    Pending,
    Ready(Look),
    Failed,
}

/// Looks by cover URL, the most recent few.
#[derive(Default)]
pub struct Covers {
    slots: RefCell<HashMap<String, Slot>>,
    order: RefCell<VecDeque<String>>,
}

impl Covers {
    /// The look of the cover at `url`, once its bytes are in and it has been analysed. The
    /// first call starts the work; `me` hears when it's done. Without a window (tests,
    /// headless snapshots) it's done on the spot, so the first frame shows it.
    pub fn look(&self, link: &Link, url: &str, me: Option<&AppHandle<Main>>) -> Option<Look> {
        match self.slots.borrow().get(url) {
            Some(Slot::Ready(look)) => return Some(look.clone()),
            Some(Slot::Pending | Slot::Failed) => return None,
            None => {}
        }
        let bytes = link.image(url)?;
        match me.filter(|_| rune::tooling::headless_run().is_none()) {
            Some(me) => {
                self.insert(url, Slot::Pending);
                let (me, url) = (me.clone(), url.to_owned());
                std::thread::spawn(move || {
                    let analysed = analyse(&bytes);
                    me.update(move |s: &mut Main, _| s.viz.covers.arrived(&url, analysed));
                });
                None
            }
            None => {
                self.arrived(url, analyse(&bytes));
                self.look(link, url, None)
            }
        }
    }

    fn arrived(&self, url: &str, analysed: Option<(ArtPalette, Vec<u8>)>) {
        let slot = match analysed {
            Some((palette, rgba)) => {
                let backdrop = Pixels::new();
                backdrop.set(BACKDROP_PX, BACKDROP_PX, rgba);
                Slot::Ready(Look { palette, backdrop })
            }
            None => Slot::Failed,
        };
        self.insert(url, slot);
    }

    fn insert(&self, url: &str, slot: Slot) {
        let mut slots = self.slots.borrow_mut();
        let mut order = self.order.borrow_mut();
        if slots.insert(url.to_owned(), slot).is_none() {
            order.push_back(url.to_owned());
        }
        while order.len() > KEEP {
            if let Some(oldest) = order.pop_front() {
                slots.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(colour: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut img = image::RgbaImage::new(64, 64);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba(colour(x, y));
        }
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_two_colour_cover_gives_both_hues() {
        let bytes = png(|x, _| if x < 32 { [200, 40, 40, 255] } else { [40, 60, 200, 255] });
        let (palette, backdrop) = analyse(&bytes).expect("a PNG decodes");
        let hues = [lch(palette.glow).2, lch(palette.glow2).2];
        let reddish = hues.iter().any(|h| *h < 60.0 || *h > 330.0);
        let bluish = hues.iter().any(|h| (230.0..300.0).contains(h));
        assert!(reddish && bluish, "glow hues {hues:?}");
        assert_eq!(backdrop.len(), (BACKDROP_PX * BACKDROP_PX * 4) as usize);
    }

    #[test]
    fn glows_stay_light_on_a_dark_cover() {
        let bytes = png(|_, _| [12, 10, 14, 255]);
        let (palette, _) = analyse(&bytes).unwrap();
        assert!(lch(palette.glow).0 > 0.7);
    }

    #[test]
    fn garbage_is_not_a_cover() {
        assert!(analyse(b"not an image").is_none());
    }
}
