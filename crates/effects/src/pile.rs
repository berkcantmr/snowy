//! Snow accumulation: per-ledge column heightmaps, plus falling clumps when a ledge goes away.

use crate::util::*;
use fastrand::Rng;
use snowy_core::{Instance, Surface};

/// Cap on instances emitted for piles.
pub const MAX_PILE_INSTANCES: usize = 6000;
/// Pseudo surface id of the ground pile.
pub const GROUND_ID: u64 = u64::MAX;
const MAX_CLUMPS: usize = 800;

/// Default maximum ground snow height in pixels at 1080p (ledges get half).
pub const DEFAULT_DEPTH: f32 = 28.0;
/// Range of the user setting.
pub const DEPTH_RANGE: (f32, f32) = (8.0, 90.0);

pub struct Pile {
    pub id: u64,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    pub colw: f32,
    pub max_h: f32,
    pub ground: bool,
    pub cols: Vec<f32>,
    /// Current maximum height per column: drifts (hills and dips) times the end taper.
    caps: Vec<f32>,
    /// Pixel scale (1.0 at 1080p), so drifts are the same size on every screen.
    sc: f32,
    /// Columns under resting balls this frame: smoothing leaves them alone, so a ball's
    /// footing doesn't wear away under it.
    frozen: Vec<usize>,
}

/// Smooth value noise in 0..1 along x (`seed` picks the pattern).
fn value_noise(x: f32, seed: u64) -> f32 {
    fn hash(i: i64, seed: u64) -> f32 {
        let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        h ^= h >> 31;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 29;
        (h >> 40) as f32 / (1u64 << 24) as f32
    }
    let fl = x.floor();
    let f = x - fl;
    let t = f * f * (3.0 - 2.0 * f);
    let i = fl as i64;
    let (a, b) = (hash(i, seed), hash(i + 1, seed));
    a + (b - a) * t
}

/// Drift shape at x (reference pixels): 0..1, a few big hills with smaller bumps on them.
pub fn drift(x: f32, seed: u64) -> f32 {
    let big = value_noise(x / 240.0, seed);
    let small = value_noise(x / 85.0, seed ^ 0x5157);
    (big * 0.68 + small * 0.32).clamp(0.0, 1.0)
}

impl Pile {
    fn new(id: u64, x0: f32, x1: f32, y: f32, cw: f32, max_h: f32, sc: f32) -> Pile {
        let n = (((x1 - x0) / cw).round() as usize).max(1);
        let mut p = Pile {
            id,
            x0,
            x1,
            y,
            colw: (x1 - x0) / n as f32,
            max_h,
            ground: id == GROUND_ID,
            cols: vec![0.0; n],
            caps: vec![max_h; n],
            sc: if sc.is_finite() { sc.max(0.01) } else { 1.0 },
            frozen: Vec::new(),
        };
        p.reshape(0.0);
        p
    }

    pub fn col(&self, x: f32) -> usize {
        let n = self.cols.len() as isize;
        (((x - self.x0) / self.colw) as isize).clamp(0, n - 1) as usize
    }

    /// Maximum height of column `i` for the drift pattern shifted by `offset` reference
    /// pixels: ground snow forms hills and dips (35..100 % of the maximum); ledge snow is
    /// flatter and tapers towards the ends.
    fn cap_for(&self, i: usize, offset: f32) -> f32 {
        let cx = (self.x0 + (i as f32 + 0.5) * self.colw) / self.sc - offset;
        let seed = self.id ^ ((self.y / self.sc) as u64).wrapping_mul(31);
        if self.ground {
            return self.max_h * (0.35 + 0.65 * drift(cx, seed));
        }
        let n = self.cols.len();
        let d = (i as f32 + 0.5).min(n as f32 - i as f32 - 0.5);
        let t = (d / 4.0).clamp(0.0, 1.0);
        let taper = 0.15 + 0.85 * t * t * (3.0 - 2.0 * t);
        self.max_h * taper * (0.6 + 0.4 * drift(cx, seed))
    }

    fn cap(&self, i: usize) -> f32 {
        self.caps[i]
    }

