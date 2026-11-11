//! Generated album covers, and the palette a concept derives from a cover.
//!
//! The real app already has the decoded cover; `ArtPalette::from_rgba` is what it would run on
//! the image worker once per track (k-means in OKLab over a 24×24 thumbnail, under a millisecond).

use rune::{Color, oklch, oklcha};

pub const COVER_PX: u32 = 640;

fn srgb_encode(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

fn srgb_decode(v: u8) -> f32 {
    let s = v as f32 / 255.0;
    if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

fn hash(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x27d4_eb2d) ^ y.wrapping_mul(0x1656_67b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    (h & 0xffff) as f32 / 65_535.0
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lin(c: Color) -> [f32; 3] {
    [c.r, c.g, c.b]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cover {
    /// Velvet Antenna, "Carrier Wave": magenta to green with radio rings.
    CarrierWave,
    /// Marlowe & The Estuary, "Low Tide Radio": a low sun over banded water.
    LowTide,
    /// Okonkwo Sound System, "Concrete Garden": warm concrete blocks with a green garden.
    ConcreteGarden,
    /// Plain demo gradients for recents tiles.
    Gradient(u8),
}

pub struct TrackInfo {
    pub title: &'static str,
    pub artist: &'static str,
    pub album: &'static str,
}

impl Cover {
    pub fn info(self) -> TrackInfo {
        match self {
            Cover::CarrierWave => TrackInfo { title: "Carrier Wave", artist: "Velvet Antenna", album: "Carrier Wave" },
            Cover::LowTide => TrackInfo { title: "Slack Water", artist: "Marlowe & The Estuary", album: "Low Tide Radio" },
            Cover::ConcreteGarden => TrackInfo { title: "Rebar Bloom", artist: "Okonkwo Sound System", album: "Concrete Garden" },
            Cover::Gradient(_) => TrackInfo { title: "", artist: "", album: "" },
        }
    }

    /// Straight-alpha sRGB RGBA, `COVER_PX` square.
    pub fn render(self) -> Vec<u8> {
        let n = COVER_PX;
        let mut out = Vec::with_capacity((n * n * 4) as usize);
        for y in 0..n {
            for x in 0..n {
                let (u, v) = ((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32);
                let c = self.shade(u, v);
                let g = (hash(x, y) - 0.5) * 0.018;
                for ch in c {
                    out.push(srgb_encode(ch * (1.0 + g * 2.0) + g * 0.02));
                }
                out.push(255);
            }
        }
        out
    }

    fn shade(self, u: f32, v: f32) -> [f32; 3] {
        match self {
            Cover::CarrierWave => {
                let a = lin(oklch(0.48, 0.19, 330.0));
                let b = lin(oklch(0.62, 0.17, 145.0));
                let mid = lin(oklch(0.55, 0.03, 300.0));
                let t = ((u + v) * 0.5).clamp(0.0, 1.0);
                let base = if t < 0.5 { mix3(a, mid, t * 2.0) } else { mix3(mid, b, (t - 0.5) * 2.0) };
                let (dx, dy) = (u - 0.22, v - 0.78);
                let r = (dx * dx + dy * dy).sqrt();
                let ring = ((r * 38.0).sin() * 0.5 + 0.5).powf(18.0) * smooth(0.05, 0.12, r) * (1.0 - smooth(0.45, 0.8, r));
                let dot = 1.0 - smooth(0.035, 0.045, r);
                mix3(base, lin(oklch(0.93, 0.05, 330.0)), (ring * 0.55 + dot * 0.9).min(1.0))
            }
            Cover::LowTide => {
                let sky_top = lin(oklch(0.30, 0.07, 255.0));
                let sky_low = lin(oklch(0.66, 0.12, 50.0));
                let sea_top = lin(oklch(0.42, 0.08, 220.0));
                let sea_low = lin(oklch(0.22, 0.05, 235.0));
                let horizon = 0.58;
                let (dx, dy) = (u - 0.5, v - 0.47);
                let sun_r = (dx * dx + dy * dy).sqrt();
                if v < horizon {
                    let sky = mix3(sky_top, sky_low, smooth(0.0, horizon, v).powf(1.4));
                    let sun = 1.0 - smooth(0.155, 0.162, sun_r);
                    let halo = (1.0 - smooth(0.1, 0.42, sun_r)) * 0.35;
                    let sun_c = lin(oklch(0.85, 0.15, 65.0));
                    mix3(mix3(sky, sun_c, halo), sun_c, sun)
                } else {
                    let d = (v - horizon) / (1.0 - horizon);
                    let sea = mix3(sea_top, sea_low, d);
                    let band = ((d * 26.0 + (u * 9.0).sin() * 0.3).sin() * 0.5 + 0.5).powf(6.0);
                    let glint = band * (1.0 - smooth(0.04, 0.2 - d * 0.08, dx.abs())) * (1.0 - d * 0.7);
                    mix3(sea, lin(oklch(0.82, 0.14, 60.0)), glint * 0.85)
                }
            }
            Cover::ConcreteGarden => {
                let concrete = lin(oklch(0.62, 0.02, 70.0));
                let shadow = lin(oklch(0.40, 0.02, 60.0));
                let green = lin(oklch(0.58, 0.17, 140.0));
                let rust = lin(oklch(0.58, 0.14, 45.0));
                let (cx, cy) = ((u * 4.0).floor(), (v * 4.0).floor());
                let (fx, fy) = ((u * 4.0).fract(), (v * 4.0).fract());
                let edge = smooth(0.0, 0.03, fx.min(fy)) * smooth(0.0, 0.03, (1.0 - fx).min(1.0 - fy));
                let pick = hash(cx as u32 * 7 + 1, cy as u32 * 13 + 3);
                let block = if pick > 0.78 { rust } else { mix3(concrete, shadow, pick * 0.6) };
                let base = mix3(shadow, block, edge);
                let (dx, dy) = (u - 0.68, v - 0.66);
                let leaf = (-((dx * dx + dy * dy) * 9.0)).exp() * (0.6 + 0.4 * ((u * 60.0).sin() * (v * 47.0).cos()));
                mix3(base, green, smooth(0.25, 0.6, leaf))
            }
            Cover::Gradient(k) => {
                let (a, b) = match k % 3 {
                    0 => (oklch(0.72, 0.15, 330.0), oklch(0.78, 0.15, 145.0)),
                    1 => (oklch(0.62, 0.16, 145.0), oklch(0.55, 0.17, 320.0)),
                    _ => (oklch(0.42, 0.16, 320.0), oklch(0.55, 0.15, 145.0)),
                };
                mix3(lin(a), lin(b), ((u + v) * 0.5).clamp(0.0, 1.0))
            }
        }
    }
}

/// Colours derived from a cover. Every UI colour is clamped in OKLCH so it stays readable on
/// Spindle's dark surfaces whatever the art is.
#[derive(Clone, Copy, Debug)]
pub struct ArtPalette {
    /// The most colourful well-represented swatch, as is.
    pub vivid: Color,
    /// A second swatch far from `vivid` in hue, or a shifted `vivid`.
    pub second: Color,
    /// Swatches by population, most first.
    pub swatches: [Color; 5],
    /// Light enough to read as light on dark: glows, bars, rings.
    pub glow: Color,
    pub glow2: Color,
    /// Saturated mid tones for thin lines of light, where pastel would read as grey.
    pub line: Color,
    pub line2: Color,
    /// Deep tones for backgrounds under white text.
    pub wash: Color,
    pub wash2: Color,
}

fn lch(c: Color) -> (f32, f32, f32) {
    let [l, a, b, _] = c.to_oklab();
    (l, (a * a + b * b).sqrt(), b.atan2(a).to_degrees().rem_euclid(360.0))
}

impl ArtPalette {
    pub fn from_rgba(rgba: &[u8], width: u32, height: u32) -> ArtPalette {
        const N: u32 = 24;
        let mut samples: Vec<[f32; 3]> = Vec::with_capacity((N * N) as usize);
        for ty in 0..N {
            for tx in 0..N {
                let (x0, x1) = (tx * width / N, (tx + 1) * width / N);
                let (y0, y1) = (ty * height / N, (ty + 1) * height / N);
                let mut acc = [0.0f32; 3];
                let mut n = 0.0;
                for y in (y0..y1).step_by(2) {
                    for x in (x0..x1).step_by(2) {
                        let i = ((y * width + x) * 4) as usize;
                        for c in 0..3 {
                            acc[c] += srgb_decode(rgba[i + c]);
                        }
                        n += 1.0;
                    }
                }
                let c = Color { r: acc[0] / n, g: acc[1] / n, b: acc[2] / n, a: 1.0 };
                let [l, a, b, _] = c.to_oklab();
                samples.push([l, a, b]);
            }
        }
        let k = 5;
        let mut centres = vec![samples[0]];
        while centres.len() < k {
            let far = samples
                .iter()
                .max_by(|p, q| {
                    let d = |s: &[f32; 3]| centres.iter().map(|c| dist(s, c)).fold(f32::MAX, f32::min);
                    d(p).total_cmp(&d(q))
                })
                .copied()
                .unwrap();
            centres.push(far);
        }
        let mut counts = vec![0usize; k];
        for _ in 0..12 {
            let mut sums = vec![[0.0f32; 3]; k];
            counts = vec![0; k];
            for s in &samples {
                let j = (0..k).min_by(|a, b| dist(s, &centres[*a]).total_cmp(&dist(s, &centres[*b]))).unwrap();
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
        let swatches: [Color; 5] = std::array::from_fn(|i| swatch(order[i]));
        let score = |j: usize| {
            let (_, c, _) = lch(swatch(j));
            c * (counts[j] as f32 / total).sqrt()
        };
        let vivid_j = (0..k).max_by(|a, b| score(*a).total_cmp(&score(*b))).unwrap();
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
        ArtPalette {
            vivid,
            second,
            swatches,
            glow: tone(vivid, 0.78, 0.17),
            glow2: tone(second, 0.74, 0.15),
            line: tone(vivid, 0.7, 0.2),
            line2: tone(second, 0.68, 0.18),
            wash: tone(vivid, 0.34, 0.10),
            wash2: tone(second, 0.30, 0.08),
        }
    }

    /// Mesh-gradient stops: deep versions of every swatch, so white text reads on top.
    pub fn deep_stops(&self) -> Vec<Color> {
        let mut out = vec![self.wash, self.wash2];
        for (i, c) in self.swatches.iter().enumerate().take(4) {
            let (_, ch, h) = lch(*c);
            out.push(oklch(0.24 + 0.05 * (i % 2) as f32, ch.clamp(0.03, 0.09), h));
        }
        out
    }

    pub fn glow_a(&self, a: f32) -> Color {
        let (l, c, h) = lch(self.glow);
        oklcha(l, c, h, a)
    }
}

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    let w = [2.0, 1.0, 1.0];
    (0..3).map(|i| w[i] * (a[i] - b[i]).powi(2)).sum()
}

/// A cheap stand-in for a heavily blurred cover: box-blurred at 32 px, then upscaled by the GPU's
/// bilinear filter. Computed once per track, so a full-window backdrop costs no blur passes.
pub fn blurred_thumb(rgba: &[u8], width: u32, n: u32) -> Vec<u8> {
    let mut lin: Vec<[f32; 3]> = Vec::with_capacity((n * n) as usize);
    for ty in 0..n {
        for tx in 0..n {
            let mut acc = [0.0f32; 3];
            let mut cnt = 0.0;
            let step = width / n;
            for y in (ty * step..(ty + 1) * step).step_by(3) {
                for x in (tx * step..(tx + 1) * step).step_by(3) {
                    let i = ((y * width + x) * 4) as usize;
                    for c in 0..3 {
                        acc[c] += srgb_decode(rgba[i + c]);
                    }
                    cnt += 1.0;
                }
            }
            lin.push(acc.map(|v| v / cnt));
        }
    }
    for _ in 0..3 {
        let src = lin.clone();
        for y in 0..n as i32 {
            for x in 0..n as i32 {
                let mut acc = [0.0f32; 3];
                let mut cnt = 0.0;
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let (sx, sy) = ((x + dx).clamp(0, n as i32 - 1), (y + dy).clamp(0, n as i32 - 1));
                        let s = src[(sy * n as i32 + sx) as usize];
                        for c in 0..3 {
                            acc[c] += s[c];
                        }
                        cnt += 1.0;
                    }
                }
                lin[(y * n as i32 + x) as usize] = acc.map(|v| v / cnt);
            }
        }
    }
    let mut out = Vec::with_capacity((n * n * 4) as usize);
    for p in lin {
        for c in p {
            out.push(srgb_encode(c));
        }
        out.push(255);
    }
    out
}
