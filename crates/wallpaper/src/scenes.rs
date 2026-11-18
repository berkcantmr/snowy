//! The six painted scenes. Everything is procedural and deterministic.
//! Composition rule: the left third stays calm (low contrast, no buildings) so desktop
//! icons remain readable; focal points sit in the right two thirds.

use crate::paint::*;
use crate::palette::{lit, Pal};
use crate::Theme;
use std::f32::consts::PI;

#[derive(Clone, Copy)]
struct G {
    w: f32,
    h: f32,
    /// Scale relative to a 1080 px tall canvas.
    s: f32,
}

fn geo(c: &Canvas) -> G {
    G { w: c.w as f32, h: c.h as f32, s: c.h as f32 / 1080.0 }
}

pub fn paint(c: &mut Canvas, theme: Theme, p: &Pal, seed: u32) {
    match theme {
        Theme::SnowyHills => snowy_hills(c, p, seed),
        Theme::PineForest => pine_forest(c, p, seed),
        Theme::FrozenLake => frozen_lake(c, p, seed),
        Theme::Mountains => mountains(c, p, seed),
        Theme::StarryVillage => starry_village(c, p, seed),
        Theme::Aurora => aurora_tundra(c, p, seed),
    }
}

// ---------------------------------------------------------------- sky

fn ambient(p: &Pal, l: f32) -> Rgb {
    mix(p.snow_shadow, p.snow_light, l)
}

fn sky(c: &mut Canvas, p: &Pal, horizon: f32, aurora_k: f32, seed: u32) {
    let g = geo(c);
    let hy = (g.h * horizon) as usize;
    c.vgradient(0, hy.max(2), &[(0.0, p.top), (0.55, p.mid), (1.0, p.hor)]);
    c.vgradient(hy.max(2), c.h, &[(0.0, p.hor), (1.0, p.hor)]);

    stars(c, p, g.h * horizon, seed);
    aurora(c, g.h * horizon, p.aurora * aurora_k, seed);

    // Sun
    if p.sun_alpha > 0.0 {
        let (sx, sy) = (p.sun.0 * g.w, p.sun.1 * g.h);
        let a = p.sun_alpha;
        c.glow_e(sx, sy, 520.0 * g.s, 340.0 * g.s, p.glow_col, p.glow_k * 0.42 * a);
        c.glow(sx, sy, 150.0 * g.s, p.glow_col, p.glow_k * 0.55 * a);
        c.glow(sx, sy, p.sun_r * g.s * 2.0, p.sun_col, 0.75 * a);
        c.disc(sx, sy, p.sun_r * g.s, p.sun_col, a);
    }
    // Moon
    if p.moon_alpha > 0.0 {
        let (mx, my) = (p.moon.0 * g.w, p.moon.1 * g.h);
        let a = p.moon_alpha;
        let r = 30.0 * g.s;
        c.glow(mx, my, 300.0 * g.s, [0.35, 0.5, 0.9], 0.22 * a);
        c.glow(mx, my, 95.0 * g.s, [0.55, 0.68, 1.0], 0.38 * a);
        c.disc(mx, my, r, [0.93, 0.94, 0.90], a);
        // soft maria
        for (dx, dy, rr) in [(-0.3, -0.25, 0.32), (0.25, 0.1, 0.4), (-0.1, 0.4, 0.22), (0.35, -0.4, 0.18)] {
            c.disc(mx + dx * r, my + dy * r, rr * r, [0.70, 0.74, 0.80], 0.22 * a);
        }
        c.glow(mx, my, r * 1.5, [0.85, 0.9, 1.0], 0.25 * a);
    }
}

fn stars(c: &mut Canvas, p: &Pal, hor_y: f32, seed: u32) {
    if p.stars <= 0.01 {
        return;
    }
    let g = geo(c);
    let n = ((420.0 * g.w * g.h / (1920.0 * 1080.0)) as usize).max(24);
    let mut rng = Rng(seed ^ 0x57A5);
    for _ in 0..n {
        let x = rng.next() * g.w;
        let y = hor_y * rng.next().powf(0.85) * 0.97;
        let b = rng.next();
        let tint = rng.next();
        let col = mix([0.78, 0.86, 1.0], [1.0, 0.94, 0.80], tint);
        let fade = 1.0 - smooth(0.5, 1.0, y / hor_y);
        let k = p.stars * (0.18 + 0.82 * b * b) * fade;
        let r = ((0.5 + b * 0.9) * g.s.max(0.55)).max(0.55);
        c.glow(x, y, r, col, k);
        if b > 0.9 {
            c.glow(x, y, r * 3.2, col, k * 0.22);
        }
    }
}

