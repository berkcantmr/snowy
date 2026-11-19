//! Snow and HeavySnow: soft flakes in three depth layers.

use crate::pile::Piles;
use crate::snowball::Snowballs;
use crate::util::*;
use fastrand::Rng;
use snowy_core::{Effect, Env, Instance, Scene, WorldEvent};

const SHADER: &str = include_str!("shaders/snow.hlsl");

struct Flake {
    x: f32,
    y: f32,
    vx: f32,
    fall: f32,
    resp: f32,
    size: f32,
    alpha: f32,
    depth: f32,
    phase: f32,
    freq: f32,
    amp: f32,
}

pub struct Snow {
    w: f32,
    h: f32,
    heavy: bool,
    t: f32,
    rng: Rng,
    flakes: Vec<Flake>,
    inst: Vec<Instance>,
    piles: Piles,
    balls: Snowballs,
    /// Last `scene.snowballs`; grab zones exist only when snowball play is allowed.
    snowballs_on: bool,
}

fn spawn(rng: &mut Rng, heavy: bool, x: f32, y: f32) -> Flake {
    let layer = {
        let r = rng.f32();
        if r < 0.45 {
            0.0
        } else if r < 0.8 {
            0.5
        } else {
            1.0
        }
    };
    let depth = (layer + range(rng, -0.08, 0.08)).clamp(0.0, 1.0);
    let (size_k, fall_k, wind_k) = if heavy { (1.25, 1.35, 1.8) } else { (1.0, 1.0, 1.0) };
    Flake {
        x,
        y,
        vx: 0.0,
        fall: lerp(35.0, 95.0, depth) * fall_k * range(rng, 0.85, 1.15),
        resp: lerp(30.0, 110.0, depth) * wind_k,
        size: lerp(2.2, 5.5, depth) * size_k * range(rng, 0.85, 1.15),
        alpha: lerp(0.4, 0.95, depth),
        depth,
        phase: range(rng, 0.0, std::f32::consts::TAU),
        freq: range(rng, 0.6, 1.6),
        amp: range(rng, 8.0, 26.0),
    }
}

impl Snow {
    pub fn new(w: f32, h: f32, heavy: bool) -> Self {
        let mut s = Snow {
            w,
            h,
            heavy,
            t: 0.0,
            rng: Rng::with_seed(if heavy { 0x5a0f } else { 0x5a0e }),
            flakes: Vec::new(),
            inst: Vec::new(),
            piles: Piles::default(),
            balls: Snowballs::default(),
            snowballs_on: false,
        };
        s.adjust(1.0);
        s.build();
        s
    }

    fn adjust(&mut self, intensity: f32) {
        let base = if self.heavy { 1100.0 } else { 350.0 };
        let n = target_count(base, self.w, self.h, intensity);
        self.flakes.truncate(n);
        if self.flakes.len() < n {
            self.flakes.reserve(n - self.flakes.len());
            self.inst.reserve(n);
        }
        while self.flakes.len() < n {
            let (x, y) = (self.rng.f32() * self.w, self.rng.f32() * self.h);
            let f = spawn(&mut self.rng, self.heavy, x, y);
            self.flakes.push(f);
        }
    }

    fn build(&mut self) {
        let sc = ui_scale(self.w, self.h);
        self.inst.clear();
        self.piles.build(&mut self.inst, sc);
        self.balls.build(&mut self.inst);
        for f in &self.flakes {
            self.inst.push(Instance {
                pos: [f.x, f.y],
                size: f.size * sc,
                rot: 0.0,
                color: [0.9, 0.95, 1.0, f.alpha],
                param: [f.depth, 0.0, 0.0, 0.0],
            });
        }
    }
}

