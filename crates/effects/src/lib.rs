//! Weather effect simulations and their HLSL shaders.

mod fireflies;
mod petals;
mod pile;
mod rain;
mod rest;
mod snow;
mod snowball;
mod starfall;
mod util;

/// Default maximum ground snow height in pixels at 1080p (`Effect::set_pile_depth`).
pub const DEFAULT_PILE_DEPTH: f32 = pile::DEFAULT_DEPTH;

use snowy_core::{Effect, EffectKind};

/// Create an effect sized for an overlay of `width` x `height` physical pixels.
pub fn create(kind: EffectKind, width: f32, height: f32) -> Box<dyn Effect> {
    let (w, h) = util::sanitize(width, height);
    match kind {
        EffectKind::Snow => Box::new(snow::Snow::new(w, h, false)),
        EffectKind::HeavySnow => Box::new(snow::Snow::new(w, h, true)),
        EffectKind::Rain => Box::new(rain::Rain::new(w, h)),
        EffectKind::Fireflies => Box::new(fireflies::Fireflies::new(w, h)),
        EffectKind::Sakura => Box::new(petals::Petals::new(w, h, petals::SAKURA)),
        EffectKind::Leaves => Box::new(petals::Petals::new(w, h, petals::LEAVES)),
        EffectKind::Starfall => Box::new(starfall::Starfall::new(w, h)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snowy_core::{Env, Scene};

    fn check(e: &dyn Effect, w: f32, h: f32) {
        let m = 400.0;
        assert!(!e.instances().is_empty());
        for i in e.instances() {
            assert!(i.pos[0].is_finite() && i.pos[1].is_finite());
            assert!(i.rot.is_finite());
            assert!(i.size.is_finite() && i.size > 0.0);
            for c in i.color {
                assert!((0.0..=1.0).contains(&c), "color {:?}", i.color);
            }
            assert!(i.param.iter().all(|p| p.is_finite()));
            assert!(i.pos[0] > -m && i.pos[0] < w + m, "x {}", i.pos[0]);
            assert!(i.pos[1] > -m && i.pos[1] < h + m, "y {}", i.pos[1]);
        }
    }

    #[test]
    fn all_effects_simulate() {
        for kind in EffectKind::ALL {
            for wind in [-1.0f32, 0.0, 1.0] {
                let mut e = create(kind, 1920.0, 1080.0);
                let mut t = 0.0;
                for _ in 0..600 {
                    t += 1.0 / 60.0;
                    let env = Env { wind, intensity: 1.0, speed: 1.0, time: t };
                    e.update(1.0 / 60.0, &env, &Scene::EMPTY);
                }
                check(e.as_ref(), 1920.0, 1080.0);
            }
        }
    }

    #[test]
    fn resize_and_extremes() {
        for kind in EffectKind::ALL {
            let mut e = create(kind, 1920.0, 1080.0);
            let mut t = 0.0;
            for (w, h, inten, spd) in [(3840.0, 2160.0, 2.0, 2.0), (800.0, 600.0, 0.25, 0.5), (1920.0, 1080.0, 1.0, 1.0)] {
                e.resize(w, h);
                for k in 0..120 {
                    t += 0.05;
                    let dt = if k % 10 == 0 { 0.1 } else { 0.016 };
                    e.update(dt, &Env { wind: 1.0, intensity: inten, speed: spd, time: t }, &Scene::EMPTY);
                }
                check(e.as_ref(), w, h);
            }
        }
    }

    #[test]
    fn shaders_have_entry_points() {
        for kind in EffectKind::ALL {
            let e = create(kind, 640.0, 480.0);
            let s = e.shader();
            for needle in ["VSMain", "PSMain", "SV_VertexID", "register(b0)"] {
                assert!(s.contains(needle), "{kind:?} missing {needle}");
            }
            assert!(std::ptr::eq(s, e.shader()));
        }
    }

    #[test]
    fn heavy_snow_4k_speed() {
        let mut e = create(EffectKind::HeavySnow, 3840.0, 2160.0);
        let env = Env { wind: 0.5, intensity: 2.0, speed: 1.0, time: 0.0 };
        e.update(1.0 / 60.0, &env, &Scene::EMPTY);
        let t0 = std::time::Instant::now();
        for _ in 0..100 {
            e.update(1.0 / 60.0, &env, &Scene::EMPTY);
        }
        eprintln!("heavy snow 4K update: {:?}/frame", t0.elapsed() / 100);
    }
}
