//! Snowballs: pick snow off a pile by dragging, throw, bounce, rest or shatter into chunks.

use crate::pile::{Piles, GROUND_ID};
use crate::util::*;
use fastrand::Rng;
use snowy_core::{Instance, Scene, WorldEvent};

const MAX_BALLS: usize = 12;
const MAX_CHUNKS: usize = 400;
const BREAK_SPEED: f32 = 700.0;
const MAX_THROW: f32 = 3000.0;
/// Seconds a two-ball stack waits for a third before becoming a snowman.
const STACK_WAIT: f32 = 1.5;
/// A ball may sit on one at most this much smaller than itself.
const STACK_RATIO: f32 = 1.05;
/// How far a resting ball sinks into the snow, as a fraction of its radius.
const SINK: f32 = 0.15;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BState {
    Held,
    Free,
    /// Resting on the pile with this surface id, whose top edge was at `y`.
    Rest { id: u64, y: f32 },
    /// Sitting on top of the ball with this `Ball::id` (ground stacks only).
    Stacked { on: u32 },
}

#[derive(Clone, Copy)]
pub struct Ball {
    /// Unique among live balls; what `BState::Stacked` refers to.
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub r: f32,
    pub rot: f32,
    pub state: BState,
    /// Picked up again from rest: keeps its size instead of growing on snow.
    pub fixed: bool,
    /// Just released by the user this frame (stack-snap ignores throw speed).
    pub rel: bool,
    /// Seconds a two-stack has waited (kept on the bottom ball).
    pub timer: f32,
}

struct Chunk {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    life: f32,
    seed: f32,
    landed: bool,
}

#[derive(Default)]
pub struct Snowballs {
    pub balls: Vec<Ball>,
    chunks: Vec<Chunk>,
    last_vel: [f32; 2],
    next_id: u32,
    prev_down: bool,
    events: Vec<WorldEvent>,
}

fn finite2(v: [f32; 2]) -> bool {
    v[0].is_finite() && v[1].is_finite()
}