impl Effect for Snow {
    fn resize(&mut self, width: f32, height: f32) {
        let (w, h) = sanitize(width, height);
        let (rx, ry) = (w / self.w, h / self.h);
        for f in &mut self.flakes {
            f.x *= rx;
            f.y *= ry;
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
        let m = 16.0 * sc;
        let (w, h, t) = (self.w, self.h, self.t);
        let k = (dt * 2.0).min(1.0);
        self.piles.update(dt, env.wind, scene.surfaces, scene.ground, w, h, &mut self.rng);
        let have_piles = !self.piles.piles.is_empty();
        // Heavy snow already has many more flakes, so each adds less to keep it ~2.5x faster.
        let per_flake = 3.0 * sc / 3.9 * if self.heavy { 0.5 } else { 1.0 };
        for f in &mut self.flakes {
            f.vx += (wind * f.resp * sc - f.vx) * k;
            let sway = (t * f.freq + f.phase).sin() * f.amp * sc;
            f.x += (f.vx + sway) * dt * sp;
            let mut y0 = f.y;
            f.y += f.fall * sc * sp * dt;
            if !self.balls.balls.is_empty() && self.balls.deflect(&mut f.x, &mut f.y, &mut f.vx, sc) {
                y0 = f.y;
            }
            let landed = have_piles && self.piles.absorb(f.x, y0, f.y, per_flake * f.size);
            if landed || f.y > h + m {
                f.y = -m - self.rng.f32() * m;
                f.x = self.rng.f32() * w;
            }
            f.x = wrap(f.x, -m, w + m);
        }
        self.snowballs_on = scene.snowballs;
        self.balls.update(dt, scene, &mut self.piles, sc, w, h, &mut self.rng);
        self.build();
    }

    fn incoming_snowball(&mut self, x: f32) -> bool {
        let sc = ui_scale(self.w, self.h);
        self.balls.incoming(x * self.w, sc, &mut self.rng);
        true
    }

    fn grabs_at(&self, pos: [f32; 2]) -> bool {
        self.snowballs_on && self.balls.balls.iter().any(|b| (b.x - pos[0]).hypot(b.y - pos[1]) <= b.r)
    }

    fn set_pile_depth(&mut self, depth: f32) {
        self.piles.set_depth(depth);
    }

    fn clear_accumulation(&mut self) {
        self.piles.clear();
        self.balls.clear();
        self.build();
    }

    fn grab_zones(&self, out: &mut Vec<[f32; 4]>) {
        if !self.snowballs_on {
            return;
        }
        // Same reach/min height the snowball spawn test uses (Snowballs::update).
        let sc = ui_scale(self.w, self.h);
        // Releases are swallowed by the hook while captured, wherever the cursor is.
        self.piles.grab_zones(40.0 * sc, 3.0 * sc, out);
        self.balls.grab_zones(out);
    }

    fn take_events(&mut self, out: &mut Vec<WorldEvent>) {
        self.balls.take_events(out);
    }

    fn clear_region(&mut self, x0: f32, x1: f32) {
        let sc = ui_scale(self.w, self.h);
        let (a, b) = (x0.min(x1), x0.max(x1));
        if let Some((y, removed)) = self.piles.clear_region(a, b) {
            let n = ((removed / 3.0) as usize).min(14);
            self.balls.spray(a, b, y, n, sc, &mut self.rng);
        }
        self.balls.clear_region(a, b, sc, &mut self.rng);
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
    use crate::snowball::BState;
    use snowy_core::{Pointer, Surface};

    const DT: f32 = 1.0 / 60.0;

    fn env(t: f32) -> Env {
        Env { wind: 0.0, intensity: 1.0, speed: 1.0, time: t }
    }

    fn ledge(id: u64, y: f32) -> Surface {
        Surface { id, x0: 400.0, x1: 900.0, y }
    }

    fn run(s: &mut Snow, secs: f32, scene: &Scene) {
        let n = (secs / DT) as usize;
        for k in 0..n {
            s.update(DT, &env(k as f32 * DT), scene);
        }
    }

    fn sum(s: &Snow) -> f32 {
        s.piles.piles.iter().map(|p| p.cols.iter().sum::<f32>()).sum()
    }

    fn scene<'a>(surfaces: &'a [Surface], ground: Option<f32>, p: Option<Pointer>) -> Scene<'a> {
        Scene { surfaces, ground, pointer: p, snowballs: true }
    }