fn aurora(c: &mut Canvas, hor_y: f32, k: f32, seed: u32) {
    if k < 0.03 {
        return;
    }
    let g = geo(c);
    const N: usize = 3;
    let w = c.w;
    let mut yc = vec![[0.0f32; N]; w];
    let mut len = vec![[0.0f32; N]; w];
    let mut amp = vec![[0.0f32; N]; w];
    for x in 0..w {
        let u = x as f32 / g.w;
        let calm = 0.5 + 0.5 * smooth(0.05, 0.55, u);
        for i in 0..N {
            let sd = seed.wrapping_add(i as u32 * 313);
            let fi = i as f32;
            let wave = (u * (2.6 + fi * 0.9) * PI + fi * 1.7).sin() * 0.07 + (u * (5.3 + fi) * PI + fi).sin() * 0.025;
            yc[x][i] = hor_y * (0.52 + fi * 0.11) + g.h * wave + g.h * 0.05 * (fbm1(sd, u * 3.0, 3, 0.5) - 0.5);
            len[x][i] = g.h * (0.16 + 0.10 * fbm1(sd ^ 0x1111, u * 2.0, 2, 0.5)) * (1.0 - fi * 0.12);
            let env = smooth(0.25, 0.6, fbm1(sd ^ 0x2222, u * 1.6 + fi * 3.0, 2, 0.5) + 0.12);
            let ray = 0.55 + 0.45 * fbm1(sd ^ 0x3333, u * 55.0, 2, 0.6);
            amp[x][i] = env * ray * calm * (1.0 - fi * 0.22);
        }
    }
    let green = [0.30, 1.0, 0.58];
    let teal = [0.20, 0.85, 0.85];
    let violet = [0.62, 0.34, 0.95];
    let edge = 9.0 * g.s;
    for y in 0..(hor_y as usize).min(c.h) {
        let yf = y as f32 + 0.5;
        for x in 0..w {
            let mut sum = [0.0f32; 3];
            for i in 0..N {
                let d = yc[x][i] - yf;
                let a = amp[x][i];
                if a < 0.01 {
                    continue;
                }
                let (v, t) = if d >= 0.0 {
                    let t = d / len[x][i];
                    ((-t * 1.15).exp(), t)
                } else {
                    ((-(d * d) / (edge * edge)).exp(), 0.0)
                };
                if v < 0.01 {
                    continue;
                }
                let col = if t < 0.6 { mix(green, teal, t / 0.6) } else { mix(teal, violet, ((t - 0.6) / 1.2).min(1.0)) };
                let f = v * a;
                for q in 0..3 {
                    sum[q] += col[q] * f;
                }
            }
            if sum[0] + sum[1] + sum[2] > 0.004 {
                let f = 0.78 * k;
                c.screen(x, y, [sum[0], sum[1], sum[2]], f);
            }
        }
    }
}

// ---------------------------------------------------------------- hills

fn hill_fn(g: G, seed: u32, base: f32, amp: f32, freq: f32, phase: f32) -> impl Fn(f32) -> f32 {
    move |x| g.h * base - g.h * amp * (fbm1(seed, x / g.w * freq + phase, 3, 0.42) - 0.5) * 2.0
}

fn peaks_fn(g: G, seed: u32, base: f32, peaks: &'static [(f32, f32, f32)], detail: f32) -> impl Fn(f32) -> f32 {
    move |x| {
        let u = x / g.w;
        let mut best = 0.0f32;
        for &(pu, ph, pw) in peaks {
            let t = (1.0 - (u - pu).abs() / pw).max(0.0);
            let rough = (fbm1(seed, x * 0.011 / g.s.max(0.2), 4, 0.55) - 0.5) * detail * g.s;
            best = best.max(ph * g.h * t.powf(1.12) + rough * t);
        }
        g.h * base - best
    }
}

fn snow_layer(c: &mut Canvas, p: &Pal, r: &Ridge, haze: f32, fade: f32, gain: f32, seed: u32) {
    let w = c.w as f32;
    let tops: Vec<Rgb> = (0..c.w)
        .map(|x| {
            let l = lit(p, r.slope(x), x as f32, w, gain);
            let t = mix(p.snow_shadow, p.snow_light, l);
            mix(t, p.warm, p.warm_amt * 0.30 * l * l)
        })
        .collect();
    let sx = 1.0 / (c.h as f32 * 0.4);
    c.ridge(r, |x, y, d| {
        let top = tops[x];
        let deep = mix(top, p.snow_shadow, 0.55);
        let t = smooth(0.0, fade, d);
        let n = vnoise2(seed, x as f32 * sx, y as f32 * sx * 3.0) - 0.5;
        let col = mix(top, deep, t * 0.85);
        let col = [col[0] + n * 0.018, col[1] + n * 0.018, col[2] + n * 0.022];
        mix(col, p.haze, haze)
    });
}

fn flat_layer(c: &mut Canvas, r: &Ridge, col: Rgb, bottom: Rgb, fade: f32) {
    c.ridge(r, |_, _, d| mix(col, bottom, smooth(0.0, fade, d)));
}

// ---------------------------------------------------------------- trees & buildings

fn pine_sil(c: &mut Canvas, x: f32, yb: f32, hgt: f32, col: Rgb, tiers: usize) {
    let th = hgt * 0.44;
    let step = (hgt * 0.9 - th) / (tiers - 1) as f32;
    let ytop = yb - hgt;
    c.rect(x - hgt * 0.03, yb - hgt * 0.12, x + hgt * 0.03, yb + hgt * 0.03, col, 1.0);
    for i in 0..tiers {
        let f = i as f32 / (tiers - 1) as f32;
        let apex = ytop + i as f32 * step;
        let hw = hgt * (0.09 + 0.17 * f);
        c.tri((x, apex), (x - hw, apex + th), (x + hw, apex + th), col, 1.0);
    }
}