    /// Recompute the caps for drift offset `offset`. Every column keeps how full it is, so
    /// hills slide along with the pattern (the wind pushing the drifts).
    fn reshape(&mut self, offset: f32) {
        for i in 0..self.cols.len() {
            let new = self.cap_for(i, offset).max(1e-3);
            let old = self.caps[i];
            if old > 1e-3 && self.cols[i] > 0.0 {
                // Snow above the cap (after the maximum was lowered) melts on its own; only
                // the part within the cap follows the drift.
                let excess = (self.cols[i] - old).max(0.0);
                self.cols[i] = (self.cols[i] - excess) * new / old + excess;
            }
            self.caps[i] = new;
        }
    }

    /// Change the maximum height. The snow already there stays as it is: it grows towards a
    /// higher maximum with the snowfall, and melts down to a lower one over a few seconds.
    fn set_max_h(&mut self, max_h: f32, offset: f32) {
        if (max_h - self.max_h).abs() < 1e-3 {
            return;
        }
        self.max_h = max_h;
        for i in 0..self.cols.len() {
            self.caps[i] = self.cap_for(i, offset).max(1e-3);
        }
    }

    /// Melt snow standing above the cap by up to `amount` per column.
    fn settle(&mut self, amount: f32) {
        for (c, &cap) in self.cols.iter_mut().zip(&self.caps) {
            if *c > cap {
                *c = (*c - amount).max(cap);
            }
        }
    }

    /// The ball with centre (cx, cy) and radius r rests here, sunk `sink` into the snow: snow
    /// under it can't stand more than `sink` inside its outline, so the ball looks pressed a
    /// little into the snow and nothing grows up through it.
    pub fn press(&mut self, cx: f32, cy: f32, r: f32, sink: f32) {
        if cx + r < self.x0 || cx - r > self.x1 {
            return;
        }
        let (a, b) = (self.col(cx - r), self.col(cx + r));
        let mut pushed = 0.0;
        for i in a..=b {
            let x = self.x0 + (i as f32 + 0.5) * self.colw;
            let dx = x - cx;
            if dx.abs() >= r {
                continue;
            }
            if !self.frozen.contains(&i) {
                self.frozen.push(i);
            }
            // The ball stands on the middle column, sunk `sink` into it (that snow hides
            // behind the ball and keeps it up); everywhere else under it the snow is squashed
            // down to the ball's outline.
            let sunk = if dx.abs() < self.colw { sink } else { 0.0 };
            let bottom = cy + (r * r - dx * dx).sqrt() - sunk;
            let max_h = (self.y - bottom).max(0.0);
            if self.cols[i] > max_h {
                pushed += self.cols[i] - max_h;
                self.cols[i] = max_h;
            }
        }
        // The squashed snow is pushed out to the sides: a little rim around the ball.
        if pushed > 0.0 {
            self.heap_around(cx, r, pushed * 0.6);
        }
    }

    /// Adds `amount` (total height) just outside a ball of radius r at cx, most of it
    /// right at the edge, without going over each column's cap.
    fn heap_around(&mut self, cx: f32, r: f32, amount: f32) {
        let n = self.cols.len();
        let (lo, hi) = (r, r * 1.6);
        let mut slots: Vec<(usize, f32)> = Vec::new();
        for side in [-1.0f32, 1.0] {
            let mut d = lo;
            while d < hi {
                let x = cx + side * (d + self.colw * 0.5);
                if x >= self.x0 && x <= self.x1 {
                    let i = self.col(x);
                    if i < n && !slots.iter().any(|(j, _)| *j == i) {
                        // Falls off linearly from the ball's edge.
                        slots.push((i, 1.0 - (d - lo) / (hi - lo)));
                    }
                }
                d += self.colw;
            }
        }
        let total: f32 = slots.iter().map(|(_, w)| w).sum();
        if total <= 0.0 {
            return;
        }
        for (i, w) in slots {
            let add = amount * w / total;
            self.cols[i] = (self.cols[i] + add).min(self.caps[i].max(self.cols[i]));
        }
    }

    pub fn height_at(&self, x: f32) -> f32 {
        self.cols[self.col(x)]
    }

    /// Y of the snow surface at x.
    pub fn top_at(&self, x: f32) -> f32 {
        self.y - self.height_at(x)
    }

