//! Rain: thin fast streaks angled by wind.

use crate::rest::find_land;
use crate::util::*;
use fastrand::Rng;
use snowy_core::{Effect, Env, Instance, Scene};

const SHADER: &str = include_str!("shaders/rain.hlsl");

/// Max live splash particles (droplets + ripples).
const SPLASH_CAP: usize = 400;
/// Fraction of hits that make a splash.
const SPLASH_CHANCE: f32 = 0.6;

#[derive(Clone, Copy)]
struct Splash {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    /// true = ripple ring, false = droplet.
    ripple: bool,
}

struct Drop {
    x: f32,
    y: f32,
    speed: f32,
    len: f32,
    width: f32,
    alpha: f32,
}

pub struct Rain {
    w: f32,
    h: f32,
    rng: Rng,
    drops: Vec<Drop>,
    splashes: Vec<Splash>,
    hits: Vec<(f32, f32)>,
    inst: Vec<Instance>,
}

fn spawn(rng: &mut Rng, x: f32, y: f32) -> Drop {
    let z = rng.f32();
    Drop {
        x,
        y,
        speed: lerp(1100.0, 1900.0, z),
        len: lerp(35.0, 80.0, z),
        width: lerp(1.4, 2.2, z),
        alpha: lerp(0.22, 0.5, z),
    }
}

impl Rain {
    pub fn new(w: f32, h: f32) -> Self {
        let mut r = Rain { w, h, rng: Rng::with_seed(0x7a1), drops: Vec::new(), splashes: Vec::with_capacity(SPLASH_CAP), hits: Vec::with_capacity(64), inst: Vec::new() };
        r.adjust(1.0);
        r.build(0.0, 1.0);
        r
    }

    fn adjust(&mut self, intensity: f32) {
        let n = target_count(450.0, self.w, self.h, intensity);
        self.drops.truncate(n);
        if self.drops.len() < n {
            self.drops.reserve(n - self.drops.len());
            self.inst.reserve(n + SPLASH_CAP);
        }
        while self.drops.len() < n {
            let (x, y) = (self.rng.f32() * self.w, self.rng.f32() * self.h);
            let d = spawn(&mut self.rng, x, y);
            self.drops.push(d);
        }
    }

    fn build(&mut self, wind: f32, sp: f32) {
        let sc = ui_scale(self.w, self.h);
        self.inst.clear();
        for d in &self.drops {
            let vx = wind * 260.0 * sc * sp;
            let vy = d.speed * sc * sp;
            let half = d.len * 0.5 * sc;
            self.inst.push(Instance {
                pos: [d.x, d.y],
                size: half,
                rot: rot_for(vx, vy),
                color: [0.65, 0.75, 0.9, d.alpha],
                param: [(d.width * 0.5 * sc / half).min(0.5), 0.0, 0.0, 0.0],
            });
        }
        for s in &self.splashes {
            let f = (s.age / s.life).clamp(0.0, 1.0);
            if s.ripple {
                self.inst.push(Instance {
                    pos: [s.x, s.y],
                    size: lerp(3.0, 15.0, f.sqrt()) * sc,
                    rot: 0.0,
                    color: [0.75, 0.83, 0.95, 0.4 * (1.0 - f)],
                    param: [0.0, 0.0, 0.0, 2.0],
                });
            } else {
                self.inst.push(Instance {
                    pos: [s.x, s.y],
                    size: 1.5 * sc,
                    rot: 0.0,
                    color: [0.75, 0.83, 0.95, 0.6 * (1.0 - f * f)],
                    param: [0.0, 0.0, 0.0, 1.0],
                });
            }
        }
    }

    fn splash(&mut self, x: f32, y: f32, sc: f32) {
        if self.rng.f32() > SPLASH_CHANCE {
            return;
        }
        let n = 2 + self.rng.usize(..3);
        if self.splashes.len() + n + 1 > SPLASH_CAP {
            return;
        }
        for _ in 0..n {
            let (vx, vy) = (range(&mut self.rng, -90.0, 90.0) * sc, -range(&mut self.rng, 130.0, 260.0) * sc);
            self.splashes.push(Splash { x, y: y - 1.0, vx, vy, age: 0.0, life: 0.25, ripple: false });
        }
        self.splashes.push(Splash { x, y: y - 1.0, vx: 0.0, vy: 0.0, age: 0.0, life: 0.3, ripple: true });
    }
}

