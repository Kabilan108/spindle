//! The compositor: each `El::effect` slot becomes rune-fx layers built from `State::sc.fx`, which
//! the poll advances each frame. The view never re-runs for these effects; the compositor reads
//! the numbers itself and goes idle (`animating() == false`) once they settle.

use rune::compose::{Compositor, Frame};
use rune::render::{EffectPhase, EffectSite, KIND_EFFECT};
use rune_fx::*;

use crate::State;

/// Album-art mesh gradient filling the slot. Data: unused.
pub const MESH: u32 = 1;
/// Beat ripples confined to the slot. Data: [origin x, origin y, intensity, 0] in window points.
pub const RIPPLE: u32 = 2;
/// Soft glow around the slot's shape. Data: [radius, intensity, 0, 0].
pub const GLOW: u32 = 3;
/// Static grain against banding. Data: [amount, 0, 0, 0].
pub const GRAIN: u32 = 4;

pub const RIPPLE_LIFE: f32 = 1.5;

fn lin(c: rune::Color) -> Rgb {
    Rgb([c.r, c.g, c.b])
}

#[derive(Default)]
pub struct Gfx {
    fx: Option<FxRenderer>,
    animating: bool,
}

impl Compositor<State> for Gfx {
    fn animating(&self) -> bool {
        self.animating
    }

    fn summary(&mut self, _device: &wgpu::Device) -> Option<String> {
        let st = crate::STATS.lock().unwrap();
        Some(format!(
            "viz: view commits {} ({} after pause), live publishes {} ({} after pause), last change at {:.2} s (paused at {:.1} s)",
            st.commits, st.paused_commits, st.live_sets, st.paused_sets, st.last_change, crate::PAUSE_AT
        ))
    }

    fn render(&mut self, f: &mut Frame<'_, State>) {
        let (device, queue, size) = (f.device, f.queue, f.size);
        let t = f.rt.t as f32;
        let st = &f.rt.state;
        let pal = st.palette();
        let fx_state = &st.sc.fx;
        let mut owners: Vec<(u32, usize)> = Vec::new();
        let mut layers: Vec<FxLayer> = Vec::new();
        let mut slots: Vec<FxSlot> = Vec::new();
        for (n, i) in f.rt.instances.iter().enumerate() {
            if i.params[1] != KIND_EFFECT {
                continue;
            }
            let kind = f.rt.marks.get(n).copied().unwrap_or(0) & 0xff;
            let k = i.params[3];
            let [x, y, w, h] = i.rect.map(|v| v / k);
            let r = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / k;
            let shape = Shape::rounded(x, y, w, h, r);
            let d = i.uv;
            let mut push = |layer: FxLayer| {
                owners.push((n as u32, layers.len()));
                layers.push(layer);
                slots.push(FxSlot::default().scale(k).clip(i.clip, i.fade));
            };
            match kind {
                MESH => {
                    let stops: Vec<Rgb> = pal.deep_stops().into_iter().map(lin).collect();
                    let p = MeshGradientParams { colors: Stops::new(&stops), drift: 0.18, speed: 0.3, sharpness: 1.3, warp: 0.45, ..MeshGradientParams::dusk() };
                    push(Fx::fill(p).on(shape).at(fx_state.time).seed(7));
                }
                GLOW => {
                    if fx_state.heat > 0.0 {
                        let p = GlowParams {
                            style: GlowStyle::Soft,
                            color: lin(pal.glow),
                            core: lin(pal.glow.mix(rune::Color::WHITE, 0.5)),
                            radius: d[0].max(8.0),
                            intensity: d[1].max(0.1),
                            heat: fx_state.heat,
                            occlusion: 0.0,
                            ..Default::default()
                        };
                        push(Fx::glow(p).on(shape).at(t));
                    }
                }
                RIPPLE => {
                    for shot in &fx_state.shots {
                        let age = t - shot.0;
                        if !(0.0..RIPPLE_LIFE).contains(&age) {
                            continue;
                        }
                        let p = PulseParams {
                            style: PulseStyle::Ripple { at: [d[0], d[1]] },
                            color: lin(pal.glow),
                            core: lin(pal.glow.mix(rune::Color::WHITE, 0.4)),
                            reach: 0.0,
                            width: 1.4,
                            t0: shot.0,
                            duration: RIPPLE_LIFE,
                            intensity: shot.1 * d[2].max(0.05),
                            occlusion: 0.0,
                            ..Default::default()
                        };
                        push(Fx::pulse(p).on(shape).at(t));
                    }
                }
                GRAIN => {
                    push(Fx::grain(GrainParams { amount: d[0].max(0.005), fps: 0.0, ..Default::default() }).on(shape).at(0.0));
                }
                _ => {}
            }
        }
        self.animating = layers.iter().any(|l| l.is_animating()) && (fx_state.speed > 0.0 || fx_state.heat > 0.0 || !fx_state.shots.is_empty());
        let fx = self.fx.get_or_insert_with(|| FxRenderer::new(device, f.format, FxOptions { dither: true }));
        fx.set_srgb_blend(f.srgb_blend());
        fx.add_format(device, wgpu::TextureFormat::Rgba16Float);
        fx.prepare_with_slots(device, queue, &FxFrame { viewport: [size.0, size.1], scale: 1.0, layers: &layers, backdrop: None, sources: &[] }, &slots);
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        f.ui.draw_with_backdrop(f.encoder, f.target, clear, &mut |phase, s: &EffectSite<'_>| {
            if let EffectPhase::Draw(pass) = phase {
                for (_, layer) in owners.iter().filter(|(n, _)| *n == s.index) {
                    fx.draw_layer_in(pass, s.format, s.origin, *layer);
                }
            }
        });
    }
}