    /// Add snow around x. Growth slows as a column approaches its cap, and columns fill at
    /// the same pace relative to their cap, so the drifts show from the first flakes on.
    pub fn add(&mut self, x: f32, amt: f32) {
        let i = self.col(x);
        let n = self.cols.len();
        let rel = amt / self.max_h.max(1e-3);
        for (j, wgt) in [(i as isize, 0.6), (i as isize - 1, 0.2), (i as isize + 1, 0.2)] {
            if j < 0 || j >= n as isize {
                continue;
            }
            let j = j as usize;
            let c = self.cap(j);
            let room = (1.0 - self.cols[j] / c).max(0.0);
            self.cols[j] = (self.cols[j] + rel * c * wgt * room).min(c);
        }
    }

    /// Remove up to `per_col` height from each column within [xa, xb]; returns total removed.
    pub fn remove(&mut self, xa: f32, xb: f32, per_col: f32) -> f32 {
        if xb < self.x0 || xa > self.x1 {
            return 0.0;
        }
        let (a, b) = (self.col(xa), self.col(xb));
        let mut tot = 0.0;
        for c in &mut self.cols[a..=b] {
            let r = c.min(per_col);
            *c -= r;
            tot += r;
        }
        tot
    }

    /// Smooth how full the columns are (not their heights), so gaps fill in while the
    /// drift shape stays.
    fn diffuse(&mut self, k: f32) {
        let n = self.cols.len();
        if n < 2 {
            return;
        }
        // Snow above the cap (a lowered maximum, still melting) is kept aside, not smoothed.
        let fill = |p: &Pile, i: usize| (p.cols[i] / p.caps[i].max(1e-3)).min(1.0);
        let mut prev = fill(self, 0);
        for i in 0..n {
            let cur = fill(self, i);
            let next = if i + 1 < n { fill(self, i + 1) } else { cur };
            let left = if i == 0 { cur } else { prev };
            prev = cur;
            if self.frozen.contains(&i) {
                continue;
            }
            let f = (cur + k * (left + next - 2.0 * cur)).clamp(0.0, 1.0);
            let excess = (self.cols[i] - self.caps[i]).max(0.0);
            self.cols[i] = f * self.caps[i] + excess;
        }
    }
}

struct Clump {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    amount: f32,
}

#[derive(Default)]
pub struct Piles {
    pub piles: Vec<Pile>,
    clumps: Vec<Clump>,
    last: Vec<Surface>,
    last_ground: Option<f32>,
    dims: (f32, f32),
    /// Maximum ground height at 1080p (user setting); 0 = DEFAULT_DEPTH.
    depth: f32,
    /// How far the wind has pushed the drift pattern (reference pixels).
    drift_offset: f32,
}

impl Piles {
    #[cfg(test)]
    pub fn clump_count(&self) -> usize {
        self.clumps.len()
    }

    /// Empty every pile and drop all clumps (piles keep tracking their surfaces).
    pub fn clear(&mut self) {
        for p in &mut self.piles {
            p.cols.fill(0.0);
        }
        self.clumps.clear();
    }

    /// Zero the ground pile's columns overlapping `x0..x1`. Returns the ground line y and the
    /// total height removed, or None when there is no ground pile.
    pub fn clear_region(&mut self, x0: f32, x1: f32) -> Option<(f32, f32)> {
        let p = self.piles.iter_mut().find(|p| p.ground)?;
        if x0 > x1 || x0.is_nan() || x1 < p.x0 || x0 > p.x1 {
            return Some((p.y, 0.0));
        }
        let (a, b) = (p.col(x0), p.col(x1));
        let mut tot = 0.0;
        for c in &mut p.cols[a..=b] {
            tot += *c;
            *c = 0.0;
        }
        Some((p.y, tot))
    }

    /// If x,[y0..y1] crosses the snow top of some pile, return its index.
    fn hit(&self, x: f32, y0: f32, y1: f32) -> Option<usize> {
        self.piles.iter().position(|p| {
            y0 <= p.y && x >= p.x0 && x <= p.x1 && y1 >= p.y - p.max_h && y1 >= p.top_at(x)
        })
    }

    /// Absorb a falling particle that moved from y0 to y1 at x. Returns true when it landed.
    pub fn absorb(&mut self, x: f32, y0: f32, y1: f32, amount: f32) -> bool {
        match self.hit(x, y0, y1) {
            Some(i) => {
                self.piles[i].add(x, amount);
                true
            }
            None => false,
        }
    }