fn pine_snow(c: &mut Canvas, p: &Pal, x: f32, yb: f32, hgt: f32, haze: f32) {
    let g = geo(c);
    let body = mix(p.tree, p.haze, haze);
    let tiers = 5;
    let th = hgt * 0.44;
    let step = (hgt * 0.9 - th) / (tiers - 1) as f32;
    let ytop = yb - hgt;
    let dir = if p.light_x * g.w >= x { 1.0 } else { -1.0 };
    let (l_lit, r_lit) = if dir > 0.0 { (0.38, 0.98) } else { (0.98, 0.38) };
    let side = |l: f32| mix(mix(ambient(p, l), p.warm, p.warm_amt * 0.25 * l), p.haze, haze);
    let trunk = mix(scale(mul([0.35, 0.22, 0.15], ambient(p, 0.4)), 1.0), p.haze, haze);
    c.rect(x - hgt * 0.025, yb - hgt * 0.12, x + hgt * 0.025, yb + hgt * 0.02, trunk, 1.0);
    for i in 0..tiers {
        let f = i as f32 / (tiers - 1) as f32;
        let apex = ytop + i as f32 * step;
        let hw = hgt * (0.09 + 0.17 * f);
        c.tri((x, apex), (x - hw, apex + th), (x + hw, apex + th), body, 1.0);
        let k = 0.62;
        let (sx, sy) = (hw * k, th * k);
        c.tri((x, apex), (x - sx, apex + sy), (x + sx, apex + sy), side(0.7), 1.0);
        c.tri((x, apex), (x - sx, apex + sy), (x, apex + sy * 0.92), side(l_lit), 1.0);
        c.tri((x, apex), (x, apex + sy * 0.92), (x + sx, apex + sy), side(r_lit), 1.0);
    }
}

/// Front-facing gable house. Returns the chimney top position.
#[allow(clippy::too_many_arguments)]
fn house(c: &mut Canvas, p: &Pal, x: f32, yb: f32, wd: f32, ht: f32, rh: f32, wall: Rgb, haze: f32, windows: usize) -> (f32, f32) {
    let g = geo(c);
    let dir = if p.light_x * g.w >= x { 1.0 } else { -1.0 };
    let wall_c = mix(mul(wall, scale(ambient(p, 0.5), 1.2)), p.haze, haze);
    let ov = wd * 0.09;
    let (lx, rx) = (x - wd * 0.5 - ov, x + wd * 0.5 + ov);
    let ey = yb - ht;
    let apex = (x, ey - rh);

    // chimney (behind the roof)
    let cx = x + wd * 0.22;
    let roof_y = ey - rh * (1.0 - (cx - x) / (wd * 0.5 + ov));
    let ctop = roof_y - rh * 0.5;
    let brick = mix(mul([0.45, 0.28, 0.25], ambient(p, 0.45)), p.haze, haze);
    c.rect(cx - wd * 0.05, ctop, cx + wd * 0.05, roof_y + rh * 0.15, brick, 1.0);
    c.rect(cx - wd * 0.065, ctop - rh * 0.06, cx + wd * 0.065, ctop + rh * 0.03, mix(ambient(p, 0.85), p.haze, haze), 1.0);

    c.rect(x - wd * 0.5, ey, x + wd * 0.5, yb, wall_c, 1.0);
    // roof
    let (ll, rl) = if dir > 0.0 { (0.42, 0.98) } else { (0.98, 0.42) };
    let tint = |l: f32| mix(mix(ambient(p, l), p.warm, p.warm_amt * 0.3 * l), p.haze, haze);
    c.tri((lx, ey), apex, (x, ey), tint(ll), 1.0);
    c.tri(apex, (rx, ey), (x, ey), tint(rl), 1.0);
    c.rect(lx, ey, rx, ey + ht * 0.07, mix(scale(wall_c, 0.55), p.haze, 0.0), 1.0);
    // windows & door
    let on = p.windows;
    let dark = scale(mul(wall, ambient(p, 0.2)), 0.35);
    let lamp = [1.0, 0.78, 0.40];
    let ww = wd * 0.17;
    let wy = yb - ht * 0.62;
    for i in 0..windows {
        let wx = if windows == 1 { x } else { x - wd * 0.27 + i as f32 * wd * 0.54 / (windows - 1) as f32 };
        let col = mix(mix(dark, [0.55, 0.65, 0.80], 0.25 * (1.0 - on)), lamp, on);
        c.rect(wx - ww * 0.5, wy, wx + ww * 0.5, wy + ww * 1.15, mix(col, p.haze, haze * 0.5), 1.0);
        c.rect(wx - ww * 0.5 - 1.0, wy + ww * 1.15, wx + ww * 0.5 + 1.0, wy + ww * 1.15 + ww * 0.12, ambient(p, 0.9), 0.9);
        if on > 0.02 {
            c.glow(wx, wy + ww * 0.6, ww * 2.6, [1.0, 0.62, 0.25], 0.55 * on * (1.0 - haze));
            c.glow_e(wx, yb + ht * 0.05, ww * 2.4, ww * 0.7, [1.0, 0.65, 0.28], 0.22 * on * (1.0 - haze));
        }
    }
    let door = scale(wall_c, 0.62);
    if windows == 1 {
        c.rect(x + wd * 0.28, yb - ht * 0.55, x + wd * 0.42, yb, door, 1.0);
    } else {
        c.rect(x - ww * 0.4, yb - ht * 0.5, x + ww * 0.4, yb, door, 1.0);
    }
    (cx, ctop - rh * 0.06)
}