    fn ptr(x: f32, y: f32, down: bool, vel: [f32; 2]) -> Pointer {
        Pointer { pos: [x, y], vel, down, over_desktop: true }
    }

    fn check_instances(s: &Snow) {
        for i in s.instances() {
            assert!(i.pos[0].is_finite() && i.pos[1].is_finite() && i.rot.is_finite());
            assert!(i.size.is_finite() && i.size > 0.0);
            assert!(i.color.iter().all(|c| (0.0..=1.0).contains(c)), "{:?}", i.color);
            assert!(i.param.iter().all(|p| p.is_finite()));
        }
    }

    #[test]
    fn flakes_accumulate_on_ledge() {
        for heavy in [false, true] {
            let mut s = Snow::new(1920.0, 1080.0, heavy);
            let surf = [ledge(1, 500.0)];
            run(&mut s, 20.0, &scene(&surf, None, None));
            let p = &s.piles.piles[0];
            let mid = p.cols[p.cols.len() / 2];
            assert!(mid > 0.5, "heavy={heavy} mid height {mid}");
            assert!(s.instances().iter().any(|i| i.param[3] == 1.0));
            check_instances(&s);
        }
    }

    #[test]
    fn heavy_accumulates_faster() {
        let surf = [ledge(1, 500.0)];
        let mut a = Snow::new(1920.0, 1080.0, false);
        let mut b = Snow::new(1920.0, 1080.0, true);
        run(&mut a, 20.0, &scene(&surf, None, None));
        run(&mut b, 20.0, &scene(&surf, None, None));
        assert!(sum(&b) > sum(&a) * 1.5);
    }

    #[test]
    fn pile_height_is_capped() {
        let mut s = Snow::new(1920.0, 1080.0, true);
        let surf = [ledge(1, 500.0)];
        let sc = scene(&surf, Some(1000.0), None);
        for k in 0..6000 {
            s.update(0.05, &env(k as f32), &sc);
        }
        for p in &s.piles.piles {
            assert!(p.cols.iter().all(|&c| c <= p.max_h + 1e-3));
        }
        assert!(s.piles.piles[0].cols.iter().cloned().fold(0.0, f32::max) > 8.0);
    }

    fn filled(surf: &[Surface], ground: Option<f32>) -> Snow {
        let mut s = Snow::new(1920.0, 1080.0, false);
        s.update(DT, &env(0.0), &scene(surf, ground, None));
        for p in &mut s.piles.piles {
            p.cols.fill(6.0);
        }
        s
    }

    #[test]
    fn removed_surface_drops_pile_as_clumps() {
        let surf = [ledge(1, 500.0)];
        let mut s = filled(&surf, None);
        s.update(DT, &env(0.0), &scene(&[], None, None));
        assert!(s.piles.piles.is_empty());
        assert!(s.piles.clump_count() > 3);
        assert!(s.instances().iter().any(|i| i.param[3] == 3.0));
        run(&mut s, 5.0, &scene(&[], None, None));
        assert_eq!(s.piles.clump_count(), 0);
    }

    #[test]
    fn moved_surface_drops_pile() {
        let surf = [ledge(1, 500.0)];
        let mut s = filled(&surf, None);
        let moved = [ledge(1, 550.0)];
        s.update(DT, &env(0.0), &scene(&moved, None, None));
        assert_eq!(s.piles.piles.len(), 1);
        assert!(s.piles.piles[0].cols.iter().all(|&c| c < 1.0));
        assert!(s.piles.clump_count() > 3);
        // Small jitter keeps the pile.
        let mut s = filled(&surf, None);
        s.update(DT, &env(0.0), &scene(&[ledge(1, 501.5)], None, None));
        assert!(s.piles.piles[0].cols[40] > 5.0);
        assert_eq!(s.piles.clump_count(), 0);
    }

