//! Snow accumulation: per-ledge column heightmaps, plus falling clumps when a ledge goes away.

use crate::util::*;
use fastrand::Rng;
use snowy_core::{Instance, Surface};

/// Cap on instances emitted for piles.
pub const MAX_PILE_INSTANCES: usize = 6000;
/// Pseudo surface id of the ground pile.
pub const GROUND_ID: u64 = u64::MAX;
const MAX_CLUMPS: usize = 800;

pub struct Pile {
    pub id: u64,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    pub colw: f32,
    pub max_h: f32,
    pub ground: bool,
    pub cols: Vec<f32>,
}

impl Pile {
    fn new(id: u64, x0: f32, x1: f32, y: f32, cw: f32, max_h: f32) -> Pile {
        let n = (((x1 - x0) / cw).round() as usize).max(1);
        Pile {
            id,
            x0,
            x1,
            y,
            colw: (x1 - x0) / n as f32,
            max_h,
            ground: id == GROUND_ID,
            cols: vec![0.0; n],
        }
    }

    pub fn col(&self, x: f32) -> usize {
        let n = self.cols.len() as isize;
        (((x - self.x0) / self.colw) as isize).clamp(0, n - 1) as usize
    }

    /// Maximum height of column `i` (ledge piles taper towards their ends).
    fn cap(&self, i: usize) -> f32 {
        if self.ground {
            return self.max_h;
        }
        let n = self.cols.len();
        let d = (i as f32 + 0.5).min(n as f32 - i as f32 - 0.5);
        let t = (d / 4.0).clamp(0.0, 1.0);
        self.max_h * (0.15 + 0.85 * t * t * (3.0 - 2.0 * t))
    }

    pub fn height_at(&self, x: f32) -> f32 {
        self.cols[self.col(x)]
    }

    /// Y of the snow surface at x.
    pub fn top_at(&self, x: f32) -> f32 {
        self.y - self.height_at(x)
    }

    /// Add snow around x. Growth slows as a column approaches its cap.
    pub fn add(&mut self, x: f32, amt: f32) {
        let i = self.col(x);
        let n = self.cols.len();
        for (j, wgt) in [(i as isize, 0.6), (i as isize - 1, 0.2), (i as isize + 1, 0.2)] {
            if j < 0 || j >= n as isize {
                continue;
            }
            let j = j as usize;
            let c = self.cap(j);
            let room = (1.0 - self.cols[j] / c).max(0.0);
            self.cols[j] = (self.cols[j] + amt * wgt * room).min(c);
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

    fn diffuse(&mut self, k: f32) {
        let n = self.cols.len();
        if n > 1 {
            let mut prev = self.cols[0];
            for i in 0..n {
                let cur = self.cols[i];
                let next = if i + 1 < n { self.cols[i + 1] } else { cur };
                let left = if i == 0 { cur } else { prev };
                self.cols[i] = cur + k * (left + next - 2.0 * cur);
                prev = cur;
            }
        }
        if !self.ground {
            for i in (0..n.min(5)).chain(n.saturating_sub(5)..n) {
                let c = self.cap(i);
                self.cols[i] = self.cols[i].min(c);
            }
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

    pub fn update(&mut self, dt: f32, surfaces: &[Surface], ground: Option<f32>, w: f32, h: f32, rng: &mut Rng) {
        let sc = ui_scale(w, h);
        if self.last.as_slice() != surfaces || self.last_ground != ground || self.dims != (w, h) {
            self.reconcile(surfaces, ground, w, h, rng);
        }
        let k = (dt * 1.5).min(0.2);
        for p in &mut self.piles {
            p.diffuse(k);
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
            let max_h = if is_ground { 28.0 } else { 14.0 } * sc;
            let mut np = Pile::new(s.id, s.x0, s.x1, s.y, cw, max_h);
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
        let mut p = Pile::new(1, 100.0, 300.0, 500.0, 4.0, 14.0);
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
        let mut p = Pile::new(1, 100.0, 300.0, 500.0, 4.0, 14.0);
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
        let mut p = Pile::new(GROUND_ID, 0.0, 400.0, 500.0, 4.0, 28.0);
        p.cols.fill(5.0);
        let r = p.remove(100.0, 140.0, 2.0);
        assert!(r > 0.0);
        assert!(p.height_at(120.0) < 5.0 && p.height_at(300.0) == 5.0);
    }
}