fn smoke(c: &mut Canvas, p: &Pal, x: f32, y: f32, size: f32, seed: u32, haze: f32) {
    let col = mix(mix(p.haze, p.snow_light, 0.55), p.haze, haze);
    let n = 11;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let sway = (vnoise1(seed, t * 2.5 + x * 0.01) - 0.5) * 70.0 * size * t + t * 40.0 * size;
        let px = x + sway;
        let py = y - t * 150.0 * size;
        c.puff(px, py, (9.0 + t * 30.0) * size, col, (1.0 - t) * 0.30 * (1.0 - haze * 0.5));
    }
}

// ---------------------------------------------------------------- scenes

fn snowy_hills(c: &mut Canvas, p: &Pal, seed: u32) {
    let g = geo(c);
    sky(c, p, 0.58, 0.0, seed);
    let far = Ridge::new(c.w, |x| {
        let n = fbm1(seed ^ 1, x / g.w * 3.2 + 4.0, 4, 0.55);
        g.h * 0.50 - g.h * 0.15 * (1.0 - (2.0 * n - 1.0).abs()).powf(1.6) * (0.4 + 0.6 * smooth(0.0, 0.7, x / g.w))
    });
    mountain_shade(c, p, &far, 0.72, 0.50, seed ^ 31);

    let h1 = Ridge::new(c.w, hill_fn(g, seed ^ 2, 0.57, 0.10, 1.7, 3.0));
    snow_layer(c, p, &h1, 0.5, 90.0 * g.s, 1.0, seed ^ 5);
    let mut rng = Rng(seed ^ 77);
    for i in 0..16 {
        let u = 0.34 + 0.66 * (i as f32 + rng.next() * 0.8) / 16.0;
        let x = u * g.w;
        pine_snow(c, p, x, h1.h(x as usize) + 6.0 * g.s, rng.range(70.0, 120.0) * g.s, 0.5);
    }

    let h2 = Ridge::new(c.w, |x| {
        let u = x / g.w;
        hill_fn(g, seed ^ 3, 0.70, 0.08, 1.5, 1.0)(x) - g.h * 0.05 * smooth(0.45, 0.72, u) * (1.0 - smooth(0.82, 1.0, u))
    });
    snow_layer(c, p, &h2, 0.25, 110.0 * g.s, 1.0, seed ^ 6);
    let cx = g.w * 0.66;
    let cy = h2.h(cx as usize) + 4.0 * g.s;
    for (du, sz) in [(-0.09, 170.0), (-0.065, 125.0), (0.09, 190.0), (0.115, 140.0)] {
        let x = cx + du * g.w;
        pine_snow(c, p, x, h2.h(x as usize) + 6.0 * g.s, sz * g.s, 0.15);
    }
    let (chx, chy) = house(c, p, cx, cy, 230.0 * g.s, 105.0 * g.s, 88.0 * g.s, [0.55, 0.32, 0.20], 0.1, 1);
    smoke(c, p, chx, chy, 1.5 * g.s, seed ^ 9, 0.1);
    for i in 0..8 {
        let u = 0.40 + 0.025 * i as f32 + rng.next() * 0.03;
        let x = u * g.w;
        if (x - cx).abs() > 220.0 * g.s {
            pine_snow(c, p, x, h2.h(x as usize) + 6.0 * g.s, rng.range(90.0, 150.0) * g.s, 0.2);
        }
    }

    let h3 = Ridge::new(c.w, hill_fn(g, seed ^ 4, 0.86, 0.07, 1.2, 7.0));
    snow_layer(c, p, &h3, 0.06, 140.0 * g.s, 1.0, seed ^ 7);
    for (u, sz) in [(0.80, 280.0), (0.90, 400.0), (0.98, 320.0)] {
        let x = u * g.w;
        pine_snow(c, p, x, h3.h(x as usize) + 14.0 * g.s, sz * g.s, 0.0);
    }
    let h4 = Ridge::new(c.w, hill_fn(g, seed ^ 8, 0.97, 0.03, 1.0, 11.0));
    snow_layer(c, p, &h4, 0.0, 60.0 * g.s, 1.0, seed ^ 10);
}

#[allow(clippy::too_many_arguments)]
fn forest_line(c: &mut Canvas, p: &Pal, r: &Ridge, seed: u32, col: Rgb, spacing: f32, hmin: f32, hmax: f32, tiers: usize, calm: bool) {
    let g = geo(c);
    let mut rng = Rng(seed);
    let mut x = -spacing;
    while x < g.w + spacing {
        let u = (x / g.w).clamp(0.0, 1.0);
        let m = if calm { 0.55 + 0.45 * smooth(0.08, 0.5, u) } else { 1.0 };
        let hh = rng.range(hmin, hmax) * g.s * m;
        pine_sil(c, x, r.h(x.max(0.0) as usize) + hh * 0.08, hh, col, tiers);
        x += spacing * rng.range(0.6, 1.4);
    }
    let _ = p;
}

