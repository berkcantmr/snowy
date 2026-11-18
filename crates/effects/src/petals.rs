//! Sakura petals and autumn leaves: tumbling, flipping, swaying fallers that can come to
//! rest on window ledges and the ground, and (leaves) be kicked around by the pointer.

use crate::rest::find_land;
use crate::util::*;
use fastrand::Rng;
use snowy_core::{Effect, Env, Instance, Scene};

const SHADER: &str = include_str!("shaders/petals.hlsl");

/// Max timed resting items (on ledges) per monitor.
const REST_CAP: usize = 80;
/// Window ledge must stay within this many px of where the item landed.
const LEDGE_TOL: f32 = 2.0;
/// Fraction of the half-extent that reaches below the centre when lying flat.
const FOOT: f32 = 0.4;
const FADE_TIME: f32 = 1.0;

/// Pointer speed (px/s at 1080p) below which leaves are not disturbed at all.
const KICK_MIN_SPEED: f32 = 250.0;
/// Pointer speed at which a kick reaches full strength.
const KICK_FULL_SPEED: f32 = 2600.0;
/// Reach of the pointer "foot" (px at 1080p).
const KICK_RADIUS: f32 = 42.0;
/// Below this energy a leaf only slides/turns on the ground instead of taking off.
const KICK_HOP: f32 = 0.18;
/// Gravity for kicked leaves (px/s² at 1080p); lighter than a stone.
const KICK_GRAVITY: f32 = 950.0;

/// Kick energy 0..1 from pointer speed (px/s, 1080p-normalised) and distance as a
/// fraction of the kick radius (0 = right under the cursor). Ramps smoothly with speed
/// (smoothstep) and falls off quadratically with distance, so a gentle brush only
/// nudges nearby leaves and only a fast sweep sends them flying.
fn kick_energy(speed: f32, dist_frac: f32) -> f32 {
    let s = ((speed - KICK_MIN_SPEED) / (KICK_FULL_SPEED - KICK_MIN_SPEED)).clamp(0.0, 1.0);
    let s = s * s * (3.0 - 2.0 * s);
    let f = (1.0 - dist_frac.clamp(0.0, 1.0)).powi(2);
    s * f
}

/// Per-variant tuning.
pub struct Cfg {
    count: f32,
    size: (f32, f32),
    fall: (f32, f32),
    rot_speed: f32,
    flip: (f32, f32),
    sway_amp: (f32, f32),
    wind_resp: (f32, f32),
    shape: f32,
    alpha: f32,
    palette: &'static [[f32; 3]],
    seed: u64,
    /// Seconds an item lies on a ledge before fading.
    rest_life: (f32, f32),
    /// Items landing on the ground stay forever (pile) instead of timing out.
    ground_stays: bool,
    /// Pointer sweeps launch resting items.
    kick: bool,
    /// Max persistent ground items at 1920 px width.
    ground_cap: f32,
}

pub const SAKURA: Cfg = Cfg {
    count: 120.0,
    size: (7.0, 12.0),
    fall: (40.0, 80.0),
    rot_speed: 1.5,
    flip: (1.5, 3.5),
    sway_amp: (15.0, 40.0),
    wind_resp: (60.0, 140.0),
    shape: 0.0,
    alpha: 0.9,
    palette: &[
        [1.0, 0.72, 0.82],
        [1.0, 0.80, 0.87],
        [0.98, 0.64, 0.76],
        [1.0, 0.88, 0.92],
    ],
    seed: 0x5a4,
    rest_life: (4.0, 10.0),
    ground_stays: false,
    kick: false,
    ground_cap: 0.0,
};