    #[test]
    fn cropped_segment_drops_uncovered_columns() {
        let surf = [ledge(1, 500.0)];
        let mut s = filled(&surf, None);
        let cropped = [Surface { id: 1, x0: 400.0, x1: 600.0, y: 500.0 }];
        s.update(DT, &env(0.0), &scene(&cropped, None, None));
        assert!(s.piles.piles[0].cols[10] > 4.0);
        assert!(s.piles.clump_count() > 3);
    }

    #[test]
    fn ground_pile_only_when_ground_some() {
        let mut a = Snow::new(1920.0, 1080.0, true);
        let mut b = Snow::new(1920.0, 1080.0, true);
        run(&mut a, 15.0, &scene(&[], Some(1080.0), None));
        run(&mut b, 15.0, &scene(&[], None, None));
        assert!(sum(&a) > 100.0);
        assert!(b.piles.piles.is_empty());
    }

    #[test]
    fn snowball_spawn_conditions() {
        let g = Some(1000.0);
        let near = |down, over, y| Pointer { pos: [500.0, y], vel: [0.0; 2], down, over_desktop: over };
        for (p, snowballs, expect) in [
            (near(true, true, 300.0), true, 0),
            (near(false, true, 990.0), true, 0),
            (near(true, false, 990.0), true, 0),
            (near(true, true, 990.0), false, 0),
            (near(true, true, 990.0), true, 1),
        ] {
            let mut s = filled(&[], g);
            let sc = Scene { surfaces: &[], ground: g, pointer: Some(p), snowballs };
            s.update(DT, &env(0.0), &sc);
            assert_eq!(s.balls.balls.len(), expect, "{p:?} {snowballs}");
        }
        // Snow-free ground does not trigger.
        let mut s = Snow::new(1920.0, 1080.0, false);
        s.update(DT, &env(0.0), &scene(&[], g, Some(near(true, true, 995.0))));
        assert!(s.balls.balls.is_empty());
    }

    #[test]
    fn snowballs_flag_off_keeps_existing_balls() {
        let g = Some(1000.0);
        let mut s = filled(&[], g);
        s.update(DT, &env(0.0), &scene(&[], g, Some(ptr(500.0, 990.0, true, [0.0; 2]))));
        assert_eq!(s.balls.balls.len(), 1);
        let off = Scene { surfaces: &[], ground: g, pointer: Some(ptr(500.0, 990.0, false, [0.0; 2])), snowballs: false };
        run(&mut s, 2.0, &off);
        assert_eq!(s.balls.balls.len(), 1);
    }

    #[test]
    fn snowball_grows_and_clears_snow_when_dragged() {
        let g = Some(1000.0);
        let mut s = filled(&[], g);
        s.piles.piles[0].cols.fill(10.0);
        let before = sum(&s);
        for k in 0..150 {
            let p = ptr(200.0 + k as f32 * 8.0, 990.0, true, [480.0, 0.0]);
            s.update(DT, &env(0.0), &scene(&[], g, Some(p)));
        }
        assert_eq!(s.balls.balls.len(), 1);
        assert!(s.balls.balls[0].r > 15.0, "r {}", s.balls.balls[0].r);
        assert!(sum(&s) < before - 20.0);
        let ball = s.instances().iter().find(|i| i.param[3] == 2.0).unwrap();
        assert!(ball.rot != 0.0);
        check_instances(&s);
    }

    fn throw(vel: [f32; 2]) -> Snow {
        let g = Some(1000.0);
        let mut s = filled(&[], g);
        s.piles.piles[0].cols.fill(10.0);
        s.update(DT, &env(0.0), &scene(&[], g, Some(ptr(500.0, 960.0, true, [0.0; 2]))));
        assert_eq!(s.balls.balls.len(), 1);
        s.update(DT, &env(0.0), &scene(&[], g, Some(ptr(500.0, 960.0, false, vel))));
        s
    }

