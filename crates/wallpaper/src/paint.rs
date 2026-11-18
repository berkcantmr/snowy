//! Tiny CPU painter: float canvas, smooth noise, anti-aliased shapes, soft glow.

pub type Rgb = [f32; 3];

pub fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub fn mul(a: Rgb, b: Rgb) -> Rgb {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

pub fn scale(a: Rgb, k: f32) -> Rgb {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ---------- noise ----------

fn hash_u(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    h
}

/// Deterministic value in 0..1 for an integer lattice point.
pub fn hash1(seed: u32, x: i32) -> f32 {
    (hash_u((x as u32).wrapping_mul(0x9e37_79b1) ^ seed.wrapping_mul(0x85eb_ca6b)) >> 8) as f32 / 16_777_216.0
}

pub fn hash2(seed: u32, x: i32, y: i32) -> f32 {
    let h = hash_u((x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca77) ^ seed.wrapping_mul(0xc2b2_ae3d));
    (hash_u(h) >> 8) as f32 / 16_777_216.0
}

fn quintic(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub fn vnoise1(seed: u32, x: f32) -> f32 {
    let i = x.floor();
    let f = quintic(x - i);
    let a = hash1(seed, i as i32);
    let b = hash1(seed, i as i32 + 1);
    a + (b - a) * f
}

pub fn fbm1(seed: u32, x: f32, oct: u32, gain: f32) -> f32 {
    let (mut sum, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0);
    for o in 0..oct {
        sum += amp * vnoise1(seed.wrapping_add(o * 101), x * f);
        norm += amp;
        amp *= gain;
        f *= 2.03;
    }
    sum / norm
}

pub fn vnoise2(seed: u32, x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (quintic(x - xi), quintic(y - yi));
    let (xi, yi) = (xi as i32, yi as i32);
    let a = hash2(seed, xi, yi);
    let b = hash2(seed, xi + 1, yi);
    let c = hash2(seed, xi, yi + 1);
    let d = hash2(seed, xi + 1, yi + 1);
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    top + (bot - top) * fy
}

pub fn fbm2(seed: u32, x: f32, y: f32, oct: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0);
    for o in 0..oct {
        sum += amp * vnoise2(seed.wrapping_add(o * 57), x * f, y * f);
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

/// Small deterministic random stream.
pub struct Rng(pub u32);

impl Rng {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f32 {
        self.0 = hash_u(self.0.wrapping_add(0x9e37_79b9));
        (self.0 >> 8) as f32 / 16_777_216.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }
}

// ---------- ridge (silhouette) ----------

/// Anti-aliased horizon line: three horizontal subsamples per pixel column.
pub struct Ridge {
    pub hs: Vec<[f32; 3]>,
}

impl Ridge {
    pub fn new(w: usize, f: impl Fn(f32) -> f32) -> Self {
        let hs = (0..w).map(|x| [f(x as f32 + 0.17), f(x as f32 + 0.5), f(x as f32 + 0.83)]).collect();
        Ridge { hs }
    }
    pub fn h(&self, x: usize) -> f32 {
        self.hs[x.min(self.hs.len() - 1)][1]
    }
    /// dh/dx over a wider window (k px each side), for broad face shading.
    pub fn slope_k(&self, x: usize, k: usize) -> f32 {
        let n = self.hs.len();
        let a = self.hs[x.saturating_sub(k)][1];
        let b = self.hs[(x + k).min(n - 1)][1];
        (b - a) / (2 * k) as f32
    }
    /// dh/dx in px/px.
    pub fn slope(&self, x: usize) -> f32 {
        let n = self.hs.len();
        let a = self.hs[x.saturating_sub(2)][1];
        let b = self.hs[(x + 2).min(n - 1)][1];
        (b - a) / 4.0
    }
}

// ---------- canvas ----------

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgb>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, px: vec![[0.0; 3]; w * h] }
    }

    #[inline]
    pub fn over(&mut self, x: usize, y: usize, c: Rgb, a: f32) {
        let p = &mut self.px[y * self.w + x];
        p[0] += (c[0] - p[0]) * a;
        p[1] += (c[1] - p[1]) * a;
        p[2] += (c[2] - p[2]) * a;
    }

    /// Light-adding blend (screen): never clips, good for glows.
    #[inline]
    pub fn screen(&mut self, x: usize, y: usize, c: Rgb, a: f32) {
        let p = &mut self.px[y * self.w + x];
        for i in 0..3 {
            p[i] = 1.0 - (1.0 - p[i]) * (1.0 - (c[i] * a).clamp(0.0, 1.0));
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Rgb {
        self.px[y * self.w + x]
    }

    /// Vertical gradient between rows y0..y1; stops are (t in 0..1, color).
    pub fn vgradient(&mut self, y0: usize, y1: usize, stops: &[(f32, Rgb)]) {
        let y1 = y1.min(self.h);
        for y in y0..y1 {
            let t = (y - y0) as f32 / ((y1 - y0).max(2) - 1) as f32;
            let mut c = stops[stops.len() - 1].1;
            for k in 0..stops.len() - 1 {
                if t <= stops[k + 1].0 {
                    let (t0, c0) = stops[k];
                    let (t1, c1) = stops[k + 1];
                    let u = (t - t0) / (t1 - t0).max(1e-4);
                    c = mix(c0, c1, smooth(0.0, 1.0, u) * 0.4 + u * 0.6);
                    break;
                }
            }
            for x in 0..self.w {
                self.px[y * self.w + x] = c;
            }
        }
    }

    fn bbox(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> (usize, usize, usize, usize) {
        let ax = (x0.floor() as i64).clamp(0, self.w as i64) as usize;
        let ay = (y0.floor() as i64).clamp(0, self.h as i64) as usize;
        let bx = (x1.ceil() as i64 + 1).clamp(0, self.w as i64) as usize;
        let by = (y1.ceil() as i64 + 1).clamp(0, self.h as i64) as usize;
        (ax, ay, bx, by)
    }

    /// Soft light: gaussian falloff of radius `r`, additive (screen).
    pub fn glow(&mut self, cx: f32, cy: f32, r: f32, col: Rgb, k: f32) {
        self.glow_e(cx, cy, r, r, col, k);
    }

    pub fn glow_e(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgb, k: f32) {
        if k <= 0.0 {
            return;
        }
        let (ax, ay, bx, by) = self.bbox(cx - rx * 2.6, cy - ry * 2.6, cx + rx * 2.6, cy + ry * 2.6);
        for y in ay..by {
            let dy = (y as f32 + 0.5 - cy) / ry;
            for x in ax..bx {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let d2 = dx * dx + dy * dy;
                if d2 < 6.8 {
                    let f = (-d2 * 0.5).exp() - 0.0011;
                    if f > 0.0 {
                        self.screen(x, y, col, k * f);
                    }
                }
            }
        }
    }

    /// Soft puff (normal blend, smooth edge).
    pub fn puff(&mut self, cx: f32, cy: f32, r: f32, col: Rgb, a: f32) {
        let (ax, ay, bx, by) = self.bbox(cx - r, cy - r, cx + r, cy + r);
        for y in ay..by {
            for x in ax..bx {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() / r;
                if d < 1.0 {
                    let f = 1.0 - smooth(0.0, 1.0, d);
                    self.over(x, y, col, a * f);
                }
            }
        }
    }

    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, col: Rgb, a: f32) {
        self.ellipse(cx, cy, r, r, col, a);
    }

    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgb, a: f32) {
        let (ax, ay, bx, by) = self.bbox(cx - rx - 1.0, cy - ry - 1.0, cx + rx + 1.0, cy + ry + 1.0);
        let m = rx.min(ry);
        for y in ay..by {
            for x in ax..bx {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let dy = (y as f32 + 0.5 - cy) / ry;
                let d = ((dx * dx + dy * dy).sqrt() - 1.0) * m;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.over(x, y, col, cov * a);
                }
            }
        }
    }

    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, col: Rgb, a: f32) {
        self.convex(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], col, a);
    }

    pub fn tri(&mut self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), col: Rgb, a: f32) {
        self.convex(&[p0, p1, p2], col, a);
    }

    /// Anti-aliased convex polygon via signed edge distances.
    pub fn convex(&mut self, pts: &[(f32, f32)], col: Rgb, a: f32) {
        let n = pts.len();
        let mut area = 0.0;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for i in 0..n {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % n];
            area += ax * by - bx * ay;
            x0 = x0.min(ax);
            y0 = y0.min(ay);
            x1 = x1.max(ax);
            y1 = y1.max(ay);
        }
        let sg = if area >= 0.0 { 1.0 } else { -1.0 };
        let mut edges = Vec::with_capacity(n);
        for i in 0..n {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % n];
            let (dx, dy) = (bx - ax, by - ay);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-4 {
                continue;
            }
            edges.push((ax, ay, dy / len * sg, -dx / len * sg));
        }
        let (ax, ay, bx, by) = self.bbox(x0 - 1.0, y0 - 1.0, x1 + 1.0, y1 + 1.0);
        for y in ay..by {
            let py = y as f32 + 0.5;
            for x in ax..bx {
                let px = x as f32 + 0.5;
                let mut d = f32::MIN;
                for &(ex, ey, nx, ny) in &edges {
                    d = d.max((px - ex) * nx + (py - ey) * ny);
                }
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.over(x, y, col, cov * a);
                }
            }
        }
    }

    /// Fills everything below `ridge` using `shade(x, y, depth_below_ridge)`.
    pub fn ridge(&mut self, r: &Ridge, shade: impl Fn(usize, usize, f32) -> Rgb) {
        for x in 0..self.w {
            let s = r.hs[x];
            let top = s[0].min(s[1]).min(s[2]);
            let y0 = top.floor().max(0.0) as usize;
            for y in y0..self.h {
                let yf = y as f32;
                let cov = ((yf + 1.0 - s[0]).clamp(0.0, 1.0) + (yf + 1.0 - s[1]).clamp(0.0, 1.0) + (yf + 1.0 - s[2]).clamp(0.0, 1.0)) / 3.0;
                if cov <= 0.0 {
                    continue;
                }
                let col = shade(x, y, (yf + 0.5 - s[1]).max(0.0));
                self.over(x, y, col, cov);
            }
        }
    }

    /// Horizontal fog band.
    pub fn fog(&mut self, yc: f32, th: f32, col: Rgb, a: f32, seed: u32) {
        let y0 = ((yc - th * 2.2).max(0.0)) as usize;
        let y1 = ((yc + th * 2.2).min(self.h as f32)) as usize;
        let sx = 1.0 / (self.h as f32 * 0.55);
        for y in y0..y1 {
            let d = (y as f32 - yc) / th;
            let base = (-d * d).exp();
            for x in 0..self.w {
                let n = fbm2(seed, x as f32 * sx, y as f32 * sx * 2.2, 3);
                let f = base * (0.25 + 1.1 * n).clamp(0.0, 1.0);
                self.over(x, y, col, a * f);
            }
        }
    }

    /// Converts to 8-bit RGBA with triangular dithering (no banding) and a faint vignette.
    pub fn to_rgba(&self, seed: u32) -> Vec<u8> {
        let mut out = vec![255u8; self.w * self.h * 4];
        let (cw, ch) = (self.w as f32 * 0.5, self.h as f32 * 0.5);
        for y in 0..self.h {
            let dy = (y as f32 - ch) / ch;
            for x in 0..self.w {
                let dx = (x as f32 - cw) / cw;
                let vig = 1.0 - 0.10 * smooth(0.45, 1.5, (dx * dx * 0.8 + dy * dy).sqrt());
                let p = self.px[y * self.w + x];
                let i = (y * self.w + x) * 4;
                let n = hash2(seed ^ 0xA5A5, x as i32, y as i32) - hash2(seed ^ 0x5A5A, x as i32, y as i32);
                for k in 0..3 {
                    let v = p[k].clamp(0.0, 1.0) * vig * 255.0 + n * 0.9 + 0.5;
                    out[i + k] = v.clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    }
}
