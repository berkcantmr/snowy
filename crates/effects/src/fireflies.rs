//! Fireflies: slowly wandering, pulsing glow dots.

use crate::util::*;
use fastrand::Rng;
use snowy_core::{Effect, Env, Scene, Instance};

const SHADER: &str = include_str!("shaders/fireflies.hlsl");

struct Fly {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    phase: f32,
    rate: f32,
    size: f32,
    tint: f32,
}

pub struct Fireflies {
    w: f32,
    h: f32,
    rng: Rng,
    flies: Vec<Fly>,
    inst: Vec<Instance>,
}

fn spawn(rng: &mut Rng, w: f32, h: f32) -> Fly {
    Fly {
        x: rng.f32() * w,
        y: h * (0.3 + 0.7 * rng.f32()),
        vx: range(rng, -10.0, 10.0),
        vy: range(rng, -10.0, 10.0),
        phase: rng.f32(),
        rate: range(rng, 1.2, 3.0),
        size: range(rng, 9.0, 15.0),
        tint: rng.f32(),
    }
}

impl Fireflies {
    pub fn new(w: f32, h: f32) -> Self {
        let mut f = Fireflies { w, h, rng: Rng::with_seed(0xf1e5), flies: Vec::new(), inst: Vec::new() };
        f.adjust(1.0);
        f.build();
        f
    }

    fn adjust(&mut self, intensity: f32) {
        let n = target_count(60.0, self.w, self.h, intensity);
        self.flies.truncate(n);
        if self.flies.len() < n {
            self.flies.reserve(n - self.flies.len());
            self.inst.reserve(n);
        }
        while self.flies.len() < n {
            let f = spawn(&mut self.rng, self.w, self.h);
            self.flies.push(f);
        }
    }

    fn build(&mut self) {
        let sc = ui_scale(self.w, self.h);
        self.inst.clear();
        for f in &self.flies {
            self.inst.push(Instance {
                pos: [f.x, f.y],
                size: f.size * sc,
                rot: 0.0,
                color: [lerp(0.7, 0.9, f.tint), 1.0, lerp(0.25, 0.35, f.tint), 0.95],
                param: [f.phase, f.rate, 0.0, 0.0],
            });
        }
    }
}

impl Effect for Fireflies {
    fn resize(&mut self, width: f32, height: f32) {
        let (w, h) = sanitize(width, height);
        let (rx, ry) = (w / self.w, h / self.h);
        for f in &mut self.flies {
            f.x *= rx;
            f.y *= ry;
        }
        self.w = w;
        self.h = h;
        self.adjust(1.0);
        self.build();
    }

    fn update(&mut self, dt: f32, env: &Env, _scene: &Scene) {
        let dt = clamp_dt(dt);
        self.adjust(env.intensity);
        let sc = ui_scale(self.w, self.h);
        let sp = speed_mul(env.speed);
        let wind = wind_val(env.wind);
        let (w, h) = (self.w, self.h);
        let (top, bottom) = (h * 0.3, h - 10.0 * sc);
        let max_v = 38.0 * sc;
        let damp = (1.0 - 0.6 * dt).max(0.0);
        for f in &mut self.flies {
            // Random steering plus a soft push back into the allowed region.
            let ax = range(&mut self.rng, -1.0, 1.0) * 140.0 * sc;
            let ay = range(&mut self.rng, -1.0, 1.0) * 140.0 * sc;
            f.vx = (f.vx + ax * dt) * damp;
            f.vy = (f.vy + ay * dt) * damp;
            if f.x < 0.0 {
                f.vx += 120.0 * sc * dt;
            } else if f.x > w {
                f.vx -= 120.0 * sc * dt;
            }
            if f.y < top {
                f.vy += 120.0 * sc * dt;
            } else if f.y > bottom {
                f.vy -= 120.0 * sc * dt;
            }
            let v = (f.vx * f.vx + f.vy * f.vy).sqrt();
            if v > max_v {
                f.vx *= max_v / v;
                f.vy *= max_v / v;
            }
            f.x += (f.vx + wind * 12.0 * sc) * dt * sp;
            f.y += f.vy * dt * sp;
            f.x = f.x.clamp(-30.0, w + 30.0);
            f.y = f.y.clamp(top - 30.0, h + 30.0);
        }
        self.build();
    }

    fn instances(&self) -> &[Instance] {
        &self.inst
    }

    fn shader(&self) -> &'static str {
        SHADER
    }
}