    #[test]
    fn fast_impact_breaks_into_chunks() {
        let mut s = throw([0.0, 2500.0]);
        let g = Some(1000.0);
        let mut max_chunks = 0;
        for _ in 0..10 {
            s.update(DT, &env(0.0), &scene(&[], g, None));
            max_chunks = max_chunks.max(s.balls.chunk_count());
        }
        assert!(s.balls.balls.is_empty());
        assert!((10..=20).contains(&max_chunks), "chunks {max_chunks}");
        check_instances(&s);
        run(&mut s, 2.0, &scene(&[], g, None));
        assert_eq!(s.balls.chunk_count(), 0);
    }

    #[test]
    fn slow_landing_rests() {
        let mut s = throw([0.0, 0.0]);
        let g = Some(1000.0);
        run(&mut s, 3.0, &scene(&[], g, None));
        assert_eq!(s.balls.balls.len(), 1);
        let b = s.balls.balls[0];
        assert!(matches!(b.state, BState::Rest { .. }), "{:?}", b.state);
        assert!((b.y + b.r - (1000.0 - 10.0)).abs() < 3.0);
        // Ground vanishes: the ball falls and leaves the screen.
        run(&mut s, 3.0, &scene(&[], None, None));
        assert!(s.balls.balls.is_empty());
    }

    #[test]
    fn resting_ball_falls_when_ledge_moves() {
        let surf = [ledge(7, 500.0)];
        let mut s = filled(&surf, None);
        s.update(DT, &env(0.0), &scene(&surf, None, Some(ptr(600.0, 490.0, true, [0.0; 2]))));
        s.update(DT, &env(0.0), &scene(&surf, None, Some(ptr(600.0, 490.0, false, [0.0; 2]))));
        run(&mut s, 2.0, &scene(&surf, None, None));
        assert!(matches!(s.balls.balls[0].state, BState::Rest { .. }));
        let moved = [ledge(7, 560.0)];
        s.update(DT, &env(0.0), &scene(&moved, None, None));
        assert_eq!(s.balls.balls[0].state, BState::Free);
    }

    #[test]
    fn clear_accumulation_empties_everything() {
        let g = Some(1000.0);
        let mut s = throw([0.0, 0.0]);
        run(&mut s, 1.0, &scene(&[], g, None));
        assert!(!s.balls.balls.is_empty());
        s.update(DT, &env(0.0), &scene(&[], None, None));
        s.clear_accumulation();
        assert!(sum(&s) == 0.0 && s.balls.balls.is_empty() && s.piles.clump_count() == 0);
        assert!(s.instances().iter().all(|i| i.param[3] == 0.0));
    }

    #[test]
    fn empty_scene_has_only_flakes() {
        let mut s = Snow::new(1920.0, 1080.0, false);
        run(&mut s, 5.0, &Scene::EMPTY);
        assert!(s.instances().iter().all(|i| i.param[3] == 0.0));
        assert_eq!(s.instances().len(), s.flakes.len());
    }

    // ---- stacking / plough ----

    use crate::pile::GROUND_ID;
    use crate::snowball::Ball;
    use snowy_core::WorldEvent;

    fn ball(id: u32, x: f32, y: f32, r: f32, state: BState) -> Ball {
        Ball { id, x, y, vx: 0.0, vy: 0.0, r, rot: 0.0, state, fixed: false, rel: false, timer: 0.0 }
    }

    /// Ground at 1000 with a settled pile, plus a resting ball A (id 1, r 12) at x=500.
    fn with_rest_ball() -> Snow {
        let mut s = filled(&[], Some(1000.0));
        let top = s.piles.piles[0].top_at(500.0);
        s.balls.balls.push(ball(1, 500.0, top - 12.0, 12.0, BState::Rest { id: GROUND_ID, y: 1000.0 }));
        s
    }

