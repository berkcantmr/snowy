//! Starfall: faint twinkling stars plus occasional shooting stars.

use crate::util::*;
use fastrand::Rng;
use snowy_core::{Effect, Env, Scene, Instance};

const SHADER: &str = include_str!("shaders/starfall.hlsl");
const MAX_SHOOTERS: usize = 10;

struct Star {
    x: f32,
    y: f32,
    size: f32,
    alpha: f32,
    phase: f32,
    rate: f32,
    tint: f32,
}

struct Shooter {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    len: f32,
}

pub struct Starfall {
    w: f32,
    h: f32,
    rng: Rng,
    stars: Vec<Star>,
    shooters: Vec<Shooter>,
    timer: f32,
    inst: Vec<Instance>,
}

fn spawn_star(rng: &mut Rng, w: f32, h: f32) -> Star {
    let y = if rng.f32() < 0.85 { rng.f32() * 0.6 * h } else { range(rng, 0.6 * h, h) };
    Star {
        x: rng.f32() * w,
        y,
        size: range(rng, 2.0, 5.0),
        alpha: range(rng, 0.35, 0.9),
        phase: rng.f32(),
        rate: range(rng, 1.0, 3.5),
        tint: rng.f32(),
    }
}

impl Starfall {
    pub fn new(w: f32, h: f32) -> Self {
        let mut s = Starfall {
            w,
            h,
            rng: Rng::with_seed(0x57a2),
            stars: Vec::new(),
            shooters: Vec::with_capacity(MAX_SHOOTERS),
            timer: 1.0,
            inst: Vec::new(),
        };
        s.adjust(1.0);
        s.build();
        s
    }

    fn adjust(&mut self, intensity: f32) {
        let n = target_count(150.0, self.w, self.h, intensity);
        self.stars.truncate(n);
        if self.stars.len() < n {
            self.stars.reserve(n - self.stars.len());
            self.inst.reserve(n + MAX_SHOOTERS);
        }
        while self.stars.len() < n {
            let s = spawn_star(&mut self.rng, self.w, self.h);
            self.stars.push(s);
        }
    }

    fn next_delay(&mut self, intensity: f32) -> f32 {
        range(&mut self.rng, 1.5, 5.0) / intensity.clamp(0.1, 4.0)
    }

    fn spawn_shooter(&mut self, sc: f32, sp: f32) {
        let rng = &mut self.rng;
        let dir = if rng.bool() { 1.0 } else { -1.0 };
        let ang = range(rng, 20.0, 50.0f32).to_radians();
        let speed = range(rng, 900.0, 1400.0) * sc * sp;
        let life = range(rng, 0.8, 1.5);
        self.shooters.push(Shooter {
            x: range(rng, 0.1, 0.9) * self.w,
            y: range(rng, 0.02, 0.4) * self.h,
            vx: ang.cos() * speed * dir,
            vy: ang.sin() * speed,
            age: 0.0,
            life,
            len: range(rng, 180.0, 320.0) * sc,
        });
    }

    fn build(&mut self) {
        let sc = ui_scale(self.w, self.h);
        self.inst.clear();
        for s in &self.stars {
            self.inst.push(Instance {
                pos: [s.x, s.y],
                size: s.size * sc,
                rot: 0.0,
                color: [lerp(0.8, 1.0, s.tint), lerp(0.88, 0.95, s.tint), lerp(1.0, 0.85, s.tint), s.alpha],
                param: [s.phase, 0.0, 0.0, s.rate],
            });
        }
        for sh in &self.shooters {
            let v = (sh.vx * sh.vx + sh.vy * sh.vy).sqrt().max(1e-3);
            let (dx, dy) = (sh.vx / v, sh.vy / v);
            let half = sh.len * 0.5;
            // Head sits at the leading edge of the quad (local +Y = travel direction).
            let fade = (sh.age / 0.08).min(1.0) * (1.0 - sh.age / sh.life).clamp(0.0, 1.0);
            self.inst.push(Instance {
                pos: [sh.x - dx * half, sh.y - dy * half],
                size: half,
                rot: rot_for(dx, dy),
                color: [0.85, 0.93, 1.0, fade.clamp(0.0, 1.0)],
                param: [0.0, 1.0, (2.0 * sc / half).min(0.5), 0.0],
            });
        }
    }
}

impl Effect for Starfall {
    fn resize(&mut self, width: f32, height: f32) {
        let (w, h) = sanitize(width, height);
        let (rx, ry) = (w / self.w, h / self.h);
        for s in &mut self.stars {
            s.x *= rx;
            s.y *= ry;
        }
        for s in &mut self.shooters {
            s.x *= rx;
            s.y *= ry;
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

        self.timer -= dt;
        if self.timer <= 0.0 {
            if self.shooters.len() < MAX_SHOOTERS {
                self.spawn_shooter(sc, sp);
            }
            self.timer = self.next_delay(env.intensity);
        }
        let (w, h) = (self.w, self.h);
        for s in &mut self.shooters {
            s.age += dt;
            s.x += (s.vx + wind * 40.0 * sc) * dt;
            s.y += s.vy * dt;
        }
        self.shooters
            .retain(|s| s.age < s.life && s.x > -300.0 && s.x < w + 300.0 && s.y < h + 300.0);
        self.build();
    }

    fn instances(&self) -> &[Instance] {
        &self.inst
    }

    fn shader(&self) -> &'static str {
        SHADER
    }
}