    /// Rects where `near_snow(x, y, reach, min_h)` can be true: per pile, each run of columns
    /// taller than `min_h` becomes `[x0, highest_top - reach, x1, pile_y + reach * 0.15]`.
    pub fn grab_zones(&self, reach: f32, min_h: f32, out: &mut Vec<[f32; 4]>) {
        for p in &self.piles {
            let mut run: Option<(usize, f32)> = None; // (first col, min top)
            for i in 0..=p.cols.len() {
                let tall = i < p.cols.len() && p.cols[i] > min_h;
                match (tall, run) {
                    (true, None) => run = Some((i, p.y - p.cols[i])),
                    (true, Some((s, top))) => run = Some((s, top.min(p.y - p.cols[i]))),
                    (false, Some((s, top))) => {
                        let x0 = (p.x0 + s as f32 * p.colw).max(p.x0);
                        let x1 = (p.x0 + i as f32 * p.colw).min(p.x1);
                        out.push([x0, top - reach, x1, p.y + reach * 0.15]);
                        run = None;
                    }
                    (false, None) => {}
                }
            }
        }
    }

    /// Any pile column under x with height > `min_h` whose top is within `reach` below the point.
    pub fn near_snow(&self, x: f32, y: f32, reach: f32, min_h: f32) -> bool {
        self.piles.iter().any(|p| {
            x >= p.x0
                && x <= p.x1
                && p.height_at(x) > min_h
                && y >= p.top_at(x) - reach
                && y <= p.y + reach * 0.15
        })
    }

    /// A ball resting on the pile with surface `id` at line `y`: see `Pile::press`.
    pub fn press(&mut self, id: u64, y: f32, cx: f32, cy: f32, r: f32, sink: f32) {
        if let Some(p) = self.piles.iter_mut().find(|p| p.id == id && (p.y - y).abs() <= 2.0) {
            p.press(cx, cy, r, sink);
        }
    }

    /// Snow height under x on the pile with surface `id` at line `y`.
    pub fn height_under(&self, id: u64, y: f32, x: f32) -> Option<f32> {
        self.piles.iter().find(|p| p.id == id && (p.y - y).abs() <= 2.0).map(|p| p.height_at(x))
    }

    /// Maximum ground snow height in pixels at 1080p (ledges get half), within DEPTH_RANGE.
    pub fn set_depth(&mut self, depth: f32) {
        let d = if depth.is_finite() { depth.clamp(DEPTH_RANGE.0, DEPTH_RANGE.1) } else { DEFAULT_DEPTH };
        if (d - self.depth()).abs() < 1e-3 {
            return;
        }
        self.depth = d;
        let off = self.drift_offset;
        for p in &mut self.piles {
            let mh = Self::max_h_for(d, p.ground, p.sc);
            p.set_max_h(mh, off);
        }
    }

    fn depth(&self) -> f32 {
        if self.depth > 0.0 {
            self.depth
        } else {
            DEFAULT_DEPTH
        }
    }

    fn max_h_for(depth: f32, ground: bool, sc: f32) -> f32 {
        if ground {
            depth * sc
        } else {
            depth * 0.5 * sc
        }
    }

