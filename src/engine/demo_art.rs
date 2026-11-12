//! Three of the demo albums get painted covers, so the visualizers have real colour to work
//! with in headless scenes and videos.

use rune::{Color, oklch};

const SIZE: u32 = 320;

fn srgb_encode(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
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

#[derive(Clone, Copy)]
enum Cover {
    /// Velvet Antenna, "Carrier Wave": magenta to green with radio rings.
    CarrierWave,
    /// Marlowe & The Estuary, "Low Tide Radio": a low sun over banded water.
    LowTide,
    /// Okonkwo Sound System, "Concrete Garden": warm concrete blocks with a green garden.
    ConcreteGarden,
}

impl Cover {
    /// Straight-alpha sRGB RGBA, `COVER_PX` square.
    fn render(self) -> Vec<u8> {
        let n = SIZE;
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
        }
    }
}

/// A PNG for the demo album at `index`, if it has a painted cover.
pub(super) fn cover(index: usize) -> Option<Vec<u8>> {
    let art = match index {
        0 => Cover::LowTide,
        2 => Cover::ConcreteGarden,
        3 => Cover::CarrierWave,
        _ => return None,
    };
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, SIZE, SIZE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().and_then(|mut writer| writer.write_image_data(&art.render())).ok()?;
    Some(out)
}