fn pine_forest(c: &mut Canvas, p: &Pal, seed: u32) {
    let g = geo(c);
    sky(c, p, 0.7, 0.0, seed);
    let hazes = [0.84, 0.68, 0.5, 0.3, 0.1];
    let bases = [0.50, 0.58, 0.67, 0.77, 0.88];
    let heights = [(50.0, 92.0), (80.0, 150.0), (120.0, 220.0), (170.0, 300.0), (240.0, 420.0)];
    let spacing = [13.0, 22.0, 34.0, 52.0, 84.0];
    for i in 0..5 {
        let r = Ridge::new(c.w, hill_fn(g, seed ^ (10 + i as u32), bases[i], 0.03, 1.4 + i as f32 * 0.3, i as f32 * 5.0));
        let col = mix(p.tree, p.haze, hazes[i]);
        let col = mix(col, p.warm, p.warm_amt * 0.08 * (1.0 - hazes[i]));
        flat_layer(c, &r, col, mix(col, p.haze, 0.25), 140.0 * g.s);
        forest_line(c, p, &r, seed ^ (50 + i as u32), col, spacing[i] * g.s, heights[i].0, heights[i].1, if i < 2 { 4 } else { 5 }, true);
        if i < 4 {
            c.fog(bases[i] * g.h + 70.0 * g.s, (26.0 + 10.0 * i as f32) * g.s, mix(p.haze, p.snow_light, 0.35), 0.55 - i as f32 * 0.07, seed ^ (90 + i as u32));
        }
    }
    // snowy ground and framing pines on the right
    let ground = Ridge::new(c.w, hill_fn(g, seed ^ 99, 0.945, 0.02, 1.0, 2.0));
    for (u, sz) in [(0.9, 330.0), (0.975, 420.0)] {
        let x = u * g.w;
        pine_snow(c, p, x, ground.h(x as usize) + 20.0 * g.s, sz * g.s, 0.0);
    }
    snow_layer(c, p, &ground, 0.0, 60.0 * g.s, 1.0, seed ^ 100);
}

fn frozen_lake(c: &mut Canvas, p: &Pal, seed: u32) {
    let g = geo(c);
    let hy = 0.60;
    sky(c, p, hy, 0.5, seed);
    const FAR: [(f32, f32, f32); 4] = [(0.18, 0.20, 0.20), (0.45, 0.30, 0.22), (0.70, 0.26, 0.2), (0.95, 0.18, 0.16)];
    const NEAR: [(f32, f32, f32); 3] = [(0.32, 0.12, 0.2), (0.62, 0.17, 0.22), (0.88, 0.14, 0.2)];
    let far = Ridge::new(c.w, peaks_fn(g, seed ^ 3, 0.575, &FAR, 40.0));
    mountain_shade(c, p, &far, 0.75, 0.575, seed ^ 4);
    let near = Ridge::new(c.w, peaks_fn(g, seed ^ 5, 0.60, &NEAR, 30.0));
    mountain_shade(c, p, &near, 0.45, 0.60, seed ^ 6);
    // distant treeline
    let tl = Ridge::new(c.w, hill_fn(g, seed ^ 7, 0.598, 0.004, 3.0, 0.0));
    let tcol = mix(p.tree, p.haze, 0.45);
    flat_layer(c, &tl, tcol, tcol, 10.0);
    forest_line(c, p, &tl, seed ^ 8, tcol, 9.0 * g.s, 12.0, 26.0, 4, true);
    // far shore cabin
    let cx = g.w * 0.64;
    house(c, p, cx, g.h * hy + 2.0 * g.s, 34.0 * g.s, 15.0 * g.s, 13.0 * g.s, [0.5, 0.3, 0.2], 0.35, 1);
    c.fog(g.h * hy - 2.0 * g.s, 14.0 * g.s, mix(p.haze, p.snow_light, 0.4), 0.55, seed ^ 12);

    // lake: mirrored sky and mountains, rippled, with an icy sheen
    let y0 = (g.h * hy) as usize;
    for y in y0..c.h {
        let dy = (y - y0) as f32;
        let k = dy / (g.h - g.h * hy);
        let src = (y0 as f32 - 1.0 - dy * 0.82).max(0.0);
        let row_wob = (fbm1(seed ^ 21, y as f32 * 0.035, 3, 0.5) - 0.5) * 22.0 * g.s * (0.25 + k);
        let deep = mix(mix(p.mid, p.top, 0.4), p.tree, 0.45);
        for x in 0..c.w {
            let sx = ((x as f32 + row_wob).round() as i64).clamp(0, c.w as i64 - 1) as usize;
            let sy = src as usize;
            let a = c.get(sx, sy);
            let b = c.get(((sx + 3).min(c.w - 1)).max(0), sy);
            let r = mix(a, b, 0.5);
            let col = mix(r, deep, 0.18 + 0.42 * k.min(1.0));
            let n = fbm2(seed ^ 22, x as f32 * 0.004, y as f32 * 0.06, 3);
            let sheen = (n - 0.42).max(0.0).powi(2) * 2.6 * (1.0 - 0.55 * k);
            let col = mix(col, p.snow_light, sheen * 0.38);
            c.px[y * c.w + x] = col;
        }
    }
    if p.sun_alpha > 0.0 {
        let sy = p.sun.1 * g.h;
        c.glow_e(p.sun.0 * g.w, g.h * hy + (g.h * hy - sy) * 0.8, 70.0 * g.s, 220.0 * g.s, p.glow_col, 0.45 * p.sun_alpha);
    }
    if p.moon_alpha > 0.0 {
        let sy = p.moon.1 * g.h;
        c.glow_e(p.moon.0 * g.w, g.h * hy + (g.h * hy - sy) * 0.8, 40.0 * g.s, 200.0 * g.s, [0.6, 0.72, 1.0], 0.4 * p.moon_alpha);
    }
    // shore banks
    let right = Ridge::new(c.w, |x| {
        let u = x / g.w;
        g.h * 0.985 - g.h * 0.24 * smooth(0.62, 1.04, u).powf(1.6) + g.h * 0.01 * (fbm1(seed ^ 31, u * 6.0, 2, 0.5) - 0.5)
    });
    snow_layer(c, p, &right, 0.0, 70.0 * g.s, 1.0, seed ^ 32);
    let mut rng = Rng(seed ^ 33);
    for i in 0..4 {
        let u = 0.86 + 0.045 * i as f32 + rng.next() * 0.02;
        let x = (u * g.w).min(g.w - 4.0);
        pine_snow(c, p, x, right.h(x as usize) + 8.0 * g.s, rng.range(170.0, 300.0) * g.s, 0.0);
    }
    snow_layer(c, p, &right, 0.0, 70.0 * g.s, 1.0, seed ^ 32);
    let left = Ridge::new(c.w, |x| {
        let u = x / g.w;
        g.h * 1.02 - g.h * 0.07 * (1.0 - smooth(0.0, 0.3, u)).powf(1.5)
    });
    snow_layer(c, p, &left, 0.0, 60.0 * g.s, 1.0, seed ^ 34);
}