    fn drop_on(s: &mut Snow, id: u32, r: f32, dx: f32) {
        let a = s.balls.balls[0];
        s.balls.balls.push(Ball { vy: 100.0, ..ball(id, a.x + dx, a.y - a.r - r - 30.0, r, BState::Free) });
    }

    fn events(s: &mut Snow) -> Vec<WorldEvent> {
        let mut v = Vec::new();
        s.take_events(&mut v);
        v
    }

    #[test]
    fn two_stacked_balls_become_snowman_after_wait() {
        let mut s = with_rest_ball();
        drop_on(&mut s, 2, 10.0, 3.0);
        let g = scene(&[], Some(1000.0), None);
        run(&mut s, 1.0, &g);
        assert!(matches!(s.balls.balls[1].state, BState::Stacked { on: 1 }), "{:?}", s.balls.balls[1].state);
        assert!(events(&mut s).is_empty(), "must wait for a possible third ball");
        run(&mut s, 1.0, &g);
        let ev = events(&mut s);
        assert_eq!(ev.len(), 1, "{ev:?}");
        let WorldEvent::SnowmanBuilt { x, balls, height } = ev[0];
        assert_eq!(balls, 2);
        assert!((x - 500.0).abs() < 1.0);
        assert!((height - 44.0 * 0.92).abs() < 1e-3);
        assert!(s.balls.balls.is_empty());
        assert!(events(&mut s).is_empty(), "take_events drains");
        run(&mut s, 2.0, &g);
        assert!(events(&mut s).is_empty());
    }

    #[test]
    fn third_ball_completes_snowman_immediately() {
        let mut s = with_rest_ball();
        drop_on(&mut s, 2, 10.0, 0.0);
        let g = scene(&[], Some(1000.0), None);
        run(&mut s, 0.8, &g);
        let b = s.balls.balls[1];
        s.balls.balls.push(Ball { vy: 100.0, ..ball(3, b.x, b.y - b.r - 8.0 - 30.0, 8.0, BState::Free) });
        run(&mut s, 0.5, &g);
        let ev = events(&mut s);
        assert_eq!(ev.len(), 1, "{ev:?}");
        let WorldEvent::SnowmanBuilt { balls, height, .. } = ev[0];
        assert_eq!(balls, 3);
        assert!((height - 60.0 * 0.92).abs() < 1e-3);
        assert!(s.balls.balls.is_empty());
        assert!(s.balls.chunk_count() > 0);
    }

    #[test]
    fn bigger_ball_does_not_stack_on_smaller() {
        let mut s = with_rest_ball();
        drop_on(&mut s, 2, 14.0, 0.0);
        run(&mut s, 3.0, &scene(&[], Some(1000.0), None));
        assert!(events(&mut s).is_empty());
        assert!(s.balls.balls.iter().all(|b| !matches!(b.state, BState::Stacked { .. })));
        let (a, b) = (s.balls.balls[0], s.balls.balls[1]);
        assert!((a.x - b.x).abs() > 3.0, "upper ball should have rolled off");
    }

    #[test]
    fn fast_ball_does_not_stack() {
        let mut s = with_rest_ball();
        drop_on(&mut s, 2, 10.0, 0.0);
        s.balls.balls[1].vy = 1500.0;
        run(&mut s, 3.0, &scene(&[], Some(1000.0), None));
        assert!(s.balls.balls.iter().all(|b| !matches!(b.state, BState::Stacked { .. })));
    }

    #[test]
    fn stacking_on_ledge_pile_emits_nothing() {
        let surf = [ledge(1, 500.0)];
        let mut s = filled(&surf, None);
        let top = s.piles.piles[0].top_at(600.0);
        s.balls.balls.push(ball(1, 600.0, top - 12.0, 12.0, BState::Rest { id: 1, y: 500.0 }));
        drop_on(&mut s, 2, 10.0, 0.0);
        run(&mut s, 3.0, &scene(&surf, None, None));
        assert!(events(&mut s).is_empty());
        assert!(s.balls.balls.iter().all(|b| !matches!(b.state, BState::Stacked { .. })));
    }