pub const LEAVES: Cfg = Cfg {
    count: 90.0,
    size: (11.0, 18.0),
    fall: (70.0, 130.0),
    rot_speed: 2.5,
    flip: (1.0, 3.0),
    sway_amp: (30.0, 70.0),
    wind_resp: (80.0, 180.0),
    shape: 1.0,
    alpha: 0.95,
    palette: &[
        [0.90, 0.45, 0.10],
        [0.78, 0.20, 0.10],
        [0.55, 0.30, 0.12],
        [0.95, 0.72, 0.15],
        [0.70, 0.35, 0.08],
    ],
    seed: 0x1ea5,
    rest_life: (6.0, 15.0),
    ground_stays: true,
    kick: true,
    ground_cap: 220.0,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum St {
    /// Drifting down.
    Fall,
    /// Kicked: ballistic arc with gravity.
    Fly,
    /// Lying on a ledge / the ground.
    Rest,
    /// Lying, fading out before respawn.
    Fade,
}

struct Petal {
    x: f32,
    y: f32,
    vx: f32,
    fall: f32,
    resp: f32,
    size: f32,
    rot: f32,
    rot_speed: f32,
    flip: f32,
    flip_speed: f32,
    phase: f32,
    freq: f32,
    amp: f32,
    seed: f32,
    color: [f32; 3],
    st: St,
    /// Window id it lies on; None = ground (only meaningful in Rest/Fade).
    surf: Option<u64>,
    /// Y of the ledge at landing time.
    surf_y: f32,
    /// Distance from ground line to centre (ground resting only).
    rest_off: f32,
    rest_rot: f32,
    rest_flip: f32,
    t_rest: f32,
    /// Seconds before fading; infinite for pile items.
    life: f32,
    fade_t: f32,
    seq: u32,
    kvx: f32,
    kvy: f32,
}

impl Petal {
    fn persistent(&self) -> bool {
        self.life.is_infinite()
    }
    fn resting(&self) -> bool {
        matches!(self.st, St::Rest | St::Fade)
    }
    fn pile_item(&self) -> bool {
        self.st == St::Rest && self.persistent()
    }
}

pub struct Petals {
    w: f32,
    h: f32,
    t: f32,
    cfg: Cfg,
    rng: Rng,
    seq: u32,
    items: Vec<Petal>,
    inst: Vec<Instance>,
}

fn spawn(rng: &mut Rng, cfg: &Cfg, x: f32, y: f32) -> Petal {
    let c = cfg.palette[rng.usize(..cfg.palette.len())];
    let j = range(rng, 0.94, 1.06);
    Petal {
        x,
        y,
        vx: 0.0,
        fall: range(rng, cfg.fall.0, cfg.fall.1),
        resp: range(rng, cfg.wind_resp.0, cfg.wind_resp.1),
        size: range(rng, cfg.size.0, cfg.size.1),
        rot: range(rng, 0.0, std::f32::consts::TAU),
        rot_speed: range(rng, -cfg.rot_speed, cfg.rot_speed),
        flip: range(rng, 0.0, std::f32::consts::TAU),
        flip_speed: range(rng, cfg.flip.0, cfg.flip.1),
        phase: range(rng, 0.0, std::f32::consts::TAU),
        freq: range(rng, 0.5, 1.4),
        amp: range(rng, cfg.sway_amp.0, cfg.sway_amp.1),
        seed: rng.f32(),
        color: [(c[0] * j).min(1.0), (c[1] * j).min(1.0), (c[2] * j).min(1.0)],
        st: St::Fall,
        surf: None,
        surf_y: 0.0,
        rest_off: 0.0,
        rest_rot: 0.0,
        rest_flip: 0.0,
        t_rest: 0.0,
        life: 0.0,
        fade_t: 0.0,
        seq: 0,
        kvx: 0.0,
        kvy: 0.0,
    }
}

/// How many pile items already lie near `x` (capped at 3 layers).
fn pile_depth(items: &[Petal], x: f32, reach: f32) -> usize {
    items.iter().filter(|p| p.pile_item() && (p.x - x).abs() < reach).count().min(3)
}

impl Petals {
    pub fn new(w: f32, h: f32, cfg: Cfg) -> Self {
        let mut p = Petals {
            w,
            h,
            t: 0.0,
            rng: Rng::with_seed(cfg.seed),
            cfg,
            seq: 0,
            items: Vec::new(),
            inst: Vec::new(),
        };
        p.adjust(1.0);
        p.build();
        p
    }

    /// Keep the number of "active" items (everything except pile items) at the target.
    fn adjust(&mut self, intensity: f32) {
        let n = target_count(self.cfg.count, self.w, self.h, intensity);
        let mut active = self.items.iter().filter(|p| !p.pile_item()).count();
        while active > n {
            if let Some(i) = self.items.iter().rposition(|p| !p.pile_item()) {
                self.items.swap_remove(i);
            }
            active -= 1;
        }
        if active < n {
            self.items.reserve(n - active);
            self.inst.reserve(n + 16);
        }
        while active < n {
            let (x, y) = (self.rng.f32() * self.w, self.rng.f32() * self.h);
            let p = spawn(&mut self.rng, &self.cfg, x, y);
            self.items.push(p);
            active += 1;
        }
    }

    fn respawn(&mut self, i: usize) {
        let m = 30.0 * ui_scale(self.w, self.h);
        let x = self.rng.f32() * self.w;
        let y = -m - self.rng.f32() * m;
        self.items[i] = spawn(&mut self.rng, &self.cfg, x, y);
    }

    fn build(&mut self) {
        let sc = ui_scale(self.w, self.h);
        let (shape, alpha) = (self.cfg.shape, self.cfg.alpha);
        self.inst.clear();
        for p in &self.items {
            let (rot, flip, a) = match p.st {
                St::Rest => (p.rest_rot, p.rest_flip, alpha),
                St::Fade => (p.rest_rot, p.rest_flip, alpha * (1.0 - p.fade_t / FADE_TIME).clamp(0.0, 1.0)),
                _ => (p.rot, p.flip, alpha),
            };
            self.inst.push(Instance {
                pos: [p.x, p.y],
                size: p.size * sc,
                rot,
                color: [p.color[0], p.color[1], p.color[2], a],
                param: [flip, shape, p.seed, 0.0],
            });
        }
    }

    fn land(&mut self, i: usize, id: Option<u64>, ly: f32, sc: f32) {
        let persist = id.is_none() && self.cfg.ground_stays;
        let (x, size) = (self.items[i].x, self.items[i].size * sc);
        let depth = if persist { pile_depth(&self.items, x, size * 1.2) } else { 0 };
        self.seq += 1;
        let life = if persist { f32::INFINITY } else { range(&mut self.rng, self.cfg.rest_life.0, self.cfg.rest_life.1) };
        let jitter = if persist { 0.8 } else { 0.4 };
        let base = if self.rng.bool() { 1.0 } else { -1.0 } * std::f32::consts::FRAC_PI_2;
        let rest_rot = base + range(&mut self.rng, -jitter, jitter);
        let rest_flip = range(&mut self.rng, 0.7, 1.2);
        let off = size * FOOT + depth as f32 * size * 0.3;
        let p = &mut self.items[i];
        p.st = St::Rest;
        p.surf = id;
        p.surf_y = ly;
        p.rest_off = off;
        p.y = ly - off;
        p.rest_rot = rest_rot;
        p.rest_flip = rest_flip;
        p.t_rest = 0.0;
        p.life = life;
        p.fade_t = 0.0;
        p.seq = self.seq;
        p.vx = 0.0;
    }
}

impl Effect for Petals {
    fn resize(&mut self, width: f32, height: f32) {
        let (w, h) = sanitize(width, height);
        let (rx, ry) = (w / self.w, h / self.h);
        for p in &mut self.items {
            p.x *= rx;
            p.y *= ry;
            p.surf_y *= ry;
            p.rest_off *= ry;
        }
        self.w = w;
        self.h = h;
        self.adjust(1.0);
        self.build();
    }

    fn update(&mut self, dt: f32, env: &Env, scene: &Scene) {
        let dt = clamp_dt(dt);
        self.t += dt;
        self.adjust(env.intensity);
        let sc = ui_scale(self.w, self.h);
        let sp = speed_mul(env.speed);
        let wind = wind_val(env.wind);
        let (w, h, t) = (self.w, self.h, self.t);
        let m = 30.0 * sc;
        let k = (dt * 1.5).min(1.0);

        let mut n_timed = 0usize;
        let mut n_pile = 0usize;
        for p in &self.items {
            if p.resting() {
                if p.persistent() {
                    n_pile += 1;
                } else {
                    n_timed += 1;
                }
            }
        }
        let pile_cap = ((self.cfg.ground_cap * w / REF_W).round() as usize).max(1);

        // Pointer sweep that can kick resting items.
        let kick = match scene.pointer {
            Some(pt)
                if self.cfg.kick
                    && pt.pos.iter().chain(pt.vel.iter()).all(|v| v.is_finite())
                    && pt.vel[0].hypot(pt.vel[1]) > KICK_MIN_SPEED * sc
                    && (pt.over_desktop
                        || scene.ground.is_some_and(|g| (g - pt.pos[1]).abs() < 80.0 * sc)) =>
            {
                Some(pt)
            }
            _ => None,
        };
        let kick_r = KICK_RADIUS * sc;
        let kick_r2 = kick_r * kick_r;

        for i in 0..self.items.len() {
            match self.items[i].st {
                St::Fall | St::Fly => {
                    let p = &mut self.items[i];
                    let prev_foot = p.y + p.size * sc * FOOT;
                    p.flip += p.flip_speed * dt * sp;
                    p.rot += p.rot_speed * dt * sp;
                    let dy;
                    if p.st == St::Fly {
                        // Leaves are light: modest gravity, strong air drag both ways.
                        p.kvy += KICK_GRAVITY * sc * dt;
                        p.kvx *= 1.0 - (2.2 * dt).min(1.0);
                        if p.kvy < 0.0 {
                            p.kvy *= 1.0 - (1.2 * dt).min(1.0);
                        }
                        p.x += p.kvx * dt;
                        dy = p.kvy * dt;
                        if p.kvy > p.fall * sc {
                            p.st = St::Fall;
                            p.vx = p.kvx;
                            p.rot_speed = p.rot_speed.clamp(-self.cfg.rot_speed, self.cfg.rot_speed);
                        }
                    } else {
                        p.vx += (wind * p.resp * sc - p.vx) * k;
                        let sway = (t * p.freq + p.phase).sin() * p.amp * sc;
                        // Edge-on petals slip through the air faster than broad-side ones.
                        let drag = 0.75 + 0.25 * (1.0 - p.flip.cos().abs());
                        p.x += (p.vx + sway) * dt * sp;
                        dy = p.fall * sc * drag * sp * dt;
                    }
                    p.y += dy;
                    if p.flip > 62.8 {
                        p.flip -= 62.8319;
                    }
                    p.rot = p.rot.rem_euclid(6.2831855);
                    p.x = wrap(p.x, -m, w + m);

                    let mut landed = None;
                    if dy > 0.0 {
                        let foot = p.size * sc * FOOT;
                        if let Some((id, ly)) = find_land(scene, p.x, prev_foot, p.y + foot) {
                            let persist = id.is_none() && self.cfg.ground_stays;
                            if persist || n_timed < REST_CAP {
                                landed = Some((id, ly, persist));
                            }
                        }
                    }
                    if let Some((id, ly, persist)) = landed {
                        self.land(i, id, ly, sc);
                        if persist {
                            n_pile += 1;
                        } else {
                            n_timed += 1;
                        }
                    } else if self.items[i].y > h + m {
                        self.respawn(i);
                    }
                }
                St::Rest | St::Fade => {
                    let p = &mut self.items[i];
                    let was_timed = !p.persistent();
                    let held = match p.surf {
                        Some(id) => scene.surfaces.iter().any(|s| {
                            s.id == id && p.x >= s.x0 && p.x <= s.x1 && (s.y - p.surf_y).abs() <= LEDGE_TOL
                        }),
                        None => match scene.ground {
                            Some(g) => {
                                p.y = g - p.rest_off;
                                p.surf_y = g;
                                true
                            }
                            None => false,
                        },
                    };
                    if !held {
                        if was_timed {
                            n_timed = n_timed.saturating_sub(1);
                        } else {
                            n_pile = n_pile.saturating_sub(1);
                        }
                        if p.st == St::Fade {
                            self.respawn(i);
                        } else {
                            p.st = St::Fall;
                            p.surf = None;
                            p.vx = 0.0;
                        }
                        continue;
                    }
                    if p.st == St::Fade {
                        p.fade_t += dt;
                        if p.fade_t >= FADE_TIME {
                            n_timed = n_timed.saturating_sub(1);
                            self.respawn(i);
                        }
                        continue;
                    }
                    p.t_rest += dt;
                    // Strong wind occasionally lifts timed items off.
                    if was_timed && wind.abs() > 0.6 && self.rng.f32() < 0.3 * dt * wind.abs() {
                        p.st = St::Fall;
                        p.surf = None;
                        p.vx = wind * p.resp * sc * 0.5;
                        p.y -= 2.0;
                        n_timed = n_timed.saturating_sub(1);
                        continue;
                    }
                    if let Some(pt) = kick {
                        let (dx, dy) = (p.x - pt.pos[0], p.y - pt.pos[1]);
                        let d2 = dx * dx + dy * dy;
                        if d2 < kick_r2 {
                            let rng = &mut self.rng;
                            let e = kick_energy(pt.vel[0].hypot(pt.vel[1]) / sc, d2.sqrt() / kick_r)
                                * range(rng, 0.75, 1.15);
                            if e < KICK_HOP {
                                // Light touch: the leaf slides and turns a little, stays down.
                                let push = pt.vel[0].signum() * e / KICK_HOP * range(rng, 2.0, 7.0) * sc;
                                p.x = (p.x + push).clamp(0.0, w);
                                p.rest_rot += push * 0.04;
                                continue;
                            }
                            p.st = St::Fly;
                            p.surf = None;
                            p.kvx = pt.vel[0] * (0.12 + 0.33 * e) + range(rng, -40.0, 40.0) * sc * e;
                            p.kvy = -(60.0 + 620.0 * e) * sc * range(rng, 0.8, 1.15)
                                + (pt.vel[1] * 0.15 * e).min(0.0);
                            p.rot_speed =
                                if rng.bool() { 1.0 } else { -1.0 } * (2.0 + 9.0 * e) * range(rng, 0.7, 1.2);
                            p.rot = p.rest_rot;
                            p.flip = p.rest_flip;
                            p.y -= 2.0;
                            if was_timed {
                                n_timed = n_timed.saturating_sub(1);
                            } else {
                                n_pile = n_pile.saturating_sub(1);
                            }
                            continue;
                        }
                    }
                    if p.t_rest >= p.life {
                        p.st = St::Fade;
                        p.fade_t = 0.0;
                    }
                }
            }
        }

        // Over the pile cap: the oldest leaves fade away.
        while n_pile > pile_cap {
            let oldest = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, p)| p.pile_item())
                .min_by_key(|(_, p)| p.seq)
                .map(|(i, _)| i);
            let Some(i) = oldest else { break };
            let p = &mut self.items[i];
            p.st = St::Fade;
            p.life = 0.0; // no longer a pile item once fading
            p.fade_t = 0.0;
            n_pile -= 1;
        }
        self.build();
    }

    fn clear_region(&mut self, x0: f32, x1: f32) {
        if !self.cfg.kick {
            return;
        }
        let sc = ui_scale(self.w, self.h);
        let mid = (x0 + x1) * 0.5;
        for p in &mut self.items {
            if p.st != St::Rest || p.surf.is_some() || p.x < x0 || p.x > x1 {
                continue;
            }
            let rng = &mut self.rng;
            let dir = if p.x >= mid { 1.0 } else { -1.0 };
            p.st = St::Fly;
            p.kvx = dir * range(rng, 80.0, 280.0) * sc + range(rng, -40.0, 40.0) * sc;
            p.kvy = -range(rng, 260.0, 520.0) * sc;
            p.rot_speed = if rng.bool() { 1.0 } else { -1.0 } * range(rng, 3.0, 9.0);
            p.rot = p.rest_rot;
            p.flip = p.rest_flip;
            p.y -= 2.0;
        }
        self.build();
    }

    fn clear_accumulation(&mut self) {
        for i in 0..self.items.len() {
            if self.items[i].resting() {
                self.respawn(i);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use snowy_core::{Pointer, Surface};

    const W: f32 = 1920.0;
    const H: f32 = 1080.0;

    fn env(wind: f32) -> Env {
        Env { wind, intensity: 1.0, speed: 1.0, time: 0.0 }
    }

    fn run(p: &mut Petals, secs: f32, wind: f32, scene: &Scene) {
        let n = (secs * 60.0) as usize;
        for _ in 0..n {
            p.update(1.0 / 60.0, &env(wind), scene);
        }
    }

    fn ledge(y: f32) -> [Surface; 1] {
        [Surface { id: 7, x0: 0.0, x1: W, y }]
    }

    fn scene<'a>(s: &'a [Surface], ground: Option<f32>, pointer: Option<Pointer>) -> Scene<'a> {
        Scene { surfaces: s, ground, pointer, snowballs: false }
    }

    fn resting_on(p: &Petals, id: u64) -> usize {
        p.items.iter().filter(|i| i.resting() && i.surf == Some(id)).count()
    }

    fn sane(p: &Petals) {
        for i in p.instances() {
            assert!(i.pos.iter().all(|v| v.is_finite()) && i.rot.is_finite());
            assert!(i.size.is_finite() && i.size > 0.0);
            assert!(i.color.iter().all(|c| (0.0..=1.0).contains(c)), "{:?}", i.color);
            assert!(i.param.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn sakura_rests_then_leaves() {
        let mut p = Petals::new(W, H, SAKURA);
        let s = ledge(500.0);
        let sc = scene(&s, None, None);
        run(&mut p, 12.0, 0.0, &sc);
        assert!(resting_on(&p, 7) > 3);
        let (i, seq) = p.items.iter().enumerate().find(|(_, x)| x.st == St::Rest).map(|(i, x)| (i, x.seq)).unwrap();
        assert!(p.items[i].y < 500.0 && p.items[i].y > 480.0);
        run(&mut p, 11.5, 0.0, &sc);
        assert!(p.items[i].st != St::Rest || p.items[i].seq != seq);
        sane(&p);
    }

    #[test]
    fn removing_or_moving_surface_detaches() {
        let mut p = Petals::new(W, H, SAKURA);
        let s = ledge(500.0);
        run(&mut p, 12.0, 0.0, &scene(&s, None, None));
        assert!(resting_on(&p, 7) > 0);
        p.update(1.0 / 60.0, &env(0.0), &Scene::EMPTY);
        assert_eq!(resting_on(&p, 7), 0);

        let mut p = Petals::new(W, H, SAKURA);
        run(&mut p, 12.0, 0.0, &scene(&s, None, None));
        assert!(resting_on(&p, 7) > 0);
        let moved = ledge(550.0);
        p.update(1.0 / 60.0, &env(0.0), &scene(&moved, None, None));
        assert_eq!(p.items.iter().filter(|i| i.resting() && i.surf_y == 500.0).count(), 0);
    }

    #[test]
    fn leaves_pile_on_ground_and_stay() {
        let mut p = Petals::new(W, H, LEAVES);
        let sc = scene(&[], Some(H), None);
        run(&mut p, 40.0, 0.0, &sc);
        let n = p.items.iter().filter(|i| i.pile_item()).count();
        assert!(n >= 60, "{n}");
        run(&mut p, 30.0, 0.0, &sc);
        assert!(p.items.iter().filter(|i| i.pile_item()).count() >= n);
        // Active items keep falling.
        assert!(p.items.iter().any(|i| !i.pile_item()));
        sane(&p);
        p.clear_accumulation();
        assert_eq!(p.items.iter().filter(|i| i.resting()).count(), 0);
        sane(&p);
    }

    #[test]
    fn pointer_kicks_pile_only_when_fast() {
        let mut p = Petals::new(W, H, LEAVES);
        run(&mut p, 40.0, 0.0, &scene(&[], Some(H), None));
        let leaf = p.items.iter().find(|i| i.pile_item()).unwrap();
        let pos = [leaf.x, leaf.y];
        let ptr = |v: f32| Pointer { pos, vel: [v, 0.0], down: false, over_desktop: true };
        let slow = ptr(100.0);
        p.update(1.0 / 60.0, &env(0.0), &scene(&[], Some(H), Some(slow)));
        assert!(!p.items.iter().any(|i| i.st == St::Fly));
        let fast = ptr(1500.0);
        p.update(1.0 / 60.0, &env(0.0), &scene(&[], Some(H), Some(fast)));
        let fly: Vec<_> = p.items.iter().filter(|i| i.st == St::Fly).collect();
        assert!(!fly.is_empty());
        assert!(fly.iter().all(|i| i.kvy < 0.0 && i.kvx > 0.0));
        // They come back down and land again.
        run(&mut p, 6.0, 0.0, &scene(&[], Some(H), None));
        assert!(!p.items.iter().any(|i| i.st == St::Fly));
        sane(&p);
    }

    #[test]
    fn kick_energy_ramps_with_speed_and_falls_with_distance() {
        assert_eq!(kick_energy(KICK_MIN_SPEED, 0.0), 0.0);
        assert!((kick_energy(KICK_FULL_SPEED, 0.0) - 1.0).abs() < 1e-6);
        assert_eq!(kick_energy(KICK_FULL_SPEED, 1.0), 0.0);
        let mut last = 0.0;
        for v in (300..=2600).step_by(100) {
            let e = kick_energy(v as f32, 0.2);
            assert!(e >= last, "not monotonic at {v}");
            last = e;
        }
        assert!(kick_energy(1500.0, 0.2) > kick_energy(1500.0, 0.6));
    }

    /// Settles a pile and returns (effect, position of a pile leaf).
    fn settled_pile() -> (Petals, [f32; 2]) {
        let mut p = Petals::new(W, H, LEAVES);
        run(&mut p, 40.0, 0.0, &scene(&[], Some(H), None));
        let leaf = p.items.iter().find(|i| i.pile_item()).unwrap();
        let pos = [leaf.x, leaf.y];
        (p, pos)
    }

    #[test]
    fn leaves_clear_region_launches_ground_leaves_in_range_only() {
        let (mut p, _) = settled_pile();
        let (x0, x1) = (600.0, 900.0);
        let inside = |p: &Petals| p.items.iter().filter(|i| i.pile_item() && i.surf.is_none() && i.x >= x0 && i.x <= x1).count();
        assert!(inside(&p) > 0);
        let outside = p.items.iter().filter(|i| i.pile_item() && (i.x < x0 || i.x > x1)).count();
        p.clear_region(x0, x1);
        assert_eq!(inside(&p), 0);
        assert!(p.items.iter().any(|i| i.st == St::Fly));
        assert!(p.items.iter().filter(|i| i.st == St::Fly).all(|i| i.x >= x0 && i.x <= x1));
        assert_eq!(p.items.iter().filter(|i| i.pile_item() && (i.x < x0 || i.x > x1)).count(), outside);
        sane(&p);
    }

    #[test]
    fn gentle_touch_nudges_without_launching() {
        let (mut p, pos) = settled_pile();
        let before: Vec<f32> = p.items.iter().map(|i| i.x).collect();
        let ptr = Pointer { pos, vel: [450.0, 0.0], down: false, over_desktop: true };
        p.update(1.0 / 60.0, &env(0.0), &scene(&[], Some(H), Some(ptr)));
        assert!(!p.items.iter().any(|i| i.st == St::Fly), "a gentle touch must not launch leaves");
        let moved = p.items.iter().zip(&before).filter(|(i, b)| i.pile_item() && (i.x - **b).abs() > 0.5).count();
        assert!(moved > 0, "nearby leaves should shuffle a little");
        sane(&p);
    }

    #[test]
    fn harder_sweeps_send_leaves_higher() {
        let peak = |speed: f32| {
            let (mut p, pos) = settled_pile();
            let ptr = Pointer { pos, vel: [speed, 0.0], down: false, over_desktop: true };
            p.update(1.0 / 60.0, &env(0.0), &scene(&[], Some(H), Some(ptr)));
            p.items.iter().filter(|i| i.st == St::Fly).map(|i| -i.kvy).fold(0.0f32, f32::max)
        };
        let medium = peak(1100.0);
        let hard = peak(2600.0);
        assert!(medium > 0.0, "a medium sweep should lift some leaves");
        assert!(hard > medium * 1.5, "medium {medium} hard {hard}");
        // A medium sweep is a hop, not a rocket: initial upward speed well under 500 px/s.
        assert!(medium < 500.0, "medium {medium}");
    }
}