fn mountain_shade(c: &mut Canvas, p: &Pal, r: &Ridge, haze: f32, base: f32, seed: u32) {
    let g = geo(c);
    let w = g.w;
    let cols: Vec<f32> = (0..c.w).map(|x| lit(p, r.slope_k(x, (34.0 * g.s).max(2.0) as usize), x as f32, w, 0.3)).collect();
    let rock_lit = [0.50, 0.50, 0.62];
    let rock_dark = [0.22, 0.25, 0.40];
    let base_y = g.h * base;
    let sx = 1.0 / (g.h * 0.33);
    c.ridge(r, |x, y, d| {
        let gully = fbm2(seed ^ 13, x as f32 * sx * 2.0, y as f32 * sx * 0.5, 3);
        let xe = (x as f32 + d * 0.5 + (gully - 0.5) * 90.0 * g.s).clamp(0.0, g.w - 1.0) as usize;
        let l = (cols[xe] + (gully - 0.5) * 0.25).clamp(0.0, 1.0);
        let elev = base_y - y as f32;
        let n = fbm2(seed, x as f32 * sx * 0.9, y as f32 * sx * 0.55, 3);
        let streak = fbm2(seed ^ 9, x as f32 * sx * 2.2 + y as f32 * sx * 0.9, y as f32 * sx * 2.0, 3);
        let snow_line = g.h * 0.09 + (n - 0.5) * g.h * 0.16;
        let snow_m = smooth(-g.h * 0.012, g.h * 0.012, elev - snow_line);
        let snow = ambient(p, l);
        let rock = mul(mix(rock_dark, rock_lit, l), scale(ambient(p, 0.55), 1.25));
        let mut col = mix(rock, snow, snow_m);
        col = scale(col, 0.88 + 0.24 * streak);
        col = mix(col, p.warm, p.warm_amt * 0.5 * l * l * (0.4 + 0.6 * smooth(0.0, g.h * 0.2, elev)));
        col = mix(col, p.haze, 0.55 * (1.0 - smooth(0.0, g.h * 0.2, elev)));
        mix(col, p.haze, haze)
    });
}