    #[test]
    fn resting_ball_can_be_picked_up_and_has_grab_zone() {
        let mut s = with_rest_ball();
        let g = Some(1000.0);
        s.update(DT, &env(0.0), &scene(&[], g, Some(ptr(900.0, 300.0, false, [0.0; 2]))));
        let a = s.balls.balls[0];
        let mut z = Vec::new();
        s.grab_zones(&mut z);
        assert!(z.iter().any(|r| r[0] <= a.x - a.r - 6.0 + 1e-3 && r[2] >= a.x + a.r + 6.0 - 1e-3 && r[1] <= a.y - a.r && r[3] >= a.y + a.r));
        s.update(DT, &env(0.0), &scene(&[], g, Some(ptr(a.x, a.y, true, [0.0; 2]))));
        assert_eq!(s.balls.balls.len(), 1);
        assert_eq!(s.balls.balls[0].state, BState::Held);
        assert!((s.balls.balls[0].r - 12.0).abs() < 1e-3);
    }

    #[test]
    fn clear_region_zeroes_ground_columns_and_removes_balls_in_range() {
        let surf = [ledge(1, 500.0)];
        let mut s = filled(&surf, Some(1000.0));
        let top = s.piles.piles.iter().find(|p| p.ground).unwrap().top_at(400.0);
        s.balls.balls.push(ball(1, 400.0, top - 12.0, 12.0, BState::Rest { id: GROUND_ID, y: 1000.0 }));
        s.balls.balls.push(ball(2, 1200.0, top - 12.0, 12.0, BState::Rest { id: GROUND_ID, y: 1000.0 }));
        let ledge_before: Vec<f32> = s.piles.piles.iter().find(|p| !p.ground).unwrap().cols.clone();
        s.clear_region(300.0, 500.0);
        let g = s.piles.piles.iter().find(|p| p.ground).unwrap();
        for (i, c) in g.cols.iter().enumerate() {
            let x = g.x0 + (i as f32 + 0.5) * g.colw;
            if (300.0 + g.colw..500.0 - g.colw).contains(&x) {
                assert_eq!(*c, 0.0, "col at {x}");
            }
            if !(300.0 - g.colw..500.0 + g.colw).contains(&x) {
                assert!(*c > 0.0, "col at {x}");
            }
        }
        assert_eq!(s.balls.balls.len(), 1);
        assert_eq!(s.balls.balls[0].id, 2);
        assert!(s.balls.chunk_count() > 0);
        assert_eq!(s.piles.piles.iter().find(|p| !p.ground).unwrap().cols, ledge_before);
        check_instances(&s);
        // Region without a ground pile is harmless.
        let mut e = Snow::new(1920.0, 1080.0, false);
        e.clear_region(0.0, 100.0);
    }

    #[test]
    #[ignore]
    fn bench_update_with_30_surfaces() {
        let surf: Vec<Surface> = (0..30)
            .map(|i| Surface { id: i as u64 + 1, x0: 50.0 + i as f32 * 40.0, x1: 450.0 + i as f32 * 40.0, y: 100.0 + i as f32 * 28.0 })
            .collect();
        for heavy in [false, true] {
            let mut s = Snow::new(1920.0, 1080.0, heavy);
            let sc = scene(&surf, Some(1080.0), Some(ptr(900.0, 1070.0, false, [0.0; 2])));
            run(&mut s, 30.0, &sc);
            let t0 = std::time::Instant::now();
            for k in 0..600 {
                s.update(DT, &env(k as f32 * DT), &sc);
            }
            eprintln!("heavy={heavy}: {:?}/frame, {} instances", t0.elapsed() / 600, s.instances().len());
        }
    }
}
