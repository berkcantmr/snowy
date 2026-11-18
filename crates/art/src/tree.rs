//! Christmas tree builder. Unit coordinates: 1.0 = tree height, y up, origin at trunk bottom.

use crate::util::*;
use crate::Prim;
use snowy_core::props::TreeLook;
use std::f32::consts::PI;

/// (base y, apex y, half width) of each tier, bottom to top.
const TIERS: [(f32, f32, f32); 4] = [(0.08, 0.45, 0.30), (0.26, 0.62, 0.255), (0.44, 0.78, 0.20), (0.60, 0.89, 0.145)];
const GREEN: Col = rgb(0.14, 0.52, 0.27);
const GREEN_D: Col = rgb(0.07, 0.34, 0.20);
const GOLD: Col = rgb(1.0, 0.80, 0.25);
const SNOW: Col = rgb(0.96, 0.98, 1.0);
const TOP_Y: f32 = 0.885;

/// Point on tier `i`'s swag: `s` 0..1 across, `a_e` = end height as fraction of the tier,
/// `sag` = how far the middle hangs below the ends.
fn swag(i: usize, s: f32, a_e: f32, sag: f32) -> (f32, f32) {
    let (b, a, w) = TIERS[i];
    let hgt = a - b;
    let x = (2.0 * s - 1.0) * w * (1.0 - a_e);
    let y = b + hgt * (a_e - sag * (PI * s).sin());
    (x, y)
}