fn mountains(c: &mut Canvas, p: &Pal, seed: u32) {
    let g = geo(c);
    sky(c, p, 0.64, 0.4, seed);
    const FAR: [(f32, f32, f32); 5] = [(0.10, 0.13, 0.16), (0.30, 0.22, 0.18), (0.58, 0.19, 0.16), (0.80, 0.26, 0.2), (1.0, 0.18, 0.16)];
    const MAIN: [(f32, f32, f32); 4] = [(0.24, 0.24, 0.24), (0.52, 0.48, 0.28), (0.77, 0.34, 0.24), (0.97, 0.30, 0.2)];
    let far = Ridge::new(c.w, peaks_fn(g, seed ^ 1, 0.68, &FAR, 36.0));
    mountain_shade(c, p, &far, 0.62, 0.68, seed ^ 2);
    c.fog(g.h * 0.68, 26.0 * g.s, mix(p.haze, p.snow_light, 0.3), 0.45, seed ^ 3);
    let main = Ridge::new(c.w, peaks_fn(g, seed ^ 4, 0.76, &MAIN, 70.0));
    mountain_shade(c, p, &main, 0.12, 0.76, seed ^ 5);
    c.fog(g.h * 0.77, 30.0 * g.s, mix(p.haze, p.snow_light, 0.7), 0.5, seed ^ 6);
    // forested foothills
    let foot = Ridge::new(c.w, hill_fn(g, seed ^ 7, 0.835, 0.04, 1.6, 2.0));
    let col = mix(p.tree, p.haze, 0.3);
    flat_layer(c, &foot, col, col, 50.0);
    forest_line(c, p, &foot, seed ^ 8, col, 20.0 * g.s, 60.0, 120.0, 4, true);
    c.fog(g.h * 0.85, 28.0 * g.s, mix(p.haze, p.snow_light, 0.5), 0.35, seed ^ 9);
    let front = Ridge::new(c.w, hill_fn(g, seed ^ 10, 0.90, 0.04, 1.2, 6.0));
    snow_layer(c, p, &front, 0.0, 90.0 * g.s, 1.0, seed ^ 11);
    for (u, sz) in [(0.88, 300.0), (0.96, 400.0), (0.80, 210.0)] {
        let x = u * g.w;
        pine_snow(c, p, x, front.h(x as usize) + 10.0 * g.s, sz * g.s, 0.0);
    }
}

fn starry_village(c: &mut Canvas, p: &Pal, seed: u32) {
    let g = geo(c);
    sky(c, p, 0.66, 0.3, seed);
    let far = Ridge::new(c.w, hill_fn(g, seed ^ 1, 0.60, 0.06, 1.3, 2.0));
    snow_layer(c, p, &far, 0.6, 80.0 * g.s, 1.0, seed ^ 2);
    let mut rng = Rng(seed ^ 3);
    for i in 0..18 {
        let x = (0.3 + 0.7 * (i as f32 + rng.next()) / 18.0) * g.w;
        pine_snow(c, p, x, far.h(x as usize) + 4.0 * g.s, rng.range(28.0, 50.0) * g.s, 0.6);
    }
    // back row of tiny houses
    let mid = Ridge::new(c.w, hill_fn(g, seed ^ 4, 0.70, 0.04, 1.4, 5.0));
    snow_layer(c, p, &mid, 0.3, 70.0 * g.s, 1.0, seed ^ 5);
    let walls = [[0.62, 0.38, 0.30], [0.50, 0.36, 0.30], [0.66, 0.5, 0.34], [0.48, 0.32, 0.36]];
    let mut chimneys = Vec::new();
    for (i, u) in [0.40, 0.49, 0.575, 0.73, 0.81, 0.89, 0.97].into_iter().enumerate() {
        let x = (u + rng.range(-0.012, 0.012)) * g.w;
        let sz = rng.range(0.85, 1.15);
        let ch = house(c, p, x, mid.h(x as usize) + 4.0 * g.s, 110.0 * sz * g.s, 56.0 * sz * g.s, 48.0 * sz * g.s, walls[i % 4], 0.3, 1 + (i % 2));
        if i % 3 == 0 {
            chimneys.push((ch, 0.7, 0.3));
        }
        if i % 2 == 1 {
            let tx = x + 60.0 * g.s;
            pine_snow(c, p, tx, mid.h(tx as usize) + 6.0 * g.s, rng.range(60.0, 90.0) * g.s, 0.3);
        }
    }
    // church with a steeple
    let kx = g.w * 0.655;
    let ks = 1.6 * g.s;
    let ky = mid.h(kx as usize) + 4.0 * g.s;
    let wall = mix(mul([0.55, 0.42, 0.36], scale(ambient(p, 0.5), 1.2)), p.haze, 0.3);
    c.rect(kx - 16.0 * ks, ky - 118.0 * ks, kx + 16.0 * ks, ky, wall, 1.0);
    c.tri((kx - 20.0 * g.s, ky - 118.0 * g.s), (kx, ky - 190.0 * g.s), (kx + 20.0 * g.s, ky - 118.0 * g.s), ambient(p, 0.55), 1.0);
    let win = mix(scale(wall, 0.4), [1.0, 0.78, 0.4], p.windows);
    c.rect(kx - 5.0 * g.s, ky - 100.0 * g.s, kx + 5.0 * g.s, ky - 80.0 * g.s, win, 1.0);
    c.glow(kx, ky - 90.0 * g.s, 40.0 * g.s, [1.0, 0.62, 0.25], 0.45 * p.windows);

    // front row on the near hill
    let near = Ridge::new(c.w, |x| {
        let u = x / g.w;
        hill_fn(g, seed ^ 6, 0.80, 0.05, 1.3, 9.0)(x) - g.h * 0.03 * smooth(0.3, 0.9, u)
    });
    snow_layer(c, p, &near, 0.1, 90.0 * g.s, 1.0, seed ^ 7);
    for (i, u) in [0.45, 0.62, 0.78, 0.93].into_iter().enumerate() {
        let x = (u + rng.range(-0.015, 0.015)) * g.w;
        let sz = rng.range(1.2, 1.5);
        let ch = house(c, p, x, near.h(x as usize) + 6.0 * g.s, 170.0 * sz * g.s, 88.0 * sz * g.s, 74.0 * sz * g.s, walls[(i + 1) % 4], 0.08, 2);
        chimneys.push((ch, 1.0, 0.08));
        let tx = x + 95.0 * g.s;
        pine_snow(c, p, tx, near.h(tx as usize) + 6.0 * g.s, rng.range(170.0, 230.0) * g.s, 0.05);
    }
    for (i, ((cx, cy), sz, hz)) in chimneys.into_iter().enumerate() {
        smoke(c, p, cx, cy, sz * g.s * 0.9, seed ^ (20 + i as u32), hz);
    }
    // street lamp
    let lx = g.w * 0.52;
    let ly = near.h(lx as usize) + 4.0 * g.s;
    c.rect(lx - 1.5 * g.s, ly - 72.0 * g.s, lx + 1.5 * g.s, ly, scale(p.tree, 1.3), 1.0);
    c.glow(lx, ly - 74.0 * g.s, 52.0 * g.s, [1.0, 0.7, 0.32], 0.8 * p.windows);
    c.disc(lx, ly - 74.0 * g.s, 4.0 * g.s, [1.0, 0.9, 0.6], 0.5 + 0.5 * p.windows);

    let front = Ridge::new(c.w, hill_fn(g, seed ^ 8, 0.955, 0.025, 1.0, 13.0));
    for (u, sz) in [(0.95, 290.0), (0.99, 230.0)] {
        let x = u * g.w;
        pine_snow(c, p, x, front.h(x as usize) + 12.0 * g.s, sz * g.s, 0.0);
    }
    snow_layer(c, p, &front, 0.0, 60.0 * g.s, 1.0, seed ^ 9);
}