impl Effect for Rain {
    fn resize(&mut self, width: f32, height: f32) {
        let (w, h) = sanitize(width, height);
        let (rx, ry) = (w / self.w, h / self.h);
        for d in &mut self.drops {
            d.x *= rx;
            d.y *= ry;
        }
        self.w = w;
        self.h = h;
        self.adjust(1.0);
        self.build(0.0, 1.0);
    }

    fn update(&mut self, dt: f32, env: &Env, scene: &Scene) {
        let dt = clamp_dt(dt);
        self.adjust(env.intensity);
        let sc = ui_scale(self.w, self.h);
        let sp = speed_mul(env.speed);
        let wind = wind_val(env.wind);
        let (w, h) = (self.w, self.h);
        let vx = wind * 260.0 * sc * sp;
        let m = 100.0 * sc;
        self.hits.clear();
        let hits = &mut self.hits;
        for d in &mut self.drops {
            let head = d.len * 0.5 * sc;
            let prev = d.y + head;
            d.x += vx * dt;
            d.y += d.speed * sc * sp * dt;
            if let Some((_, ly)) = find_land(scene, d.x, prev, d.y + head) {
                hits.push((d.x, ly));
                d.y = -d.len * sc - self.rng.f32() * 0.1 * h;
                d.x = self.rng.f32() * w;
            } else if d.y - d.len * sc > h {
                d.y = -d.len * sc - self.rng.f32() * 0.1 * h;
                d.x = self.rng.f32() * w;
            }
            d.x = wrap(d.x, -m, w + m);
        }
        for i in 0..self.hits.len() {
            let (x, y) = self.hits[i];
            self.splash(x, y, sc);
        }
        // Advance splashes (new ones start at age 0 and are already in the list).
        let mut i = 0;
        while i < self.splashes.len() {
            let s = &mut self.splashes[i];
            s.age += dt;
            if s.age >= s.life {
                self.splashes.swap_remove(i);
                continue;
            }
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.vy += 900.0 * sc * dt;
            i += 1;
        }
        self.build(wind, sp);
    }

    fn clear_accumulation(&mut self) {
        self.splashes.clear();
        self.inst.truncate(self.drops.len());
    }

    fn instances(&self) -> &[Instance] {
        &self.inst
    }

    fn shader(&self) -> &'static str {
        SHADER
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snowy_core::Surface;

    fn env() -> Env {
        Env { wind: 0.0, intensity: 1.0, speed: 1.0, time: 0.0 }
    }

    fn splashes(r: &Rain) -> usize {
        r.instances().iter().filter(|i| i.param[3] > 0.5).count()
    }

    fn sane(r: &Rain) {
        for i in r.instances() {
            assert!(i.pos.iter().all(|v| v.is_finite()) && i.rot.is_finite());
            assert!(i.size.is_finite() && i.size > 0.0);
            assert!(i.color.iter().all(|c| (0.0..=1.0).contains(c)), "{:?}", i.color);
            assert!(i.param.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn splashes_with_surface_not_without() {
        let s = [Surface { id: 1, x0: 0.0, x1: 1920.0, y: 600.0 }];
        let scene = Scene { surfaces: &s, ground: Some(1080.0), pointer: None, snowballs: false };
        let mut r = Rain::new(1920.0, 1080.0);
        let mut max = 0;
        for _ in 0..120 {
            r.update(1.0 / 60.0, &env(), &scene);
            max = max.max(splashes(&r));
            sane(&r);
        }
        assert!(max > 0);
        assert!(r.splashes.len() <= SPLASH_CAP);
        r.clear_accumulation();
        assert_eq!(splashes(&r), 0);
        assert_eq!(r.instances().len(), r.drops.len());

        let mut r = Rain::new(1920.0, 1080.0);
        for _ in 0..120 {
            r.update(1.0 / 60.0, &env(), &Scene::EMPTY);
            assert_eq!(splashes(&r), 0);
        }
        sane(&r);
    }

    #[test]
    fn splashes_expire() {
        let scene = Scene { surfaces: &[], ground: Some(1080.0), pointer: None, snowballs: false };
        let mut r = Rain::new(1920.0, 1080.0);
        for _ in 0..120 {
            r.update(1.0 / 60.0, &env(), &scene);
        }
        assert!(splashes(&r) > 0);
        for _ in 0..60 {
            r.update(1.0 / 60.0, &env(), &Scene::EMPTY);
        }
        assert_eq!(splashes(&r), 0);
    }
}