impl Snowballs {
    #[cfg(test)]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn clear(&mut self) {
        self.balls.clear();
        self.chunks.clear();
    }

    pub fn take_events(&mut self, out: &mut Vec<WorldEvent>) {
        out.append(&mut self.events);
    }

    /// Bounding squares (padded) of balls the user can pick up again.
    pub fn grab_zones(&self, out: &mut Vec<[f32; 4]>) {
        for b in &self.balls {
            if matches!(b.state, BState::Rest { .. } | BState::Stacked { .. }) {
                let m = b.r + 6.0;
                out.push([b.x - m, b.y - m, b.x + m, b.y + m]);
            }
        }
    }

    /// Remove resting and flying balls whose centre is in `x0..x1`, with a puff of chunks.
    pub fn clear_region(&mut self, x0: f32, x1: f32, sc: f32, rng: &mut Rng) {
        let mut i = 0;
        while i < self.balls.len() {
            let b = self.balls[i];
            if b.state != BState::Held && b.x >= x0 && b.x <= x1 {
                break_ball(&b, b.x, b.y + b.r, sc, &mut self.chunks, rng);
                self.balls.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Plough spray: `n` chunks thrown up and sideways from the ground line `y`.
    pub fn spray(&mut self, x0: f32, x1: f32, y: f32, n: usize, sc: f32, rng: &mut Rng) {
        let dir = if rng.bool() { 1.0 } else { -1.0 };
        for _ in 0..n {
            if self.chunks.len() >= MAX_CHUNKS {
                break;
            }
            self.chunks.push(Chunk {
                x: range(rng, x0.min(x1), x1.max(x0)),
                y: y - 2.0 * sc,
                vx: dir * range(rng, 60.0, 280.0) * sc,
                vy: -range(rng, 160.0, 460.0) * sc,
                size: range(rng, 1.8, 3.8) * sc,
                life: range(rng, 0.6, 1.1),
                seed: range(rng, 0.0, 6.0),
                landed: false,
            });
        }
    }

    /// A falling flake at (x, y) that went into a ball is put back on its surface and nudged
    /// sideways, so flakes slide over balls and drop beside them instead of passing through.
    /// Returns true when the flake touched a ball.
    pub fn deflect(&self, x: &mut f32, y: &mut f32, vx: &mut f32, sc: f32) -> bool {
        for b in &self.balls {
            let rr = b.r + 0.5 * sc;
            let (dx, dy) = (*x - b.x, *y - b.y);
            let d2 = dx * dx + dy * dy;
            if d2 >= rr * rr {
                continue;
            }
            let d = d2.sqrt();
            // Straight on top: pick a side from the position so flakes split both ways.
            let side = if dx.abs() > 0.05 * sc { dx.signum() } else if (*y * 7.31).fract() < 0.5 { -1.0 } else { 1.0 };
            let (nx, ny) = if d > 1e-3 { (dx / d, dy / d) } else { (side, 0.0) };
            *x = b.x + nx * rr;
            *y = b.y + ny * rr;
            // Slide down the slope: faster on the steep sides than on top.
            let slide = (25.0 + 60.0 * nx.abs()) * sc;
            *vx = side * vx.abs().max(slide);
            return true;
        }
        false
    }

    /// A snowball thrown by a friend: falls in from above the top edge at `x`, slightly
    /// sideways, and lands, rolls or bursts like any other.
    pub fn incoming(&mut self, x: f32, sc: f32, rng: &mut Rng) {
        if self.balls.len() >= MAX_BALLS {
            let idx = self.balls.iter().position(|b| matches!(b.state, BState::Rest { .. })).or_else(|| self.balls.iter().position(|b| b.state == BState::Free));
            match idx {
                Some(i) => {
                    self.balls.remove(i);
                }
                None => return,
            }
        }
        let r = range(rng, 10.0, 16.0) * sc;
        self.next_id = self.next_id.wrapping_add(1);
        self.balls.push(Ball {
            id: self.next_id,
            x,
            y: -r * 2.0,
            vx: range(rng, -120.0, 120.0) * sc,
            vy: range(rng, 150.0, 300.0) * sc,
            r,
            rot: 0.0,
            state: BState::Free,
            fixed: true,
            rel: false,
            timer: 0.0,
        });
    }

    fn find(&self, id: u32) -> Option<usize> {
        self.balls.iter().position(|b| b.id == id)
    }

    fn carries(&self, id: u32) -> Option<usize> {
        self.balls.iter().position(|b| b.state == BState::Stacked { on: id })
    }

    fn is_ground_rest(b: &Ball) -> bool {
        matches!(b.state, BState::Rest { id, .. } if id == GROUND_ID)
    }

    /// Stacking: follow bases, snap landing balls onto resting ground balls, finish snowmen.
    fn stack_pass(&mut self, dt: f32, sc: f32, rng: &mut Rng) {
        // Stacked balls follow their base; lose it and they fall.
        for _ in 0..2 {
            for i in 0..self.balls.len() {
                if let BState::Stacked { on } = self.balls[i].state {
                    let base = self.find(on).map(|j| self.balls[j]).filter(|q| {
                        Self::is_ground_rest(q) || matches!(q.state, BState::Stacked { .. })
                    });
                    let b = &mut self.balls[i];
                    match base {
                        Some(q) => {
                            b.x = q.x;
                            b.y = q.y - q.r - b.r;
                            b.vx = 0.0;
                            b.vy = 0.0;
                        }
                        None => {
                            b.state = BState::Free;
                            b.vx = 0.0;
                            b.vy = 0.0;
                        }
                    }
                }
            }
        }
        // Free balls coming down onto a resting ball.
        for i in 0..self.balls.len() {
            let b = self.balls[i];
            if b.state != BState::Free {
                continue;
            }
            self.balls[i].rel = false;
            if b.vy < 0.0 && !b.rel {
                continue;
            }
            let slow = b.vx.hypot(b.vy) < BREAK_SPEED * sc || b.rel;
            let target = (0..self.balls.len()).find(|&j| {
                let l = self.balls[j];
                if j == i || (b.x - l.x).abs() >= l.r * 0.5 || b.y + b.r < l.y - l.r || b.y >= l.y {
                    return false;
                }
                let can_carry = Self::is_ground_rest(&l)
                    || matches!(l.state, BState::Stacked { on }
                        if self.find(on).is_some_and(|k| Self::is_ground_rest(&self.balls[k])));
                can_carry && self.carries(l.id).is_none()
            });
            let Some(j) = target else { continue };
            let l = self.balls[j];
            if b.r > l.r * STACK_RATIO {
                // Too big to balance: roll off to the side.
                let dir = if b.x >= l.x { 1.0 } else { -1.0 };
                self.balls[i].vx = dir * b.vx.abs().max(160.0 * sc);
            } else if slow {
                let nb = &mut self.balls[i];
                nb.state = BState::Stacked { on: l.id };
                nb.x = l.x;
                nb.y = l.y - l.r - nb.r;
                nb.vx = 0.0;
                nb.vy = 0.0;
            }
        }
        // Finished stacks become snowmen.
        'again: loop {
            for i in 0..self.balls.len() {
                let a = self.balls[i];
                if !Self::is_ground_rest(&a) {
                    continue;
                }
                let Some(bi) = self.carries(a.id) else {
                    self.balls[i].timer = 0.0;
                    continue;
                };
                let b = self.balls[bi];
                let ci = self.carries(b.id);
                if ci.is_none() {
                    self.balls[i].timer += dt;
                    if self.balls[i].timer < STACK_WAIT {
                        continue;
                    }
                }
                let mut sum = 2.0 * (a.r + b.r);
                let mut n = 2u8;
                let mut ids = [a.id, b.id, u32::MAX];
                if let Some(ci) = ci {
                    sum += 2.0 * self.balls[ci].r;
                    ids[2] = self.balls[ci].id;
                    n = 3;
                }
                self.events.push(WorldEvent::SnowmanBuilt { x: a.x, balls: n, height: sum * 0.92 });
                break_ball(&a, a.x, a.y + a.r, sc, &mut self.chunks, rng);
                self.balls.retain(|q| !ids.contains(&q.id));
                continue 'again;
            }
            break;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, scene: &Scene, piles: &mut Piles, sc: f32, w: f32, h: f32, rng: &mut Rng) {
        let ptr = scene.pointer.filter(|p| finite2(p.pos) && finite2(p.vel));
        if let Some(p) = ptr {
            self.last_vel = p.vel;
        }
        let down = ptr.is_some_and(|p| p.down);
        let edge = down && !self.prev_down;
        self.prev_down = down;
        // Pick a resting ball up again.
        let mut grabbed = false;
        if scene.snowballs && edge && !self.balls.iter().any(|b| b.state == BState::Held) {
            if let Some(p) = ptr.filter(|p| p.over_desktop) {
                let dist = |k: usize| (self.balls[k].x - p.pos[0]).hypot(self.balls[k].y - p.pos[1]);
                let best = (0..self.balls.len())
                    .filter(|&i| {
                        let b = &self.balls[i];
                        matches!(b.state, BState::Rest { .. } | BState::Stacked { .. })
                            && self.carries(b.id).is_none()
                            && dist(i) <= b.r
                    })
                    .min_by(|&i, &j| dist(i).total_cmp(&dist(j)));
                if let Some(i) = best {
                    let b = &mut self.balls[i];
                    b.state = BState::Held;
                    b.fixed = true;
                    b.vx = 0.0;
                    b.vy = 0.0;
                    b.timer = 0.0;
                    grabbed = true;
                }
            }
        }
        // Start a ball.
        if scene.snowballs && !grabbed && !self.balls.iter().any(|b| b.state == BState::Held) {
            if let Some(p) = ptr {
                if p.down && p.over_desktop && piles.near_snow(p.pos[0], p.pos[1], 40.0 * sc, 3.0 * sc) {
                    if self.balls.len() >= MAX_BALLS {
                        let idx = self
                            .balls
                            .iter()
                            .position(|b| matches!(b.state, BState::Rest { .. }))
                            .or_else(|| self.balls.iter().position(|b| b.state == BState::Free));
                        if let Some(i) = idx {
                            self.balls.remove(i);
                        }
                    }
                    if self.balls.len() < MAX_BALLS {
                        self.next_id = self.next_id.wrapping_add(1);
                        self.balls.push(Ball {
                            id: self.next_id,
                            x: p.pos[0],
                            y: p.pos[1],
                            vx: 0.0,
                            vy: 0.0,
                            r: 6.0 * sc,
                            rot: 0.0,
                            state: BState::Held,
                            fixed: false,
                            rel: false,
                            timer: 0.0,
                        });
                    }
                }
            }
        }
        let mut i = 0;
        while i < self.balls.len() {
            let mut b = self.balls[i];
            let alive = step_ball(&mut b, dt, ptr, self.last_vel, piles, sc, w, h, &mut self.chunks, rng);
            if alive {
                self.balls[i] = b;
                i += 1;
            } else {
                self.balls.remove(i);
            }
        }
        self.stack_pass(dt, sc, rng);
        // Resting balls press into the snow: nothing piles up inside them.
        for b in &self.balls {
            if let BState::Rest { id, y } = b.state {
                piles.press(id, y, b.x, b.y, b.r, b.r * SINK);
            }
        }
        // Chunks.
        let mut i = 0;
        while i < self.chunks.len() {
            let c = &mut self.chunks[i];
            c.life -= dt;
            if !c.landed {
                let y0 = c.y;
                c.vy += 1800.0 * sc * dt;
                c.x += c.vx * dt;
                c.y += c.vy * dt;
                if piles.absorb(c.x, y0, c.y, 0.3 * sc) {
                    c.landed = true;
                    c.vx = 0.0;
                    c.vy = 0.0;
                }
            }
            if c.life <= 0.0 || c.y > h + 60.0 {
                self.chunks.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    pub fn build(&self, out: &mut Vec<Instance>) {
        for b in &self.balls {
            out.push(Instance {
                pos: [b.x, b.y],
                size: b.r.max(1.0),
                rot: b.rot,
                color: [1.0, 1.0, 1.0, 1.0],
                param: [0.0, 0.0, 0.0, 2.0],
            });
        }
        for c in &self.chunks {
            out.push(Instance {
                pos: [c.x, c.y],
                size: c.size.max(0.5),
                rot: 0.0,
                color: [0.94, 0.97, 1.0, c.life.clamp(0.0, 1.0)],
                param: [c.seed, 0.0, 0.0, 3.0],
            });
        }
    }
}

fn break_ball(b: &Ball, x: f32, y: f32, sc: f32, chunks: &mut Vec<Chunk>, rng: &mut Rng) {
    let n = 10 + rng.usize(0..11);
    for _ in 0..n {
        if chunks.len() >= MAX_CHUNKS {
            break;
        }
        let a = range(rng, -std::f32::consts::PI * 0.95, -std::f32::consts::PI * 0.05);
        let sp = range(rng, 120.0, 420.0) * sc;
        chunks.push(Chunk {
            x,
            y: y - 1.0,
            vx: a.cos() * sp + b.vx * 0.15,
            vy: a.sin() * sp,
            size: (b.r * range(rng, 0.2, 0.4)).max(1.5 * sc),
            life: range(rng, 0.8, 1.2),
            seed: range(rng, 0.0, 6.0),
            landed: false,
        });
    }
}

/// Advance one ball. Returns false when it should be removed.
#[allow(clippy::too_many_arguments)]
fn step_ball(
    b: &mut Ball,
    dt: f32,
    ptr: Option<snowy_core::Pointer>,
    last_vel: [f32; 2],
    piles: &mut Piles,
    sc: f32,
    w: f32,
    h: f32,
    chunks: &mut Vec<Chunk>,
    rng: &mut Rng,
) -> bool {
    if matches!(b.state, BState::Stacked { .. }) {
        return true;
    }
    if b.state == BState::Held {
        if let Some(p) = ptr.filter(|p| p.down) {
            let k = (dt * 20.0).min(1.0);
            let (ox, oy) = (b.x, b.y);
            b.x += (p.pos[0] - b.x) * k;
            b.y += (p.pos[1] - b.y) * k;
            let dx = b.x - ox;
            b.rot += dx / b.r;
            if !b.fixed && (dx != 0.0 || oy != b.y) {
                let touching = piles.piles.iter().position(|q| {
                    b.x >= q.x0 && b.x <= q.x1 && b.y <= q.y + b.r * 0.5 && b.y + b.r >= q.top_at(b.x) - 3.0 * sc
                });
                if let Some(pi) = touching {
                    let adx = dx.abs();
                    if adx > 0.0 && piles.piles[pi].height_at(b.x) > 0.0 {
                        b.r = (b.r + 0.05 * adx).min(40.0 * sc);
                        let (xa, xb) = (b.x - b.r * 0.7, b.x + b.r * 0.7);
                        piles.piles[pi].remove(xa, xb, adx * 0.25);
                    }
                }
            }
            return true;
        }
        // Released: throw with the cursor velocity.
        let mut v = last_vel;
        let m = (v[0] * v[0] + v[1] * v[1]).sqrt();
        if m > MAX_THROW {
            v = [v[0] / m * MAX_THROW, v[1] / m * MAX_THROW];
        }
        b.vx = v[0];
        b.vy = v[1];
        b.state = BState::Free;
        b.rel = true;
    }

    if let BState::Rest { id, y } = b.state {
        let seg = piles
            .piles
            .iter()
            .find(|q| q.id == id && (q.y - y).abs() <= 2.0 && b.x >= q.x0 && b.x <= q.x1);
        match seg {
            Some(q) => {
                b.vx *= (-2.0 * dt).exp();
                if b.vx.abs() < 4.0 {
                    b.vx = 0.0;
                }
                b.x += b.vx * dt;
                b.rot += b.vx * dt / b.r;
                b.state = BState::Rest { id, y: q.y };
                if b.x < q.x0 || b.x > q.x1 {
                    b.state = BState::Free;
                } else {
                    b.y = q.top_at(b.x) - b.r * (1.0 - SINK);
                    return true;
                }
            }
            None => b.state = BState::Free,
        }
    }

    // Free flight.
    b.vy += 1800.0 * sc * dt;
    let drag = 1.0 - 0.2 * dt;
    b.vx *= drag;
    b.vy *= drag;
    let nx = b.x + b.vx * dt;
    let ny = b.y + b.vy * dt;
    let mut best: Option<(usize, f32)> = None;
    if b.vy > 0.0 {
        for (i, q) in piles.piles.iter().enumerate() {
            if nx < q.x0 || nx > q.x1 || b.y > q.y {
                continue;
            }
            let top = q.top_at(nx);
            if ny + b.r >= top && best.is_none_or(|(_, t)| top < t) {
                best = Some((i, top));
            }
        }
    }
    b.x = nx;
    b.y = ny;
    b.rot += b.vx * dt / b.r;
    if let Some((i, top)) = best {
        let impact = b.vy;
        b.y = top - b.r;
        if impact > BREAK_SPEED * sc {
            break_ball(b, b.x, top, sc, chunks, rng);
            return false;
        }
        b.vy = -impact * 0.3;
        b.vx *= 0.8;
        if b.vy.abs() < 90.0 * sc {
            b.vy = 0.0;
            b.state = BState::Rest { id: piles.piles[i].id, y: piles.piles[i].y };
        }
    }
    if b.x < b.r {
        b.x = b.r;
        b.vx = b.vx.abs() * 0.5;
    } else if b.x > w - b.r {
        b.x = w - b.r;
        b.vx = -b.vx.abs() * 0.5;
    }
    b.y < h + 200.0 && b.y.is_finite() && b.x.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use snowy_core::Pointer;

    fn scene_with<'a>(p: Option<Pointer>, ground: Option<f32>) -> Scene<'a> {
        Scene { surfaces: &[], ground, pointer: p, snowballs: true }
    }

    fn ptr(x: f32, y: f32, down: bool) -> Pointer {
        Pointer { pos: [x, y], vel: [0.0, 0.0], down, over_desktop: true }
    }

    #[test]
    fn held_ball_picks_snow_and_follows() {
        let mut rng = Rng::with_seed(1);
        let mut piles = Piles::default();
        let mut sb = Snowballs::default();
        let sc = 1.0;
        piles.update(0.016, 0.0, &[], Some(1000.0), 1920.0, 1080.0, &mut rng);
        piles.piles[0].cols.fill(8.0);
        let before: f32 = piles.piles[0].cols.iter().sum();
        for k in 0..100 {
            let p = ptr(200.0 + k as f32 * 10.0, 990.0, true);
            sb.update(0.016, &scene_with(Some(p), Some(1000.0)), &mut piles, sc, 1920.0, 1080.0, &mut rng);
        }
        assert_eq!(sb.balls.len(), 1);
        assert!(sb.balls[0].r > 12.0);
        let after: f32 = piles.piles[0].cols.iter().sum();
        assert!(after < before);
    }

    #[test]
    fn flakes_slide_off_a_ball_and_land_beside_it() {
        let mut sb = Snowballs::default();
        let r = 30.0;
        sb.balls.push(Ball {
            id: 1,
            x: 500.0,
            y: 900.0,
            vx: 0.0,
            vy: 0.0,
            r,
            rot: 0.0,
            state: BState::Rest { id: GROUND_ID, y: 1000.0 },
            fixed: true,
            rel: false,
            timer: 0.0,
        });
        // Flakes falling straight onto the ball, across its whole width.
        for k in 0..21 {
            let (mut x, mut y, mut vx) = (500.0 - r + k as f32 * 3.0, 820.0, 0.0f32);
            let mut touched = false;
            for _ in 0..2000 {
                x += vx * 0.016;
                y += 60.0 * 0.016;
                touched |= sb.deflect(&mut x, &mut y, &mut vx, 1.0);
                let (dx, dy) = (x - 500.0, y - 900.0);
                assert!(dx * dx + dy * dy >= (r - 0.01) * (r - 0.01), "flake {k} inside the ball at ({x}, {y})");
                if y > 940.0 {
                    break;
                }
            }
            assert!(touched, "flake {k} missed");
            assert!(y > 940.0, "flake {k} stuck on the ball at ({x}, {y})");
            assert!((x - 500.0).abs() >= r - 1.0, "flake {k} came down under the ball: {x}");
        }
    }
}