fn aurora_tundra(c: &mut Canvas, p: &Pal, seed: u32) {
    let g = geo(c);
    sky(c, p, 0.66, 1.0, seed);
    let mut q = *p;
    let green = [0.35, 1.0, 0.65];
    q.snow_light = mix(q.snow_light, green, 0.22 * p.aurora);
    q.snow_shadow = mix(q.snow_shadow, [0.2, 0.55, 0.5], 0.2 * p.aurora);
    q.haze = mix(q.haze, [0.25, 0.6, 0.55], 0.16 * p.aurora);
    let p = &q;

    let ridge0 = Ridge::new(c.w, |x| {
        let n = fbm1(seed ^ 1, x / g.w * 4.0, 4, 0.5);
        g.h * 0.64 - g.h * 0.035 * (n * 2.0 - 1.0).abs() * 3.0
    });
    let tcol = mix(p.tree, p.haze, 0.5);
    flat_layer(c, &ridge0, tcol, tcol, 20.0);
    forest_line(c, p, &ridge0, seed ^ 2, tcol, 11.0 * g.s, 14.0, 30.0, 4, true);
    let h1 = Ridge::new(c.w, hill_fn(g, seed ^ 3, 0.70, 0.035, 1.8, 1.0));
    snow_layer(c, p, &h1, 0.5, 80.0 * g.s, 1.0, seed ^ 4);
    let mut rng = Rng(seed ^ 5);
    for i in 0..10 {
        let x = (0.4 + 0.6 * (i as f32 + rng.next()) / 10.0) * g.w;
        pine_snow(c, p, x, h1.h(x as usize) + 4.0 * g.s, rng.range(60.0, 100.0) * g.s, 0.5);
    }
    let h2 = Ridge::new(c.w, hill_fn(g, seed ^ 6, 0.79, 0.035, 1.4, 4.0));
    snow_layer(c, p, &h2, 0.25, 90.0 * g.s, 1.0, seed ^ 7);
    // lone cabin with a warm window
    let cx = g.w * 0.66;
    let cy = h2.h(cx as usize) + 4.0 * g.s;
    for (du, sz) in [(-0.06, 150.0), (0.09, 180.0), (0.12, 125.0)] {
        let x = cx + du * g.w;
        pine_snow(c, p, x, h2.h(x as usize) + 6.0 * g.s, sz * g.s, 0.2);
    }
    let (chx, chy) = house(c, p, cx, cy, 160.0 * g.s, 76.0 * g.s, 62.0 * g.s, [0.55, 0.32, 0.20], 0.2, 1);
    smoke(c, p, chx, chy, 1.1 * g.s, seed ^ 8, 0.2);
    let h3 = Ridge::new(c.w, hill_fn(g, seed ^ 9, 0.90, 0.04, 1.2, 8.0));
    snow_layer(c, p, &h3, 0.05, 110.0 * g.s, 1.0, seed ^ 10);
    for (u, sz) in [(0.88, 330.0), (0.96, 420.0), (0.8, 230.0)] {
        let x = u * g.w;
        pine_snow(c, p, x, h3.h(x as usize) + 10.0 * g.s, sz * g.s, 0.0);
    }
    let h4 = Ridge::new(c.w, hill_fn(g, seed ^ 11, 0.98, 0.02, 1.0, 3.0));
    snow_layer(c, p, &h4, 0.0, 50.0 * g.s, 1.0, seed ^ 12);
}