pub fn build(look: &TreeLook, base: [f32; 2], height: f32, t: f32, out: &mut Vec<Prim>) {
    let mut p = Pen::new(out, base, height * 0.94, 1.0);

    p.ell(0.02, 0.012, 0.3, 0.012, [0.12, 0.22, 0.45, 0.2]);
    p.rrect(0.0, 0.058, 0.04, 0.058, 0.0, 0.2, rgb(0.46, 0.29, 0.15));
    p.rrect(-0.015, 0.058, 0.012, 0.05, 0.0, 1.0, [1.0, 1.0, 1.0, 0.1]);

    // tiers
    let scallops = [5usize, 5, 4, 3];
    for (i, &(b, a, w)) in TIERS.iter().enumerate() {
        let hgt = a - b;
        let mid = (a + b) * 0.5;
        p.tri(0.0, mid - 0.012, w * 1.02, hgt * 0.5, 0.0, GREEN_D);
        p.tri(0.0, mid, w, hgt * 0.5, 0.0, GREEN);
        let n = scallops[i];
        for k in 0..n {
            let x = (k as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let sw = w / n as f32 * 1.15;
            p.ell(x * w * 0.9, b + 0.004, sw, 0.026, GREEN_D);
            p.ell(x * w * 0.9, b + 0.012, sw * 0.95, 0.022, GREEN);
        }
        for sx in [-1.0f32, 1.0] {
            p.ell(sx * w * 0.86, b + 0.026, 0.026, 0.012, SNOW);
        }
    }

    // lights
    if look.lights != 0 {
        let counts = [6usize, 6, 5, 4];
        let mut idx = 0u32;
        for (i, &n) in counts.iter().enumerate() {
            for k in 0..n {
                let s = (k as f32 + 0.5) / n as f32;
                let (x, y) = swag(i, s, 0.26, 0.15);
                let h1 = hash(idx, 11);
                let h2 = hash(idx, 12);
                let speed = if look.lights == 5 { 3.0 + 2.5 * h1 } else { 1.5 + 1.3 * h1 };
                let b = 0.76 + 0.24 * (t * speed + h2 * std::f32::consts::TAU).sin();
                let c = light_color(look.lights, idx, t);
                p.glow(x, y - 0.006, 0.062, [c[0], c[1], c[2], b.clamp(0.0, 1.0)]);
                idx += 1;
            }
        }
    }

    // garland (drawn over the tiers, under the ornaments)
    garland(&mut p, look.garland);

    // ornaments
    if look.ornaments != 0 {
        let counts = [3usize, 3, 2, 2];
        let mut idx = 0u32;
        for (i, &n) in counts.iter().enumerate() {
            let (b, a, w) = TIERS[i];
            for k in 0..n {
                let s = ((k as f32 + 0.5) / n as f32 + (hash(idx, 21) - 0.5) * 0.18).clamp(0.08, 0.92);
                let fy = 0.17 + 0.2 * hash(idx, 22);
                let x = (2.0 * s - 1.0) * w * (1.0 - fy) * 0.78;
                let y = b + (a - b) * fy;
                ornament(&mut p, look.ornaments, idx, x, y);
                idx += 1;
            }
        }
    }

    topper(&mut p, look.topper, t);
}

fn light_color(mode: u8, i: u32, t: f32) -> Col {
    let pick = |pal: &[Col]| pal[i as usize % pal.len()];
    match mode {
        1 => rgb(1.0, 0.84, 0.48),
        2 => pick(&[rgb(1.0, 0.25, 0.25), rgb(0.3, 0.95, 0.4), rgb(0.35, 0.55, 1.0), rgb(1.0, 0.9, 0.3), rgb(1.0, 0.5, 0.9)]),
        3 => pick(&[rgb(0.35, 0.6, 1.0), rgb(0.6, 0.85, 1.0)]),
        4 => pick(&[rgb(1.0, 0.25, 0.25), rgb(0.3, 0.95, 0.4)]),
        5 => hsv(hash(i, 31) + t * 0.12, 0.75, 1.0),
        6 => rgb(1.0, 0.78, 0.25),
        _ => pick(&[rgb(0.72, 0.42, 1.0), rgb(0.95, 0.55, 1.0)]),
    }
}

fn ornament(p: &mut Pen, kind: u8, i: u32, x: f32, y: f32) {
    let red = rgb(0.88, 0.12, 0.16);
    let gold = rgb(1.0, 0.78, 0.22);
    let blue = rgb(0.25, 0.45, 0.92);
    let r = 0.03;
    let bauble = |p: &mut Pen, c: Col| {
        p.rrect(x, y + r + 0.003, 0.007, 0.005, 0.0, 0.3, rgb(0.85, 0.7, 0.25));
        p.ball(x, y, r, r, c);
    };
    let rot = (hash(i, 41) - 0.5) * 0.7;
    match kind {
        1 => bauble(p, red),
        2 => bauble(p, gold),
        3 => bauble(p, [red, gold, blue, rgb(0.9, 0.93, 1.0)][i as usize % 4]),
        4 => {
            let w = 0.013;
            let wh = rgb(0.98, 0.95, 0.95);
            p.line(x, y - 0.034, x, y - 0.009, w, red);
            p.line(x, y - 0.009, x, y + 0.016, w, wh);
            for k in 0..3 {
                let a0 = PI * k as f32 / 3.0;
                let a1 = PI * (k + 1) as f32 / 3.0;
                let c = if k % 2 == 0 { red } else { wh };
                p.line(x - 0.016 + 0.016 * a0.cos(), y + 0.016 + 0.016 * a0.sin(), x - 0.016 + 0.016 * a1.cos(), y + 0.016 + 0.016 * a1.sin(), w, c);
            }
        }
        5 => {
            let c = if i.is_multiple_of(2) { gold } else { rgb(1.0, 0.95, 0.6) };
            p.star(x, y, 0.036, rot, c);
        }
        6 => {
            let c = rgb(0.92, 0.97, 1.0);
            for k in 0..3 {
                                let a = rot + PI * k as f32 / 3.0;
                p.line(x - 0.034 * a.cos(), y - 0.034 * a.sin(), x + 0.034 * a.cos(), y + 0.034 * a.sin(), 0.009, c);
            }
            p.ell(x, y, 0.008, 0.008, c);
        }
        _ => {
            // pinecone: scaly brown ellipse with a little green sprig
            let c = rgb(0.55, 0.36, 0.19);
            p.ell_r(x, y, 0.023, 0.038, rot, c);
            for k in 0..3 {
                let dy = -0.02 + 0.02 * k as f32;
                p.ell_r(x, y + dy, 0.02 - 0.003 * (k as f32 - 1.0).abs(), 0.007, rot * 0.5, rgb(0.38, 0.23, 0.11));
            }
            p.ell_r(x, y + 0.04, 0.012, 0.008, 0.4, rgb(0.2, 0.55, 0.28));
        }
    }
}

fn garland(p: &mut Pen, kind: u8) {
    if kind == 0 {
        return;
    }
    let counts = [9usize, 8, 6, 5];
    let (a_e, sag) = (0.46, 0.13);
    let mut idx = 0u32;
    for (i, &n) in counts.iter().enumerate() {
        match kind {
            1..=3 => {
                let (c1, c2) = match kind {
                    1 => (rgb(1.0, 0.82, 0.30), rgb(1.0, 0.68, 0.15)),
                    2 => (rgb(0.92, 0.95, 1.0), rgb(0.70, 0.76, 0.90)),
                    _ => (rgb(0.86, 0.12, 0.18), rgb(0.74, 0.08, 0.14)),
                };
                let ribbon = kind == 3;
                let m = if ribbon { n + 2 } else { n };
                for k in 0..m {
                    let s0 = k as f32 / m as f32;
                    let s1 = (k + 1) as f32 / m as f32;
                    let (x0, y0) = swag(i, s0, a_e, sag);
                    let (x1, y1) = swag(i, s1, a_e, sag);
                    let c = if k % 2 == 0 { c1 } else { c2 };
                    p.line(x0, y0, x1, y1, if ribbon { 0.02 } else { 0.015 }, c);
                    if !ribbon {
                        let a = (y1 - y0).atan2(x1 - x0) + PI / 2.0 + if k % 2 == 0 { 0.5 } else { -0.5 };
                        p.ray(x1, y1, a, 0.03, 0.006, c2);
                    }
                }
                if ribbon && i == 0 {
                    // a little bow at the left end
                    let (bx, by) = swag(i, 0.0, a_e, sag);
                    p.ell_r(bx - 0.02, by + 0.006, 0.026, 0.014, 0.5, c1);
                    p.ell_r(bx + 0.02, by + 0.006, 0.026, 0.014, -0.5, c1);
                    p.ball(bx, by + 0.003, 0.011, 0.011, c2);
                }
            }
            4 => {
                for k in 0..4 {
                    let (x0, y0) = swag(i, k as f32 / 4.0, a_e, sag);
                    let (x1, y1) = swag(i, (k + 1) as f32 / 4.0, a_e, sag);
                    p.line(x0, y0, x1, y1, 0.004, rgb(0.65, 0.5, 0.35));
                }
                for k in 0..n {
                    let s = (k as f32 + 0.5) / n as f32;
                    let (x, y) = swag(i, s, a_e, sag);
                    let h = hash(idx, 51);
                    let c = if h > 0.7 { rgb(1.0, 0.93, 0.72) } else { rgb(0.99, 0.97, 0.92) };
                    p.ell_r(x, y + (hash(idx, 52) - 0.5) * 0.008, 0.014 + 0.004 * h, 0.012 + 0.003 * h, h * PI, c);
                    idx += 1;
                }
            }
            5 => {
                let pal = [rgb(0.96, 0.45, 0.55), rgb(1.0, 0.85, 0.35), rgb(0.45, 0.82, 0.92)];
                for k in 0..n + 1 {
                    let s = k as f32 / n as f32;
                    let (x, y) = swag(i, s, a_e, sag);
                    p.ball(x, y, 0.0135, 0.0135, pal[(idx as usize) % 3]);
                    idx += 1;
                }
            }
            6 => {
                let (b, a, w) = TIERS[i];
                let hgt = a - b;
                let ang = hgt.atan2(w);
                let cnt = [4usize, 4, 3, 3][i];
                for k in 0..cnt {
                    let f = 0.42 + 0.5 * (k as f32 + 0.5) / cnt as f32;
                    let y = a - hgt * f;
                    for sx in [-1.0f32, 1.0] {
                        let h = hash(idx, 61);
                        p.ell_r(sx * (w * f - 0.012), y - 0.006, 0.034 + 0.01 * h, 0.014, -sx * ang, [SNOW[0], SNOW[1], SNOW[2], 0.96]);
                        idx += 1;
                    }
                }
            }
            _ => {
                let pal = [rgb(0.88, 0.16, 0.2), rgb(0.2, 0.65, 0.35), rgb(1.0, 0.8, 0.25), rgb(0.25, 0.5, 0.9)];
                let m = n - 1;
                for k in 0..m {
                    let s = (k as f32 + 0.5) / m as f32;
                    let (x, y) = swag(i, s, a_e, sag);
                    let (xa, ya) = swag(i, s - 0.02, a_e, sag);
                    let (xb, yb) = swag(i, s + 0.02, a_e, sag);
                    let ang = (yb - ya).atan2(xb - xa) + if k % 2 == 0 { 0.0 } else { PI / 2.5 };
                    p.ring_r(x, y, 0.027, 0.016, ang, 0.36, pal[(idx as usize) % 4]);
                    idx += 1;
                }
            }
        }
    }
}

fn topper(p: &mut Pen, kind: u8, t: f32) {
    let y = TOP_Y;
    match kind {
        1 | 2 => {
            let (c, c2, gl) = if kind == 1 {
                (rgb(1.0, 0.80, 0.22), rgb(1.0, 0.95, 0.65), [1.0, 0.85, 0.35, 0.6])
            } else {
                (rgb(0.82, 0.88, 0.98), rgb(1.0, 1.0, 1.0), [0.7, 0.85, 1.0, 0.6])
            };
            let pulse = 0.85 + 0.15 * (t * 2.0).sin();
            p.glow(0.0, y + 0.05, 0.12, [gl[0], gl[1], gl[2], gl[3] * pulse]);
            p.star(0.0, y + 0.05, 0.068, 0.0, c);
            p.star(0.0, y + 0.05, 0.04, 0.0, c2);
        }
        3 => {
            let c = rgb(0.86, 0.12, 0.17);
            for sx in [-1.0f32, 1.0] {
                p.tri(sx * 0.03, y + 0.02, 0.024, 0.05, PI + sx * 0.35, mul(c, 0.8));
                p.ell_r(sx * 0.052, y + 0.07, 0.056, 0.033, sx * 0.4, c);
                p.ell_r(sx * 0.056, y + 0.072, 0.034, 0.014, sx * 0.4, rgb(1.0, 0.45, 0.45));
            }
            p.ball(0.0, y + 0.058, 0.024, 0.024, rgb(1.0, 0.8, 0.25));
        }
        4 => {
            let c = rgb(0.9, 0.97, 1.0);
            let cy = y + 0.05;
            p.glow(0.0, cy, 0.12, [0.7, 0.88, 1.0, 0.5]);
            for k in 0..3 {
                let a = PI / 2.0 + PI * k as f32 / 3.0;
                p.line(0.05 * a.cos(), cy + 0.05 * a.sin(), -0.05 * a.cos(), cy - 0.05 * a.sin(), 0.012, c);
            }
            for k in 0..6 {
                let a = PI / 2.0 + PI * k as f32 / 3.0;
                let (bx, by) = (0.03 * a.cos(), cy + 0.03 * a.sin());
                p.ray(bx, by, a + 0.9, 0.022, 0.008, c);
                p.ray(bx, by, a - 0.9, 0.022, 0.008, c);
            }
            p.ell(0.0, cy, 0.01, 0.01, c);
        }
        5 => {
            let c = rgb(0.93, 0.18, 0.33);
            let cy = y + 0.045;
            p.ell(-0.03, cy + 0.03, 0.036, 0.036, c);
            p.ell(0.03, cy + 0.03, 0.036, 0.036, c);
            p.tri(0.0, cy - 0.005, 0.066, 0.058, PI, c);
            p.ell_r(-0.04, cy + 0.045, 0.012, 0.007, 0.6, [1.0, 1.0, 1.0, 0.6]);
        }
        6 => {
            let c = rgb(1.0, 0.95, 0.72);
            let cy = y + 0.07;
            let n = 11;
            for k in 0..n {
                let f = k as f32 / (n - 1) as f32;
                let a = (95.0 + 170.0 * f).to_radians();
                let r = 0.01 + 0.03 * (PI * f).sin();
                p.ell(0.072 * a.cos() + 0.02, cy + 0.072 * a.sin(), r, r, c);
            }
            p.star(0.075, cy + 0.03, 0.022, 0.2, rgb(1.0, 0.93, 0.55));
            p.star(0.05, cy - 0.04, 0.013, 0.0, rgb(1.0, 0.93, 0.55));
        }
        7 => {
            let robe = rgb(1.0, 0.97, 0.88);
            p.ell_r(-0.055, y + 0.075, 0.03, 0.062, 0.7, rgb(0.9, 0.95, 1.0));
            p.ell_r(0.055, y + 0.075, 0.03, 0.062, -0.7, rgb(0.9, 0.95, 1.0));
            p.tri(0.0, y + 0.05, 0.05, 0.065, 0.0, robe);
            p.tri(0.0, y + 0.02, 0.042, 0.015, 0.0, GOLD);
            p.ell(0.0, y + 0.123, 0.03, 0.03, rgb(1.0, 0.86, 0.72));
            p.ring(0.0, y + 0.158, 0.036, 0.011, 0.35, GOLD);
        }
        _ => {}
    }
}