    /// `wind` is roughly -1..1 (negative = blowing left); it slowly pushes the drifts along.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, wind: f32, surfaces: &[Surface], ground: Option<f32>, w: f32, h: f32, rng: &mut Rng) {
        let sc = ui_scale(w, h);
        if self.last.as_slice() != surfaces || self.last_ground != ground || self.dims != (w, h) {
            self.reconcile(surfaces, ground, w, h, rng);
        }
        let wind = if wind.is_finite() { wind.clamp(-1.5, 1.5) } else { 0.0 };
        // About 6 reference px/s in a full wind: a hill crosses its own width in ~40 s.
        let step = wind * 6.0 * dt;
        if step.abs() > 1e-4 {
            self.drift_offset = (self.drift_offset + step) % 1.0e6;
            let off = self.drift_offset;
            for p in &mut self.piles {
                p.reshape(off);
            }
        }
        let k = (dt * 1.5).min(0.2);
        // After the maximum is lowered, extra snow melts at ~8 px/s (1080p).
        let melt = 8.0 * sc * dt;
        for p in &mut self.piles {
            p.diffuse(k);
            p.settle(melt);
            // Resting balls mark their columns again after this update.
            p.frozen.clear();
        }
        // Falling clumps.
        let mut i = 0;
        while i < self.clumps.len() {
            let c = &mut self.clumps[i];
            let y0 = c.y;
            c.vy += 1200.0 * sc * dt;
            c.vx *= 1.0 - (1.5 * dt).min(1.0);
            c.x += c.vx * dt;
            c.y += c.vy * dt;
            let (x, y1, a) = (c.x, c.y, c.amount * 0.6);
            if y1 > h + 60.0 || self.absorb(x, y0, y1, a) {
                self.clumps.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    fn reconcile(&mut self, surfaces: &[Surface], ground: Option<f32>, w: f32, h: f32, rng: &mut Rng) {
        let sc = ui_scale(w, h);
        let cw = 4.0 * sc;
        let dims_changed = self.dims != (w, h);
        let mut want: Vec<Surface> = Vec::with_capacity(surfaces.len() + 1);
        for s in surfaces {
            if s.x0.is_finite() && s.x1.is_finite() && s.y.is_finite() && s.x1 - s.x0 >= cw * 0.5 {
                want.push(*s);
            }
        }
        if let Some(g) = ground.filter(|g| g.is_finite()) {
            want.push(Surface { id: GROUND_ID, x0: 0.0, x1: w, y: g });
        }
        let old = std::mem::take(&mut self.piles);
        let mut covered: Vec<Vec<bool>> = old.iter().map(|p| vec![false; p.cols.len()]).collect();
        let mut new = Vec::with_capacity(want.len());
        for s in &want {
            let is_ground = s.id == GROUND_ID;
            let max_h = Self::max_h_for(self.depth(), is_ground, sc);
            let mut np = Pile::new(s.id, s.x0, s.x1, s.y, cw, max_h, sc);
            np.reshape(self.drift_offset);
            for (j, op) in old.iter().enumerate() {
                let same_y = (op.y - s.y).abs() <= 2.0 || (is_ground && dims_changed);
                if op.id != s.id || !same_y || op.x1 <= s.x0 || op.x0 >= s.x1 {
                    continue;
                }
                for i in 0..np.cols.len() {
                    let cx = np.x0 + (i as f32 + 0.5) * np.colw;
                    if cx >= op.x0 && cx <= op.x1 {
                        np.cols[i] = np.cols[i].max(op.height_at(cx));
                    }
                }
                for (k, c) in covered[j].iter_mut().enumerate() {
                    let cx = op.x0 + (k as f32 + 0.5) * op.colw;
                    if cx >= s.x0 && cx <= s.x1 {
                        *c = true;
                    }
                }
            }
            for i in 0..np.cols.len() {
                np.cols[i] = np.cols[i].min(np.cap(i));
            }
            new.push(np);
        }
        // Snow on columns nobody took over falls off as clumps.
        for (op, cov) in old.iter().zip(&covered) {
            let n = op.cols.len();
            let mut k = 0;
            while k < n {
                let end = (k + 3).min(n);
                let mut sum = 0.0;
                for (c, &covd) in op.cols[k..end].iter().zip(&cov[k..end]) {
                    if !covd {
                        sum += c;
                    }
                }
                if sum > 0.2 && self.clumps.len() < MAX_CLUMPS {
                    let avg = sum / (end - k) as f32;
                    self.clumps.push(Clump {
                        x: op.x0 + (k as f32 + (end - k) as f32 * 0.5) * op.colw,
                        y: op.y - avg * 0.5,
                        vx: range(rng, -35.0, 35.0) * sc,
                        vy: range(rng, -40.0, 5.0) * sc,
                        amount: sum / 3.0,
                    });
                }
                k = end;
            }
        }
        self.piles = new;
        self.last.clear();
        self.last.extend_from_slice(surfaces);
        self.last_ground = ground;
        self.dims = (w, h);
    }

    /// Append pile blobs (PARAM.w = 1) and clump specks (PARAM.w = 3).
    pub fn build(&self, out: &mut Vec<Instance>, sc: f32) {
        let start = out.len();
        'outer: for p in &self.piles {
            let n = p.cols.len();
            let mut k = 0;
            while k < n {
                if out.len() - start >= MAX_PILE_INSTANCES {
                    break 'outer;
                }
                let a = p.cols[k];
                let b = if k + 1 < n { p.cols[k + 1] } else { a };
                let hh = (a + b) * 0.5;
                if hh >= 0.35 * sc {
                    let cx = (p.x0 + (k as f32 + 1.0) * p.colw).min(p.x1);
                    let s = hh.max(p.colw * 2.0);
                    out.push(Instance {
                        pos: [cx, p.y - hh * 0.5],
                        size: s,
                        rot: 0.0,
                        color: [1.0, 1.0, 1.0, 1.0],
                        param: [hh / (2.0 * s), s, 0.0, 1.0],
                    });
                }
                k += 2;
            }
        }
        for (i, c) in self.clumps.iter().enumerate() {
            out.push(Instance {
                pos: [c.x, c.y],
                size: 1.5 * sc + c.amount * 0.35,
                rot: 0.0,
                color: [0.96, 0.98, 1.0, 1.0],
                param: [(i % 7) as f32 * 0.9, 0.0, 0.0, 3.0],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grab_zones_cover_exactly_where_near_snow_is_true() {
        let mut ps = Piles::default();
        let mut p = Pile::new(1, 100.0, 300.0, 500.0, 4.0, 14.0, 1.0);
        // Two separate runs of snow: x in [120,160) and [240,280); nothing elsewhere.
        for (i, c) in p.cols.iter_mut().enumerate() {
            let x = 100.0 + i as f32 * 4.0;
            if (120.0..160.0).contains(&x) || (240.0..280.0).contains(&x) {
                *c = 10.0;
            }
        }
        ps.piles.push(p);
        let mut z = Vec::new();
        ps.grab_zones(40.0, 3.0, &mut z);
        assert_eq!(z.len(), 2, "{z:?}");
        assert_eq!(z[0], [120.0, 500.0 - 10.0 - 40.0, 160.0, 500.0 + 6.0]);
        // Every point near_snow accepts lies inside some zone.
        for xi in (100..300).step_by(3) {
            for yi in (430..510).step_by(3) {
                let (x, y) = (xi as f32, yi as f32);
                if ps.near_snow(x, y, 40.0, 3.0) {
                    assert!(z.iter().any(|r| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3]), "({x},{y})");
                }
            }
        }
        // No snow, no zones.
        ps.clear();
        z.clear();
        ps.grab_zones(40.0, 3.0, &mut z);
        assert!(z.is_empty());
    }

    #[test]
    fn add_caps_and_tapers() {
        let mut p = Pile::new(1, 100.0, 300.0, 500.0, 4.0, 14.0, 1.0);
        for _ in 0..5000 {
            p.add(200.0, 5.0);
            p.add(100.5, 5.0);
        }
        p.diffuse(0.2);
        assert!(p.cols.iter().all(|&c| c <= 14.0 + 1e-3));
        assert!(p.cols[0] < p.cols[p.cols.len() / 2]);
    }

    #[test]
    fn remove_clears_range() {
        let mut p = Pile::new(GROUND_ID, 0.0, 400.0, 500.0, 4.0, 28.0, 1.0);
        p.cols.fill(5.0);
        let r = p.remove(100.0, 140.0, 2.0);
        assert!(r > 0.0);
        assert!(p.height_at(120.0) < 5.0 && p.height_at(300.0) == 5.0);
    }

    /// Ground pile on a 1080p screen after a long, even snowfall.
    fn full_ground(ps: &mut Piles, wind: f32, secs: f32) {
        let mut rng = Rng::with_seed(7);
        let steps = (secs / 0.016) as usize;
        for s in 0..steps {
            ps.update(0.016, wind, &[], Some(1000.0), 1920.0, 1080.0, &mut rng);
            for k in 0..40 {
                let x = ((s * 40 + k) * 37 % 1920) as f32 + 0.5;
                ps.absorb(x, 0.0, 1000.0, 3.0);
            }
        }
    }

    #[test]
    fn ground_snow_forms_drifts_not_a_flat_layer() {
        let mut ps = Piles::default();
        full_ground(&mut ps, 0.0, 60.0);
        let g = &ps.piles[0];
        let (lo, hi) = g.cols.iter().fold((f32::MAX, 0.0f32), |(a, b), &c| (a.min(c), b.max(c)));
        assert!(hi <= DEFAULT_DEPTH + 1e-3, "{hi}");
        assert!(hi > DEFAULT_DEPTH * 0.8, "hills reach near the maximum: {hi}");
        assert!(lo < hi * 0.7, "dips stay lower: {lo} vs {hi}");
        // Neighbouring columns stay close: smooth hills, not noise.
        let jump = g.cols.windows(2).map(|w| (w[0] - w[1]).abs()).fold(0.0, f32::max);
        assert!(jump < 1.5, "{jump}");
    }

    #[test]
    fn depth_setting_changes_the_limit_not_the_snow() {
        let mut ps = Piles::default();
        let top = |ps: &Piles| ps.piles[0].cols.iter().cloned().fold(0.0, f32::max);
        full_ground(&mut ps, 0.0, 30.0);
        let before = top(&ps);
        // Raising the maximum leaves the snow as it is; it grows with the snowfall.
        ps.set_depth(DEFAULT_DEPTH * 2.0);
        assert_eq!(top(&ps), before);
        full_ground(&mut ps, 0.0, 30.0);
        let more = top(&ps);
        assert!(more <= DEFAULT_DEPTH * 2.0 + 1e-3 && more > DEFAULT_DEPTH * 1.5, "{more}");
        // Lowering it melts the extra snow over a few seconds, not at once.
        ps.set_depth(DEFAULT_DEPTH * 0.5);
        assert!((top(&ps) - more).abs() < 1e-3);
        let mut rng = Rng::with_seed(3);
        ps.update(0.016, 0.0, &[], Some(1000.0), 1920.0, 1080.0, &mut rng);
        let early = top(&ps);
        assert!(early < more && early > more - 2.0, "{more} -> {early}");
        for _ in 0..(8.0 / 0.016) as usize {
            ps.update(0.016, 0.0, &[], Some(1000.0), 1920.0, 1080.0, &mut rng);
        }
        assert!(top(&ps) <= DEFAULT_DEPTH * 0.5 + 1e-3, "{}", top(&ps));
        ps.set_depth(1.0e9);
        assert!(ps.piles[0].caps.iter().all(|&c| c <= DEPTH_RANGE.1 + 1e-3));
    }

    #[test]
    fn a_resting_ball_presses_the_snow_under_it() {
        let mut p = Pile::new(GROUND_ID, 0.0, 400.0, 500.0, 4.0, 28.0, 1.0);
        p.cols.fill(20.0);
        // Ball of radius 30 sunk 4 px into a 20 px layer: its centre is 26 px above the top.
        let (cx, r, sink) = (200.0, 30.0, 4.0);
        let cy = 500.0 - 20.0 - r + sink;
        p.press(cx, cy, r, sink);
        assert!((p.height_at(cx) - 20.0).abs() < 0.2, "{}", p.height_at(cx));
        // Squashed under the ball next to where it stands, a rim just outside, untouched far away.
        assert!(p.height_at(cx + 12.0) < 20.0, "{}", p.height_at(cx + 12.0));
        assert!(p.height_at(cx + r + 2.0) > 20.0, "{}", p.height_at(cx + r + 2.0));
        assert_eq!(p.height_at(100.0), 20.0);
        // Snow can't grow up through the ball.
        for _ in 0..200 {
            p.add(cx, 3.0);
            p.press(cx, cy, r, sink);
        }
        assert!(p.height_at(cx) <= 20.0 + 0.2, "{}", p.height_at(cx));
    }

    #[test]
    fn wind_moves_the_drifts_downwind() {
        let mut ps = Piles::default();
        full_ground(&mut ps, 0.0, 40.0);
        let peak = |ps: &Piles| {
            let g = &ps.piles[0];
            // Highest column in the middle third, in x.
            let n = g.cols.len();
            let i = (n / 3..2 * n / 3).max_by(|&a, &b| g.cols[a].total_cmp(&g.cols[b])).unwrap();
            g.x0 + i as f32 * g.colw
        };
        let x0 = peak(&ps);
        let mut rng = Rng::with_seed(1);
        for _ in 0..(5.0 / 0.016) as usize {
            ps.update(0.016, 1.0, &[], Some(1000.0), 1920.0, 1080.0, &mut rng);
        }
        let x1 = peak(&ps);
        // 6 px/s for 5 s.
        assert!((x1 - x0 - 30.0).abs() < 8.0, "{x0} -> {x1}");
    }
}
