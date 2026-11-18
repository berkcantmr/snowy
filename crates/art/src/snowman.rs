//! Snowman builder. Unit coordinates: 1.0 = total height, y up, origin at the base.

use crate::util::*;
use crate::{Prim, PrimKind};
use snowy_core::props::SnowmanLook;
use std::f32::consts::{FRAC_PI_2, PI};

const COAL: Col = rgb(0.10, 0.10, 0.13);
const WOOD: Col = rgb(0.46, 0.30, 0.15);
const WOOD_D: Col = rgb(0.34, 0.21, 0.11);
const CARROT: Col = rgb(1.0, 0.52, 0.10);
const RED: Col = rgb(0.86, 0.14, 0.18);
const GOLD: Col = rgb(1.0, 0.78, 0.22);
const STEEL: Col = rgb(0.58, 0.64, 0.72);
const FUR: Col = rgb(0.97, 0.98, 1.0);
const FUR_EDGE: Col = rgb(0.74, 0.81, 0.93);
const SQ: f32 = 0.94;
const BODY_TOP: f32 = 0.80;

#[derive(Clone, Copy)]
struct Ball {
    cy: f32,
    rx: f32,
    ry: f32,
}

fn rotv(x: f32, y: f32, a: f32) -> (f32, f32) {
    let (s, c) = a.sin_cos();
    (x * c - y * s, x * s + y * c)
}

pub fn build(look: &SnowmanLook, balls: u8, base: [f32; 2], height: f32, t: f32, out: &mut Vec<Prim>) {
    let mut p = Pen::new(out, base, height, 1.0);
    let three = balls >= 3;
    let radii: &[f32] = if three { &[0.40, 0.33, 0.30] } else { &[0.50, 0.40] };
    let n = radii.len();
    let mut bl = [Ball { cy: 0.0, rx: 0.0, ry: 0.0 }; 3];
    let mut top = 0.0;
    for i in 0..n {
        let r = radii[i];
        let ry = r * SQ;
        let cy = if i == 0 { ry } else { top + ry * 0.66 };
        bl[i] = Ball { cy, rx: r, ry };
        top = cy + ry;
    }
    let f = BODY_TOP / top;
    for b in bl.iter_mut().take(n) {
        b.cy *= f;
        b.rx *= f;
        b.ry *= f;
    }
    let ti = if three { 1 } else { 0 };
    let tor = bl[ti];
    let head = bl[n - 1];
    let r = head.rx;
    let neck_y = head.cy - head.ry * 0.84;
    let dy = ((neck_y - head.cy) / head.ry).clamp(-1.0, 1.0);
    let neck_half = head.rx * (1.0 - dy * dy).sqrt() * 0.98;

    // ground shadow
    p.ell(0.02, 0.016, bl[0].rx * 1.3, 0.016, [0.12, 0.22, 0.45, 0.2]);

    // cape (behind everything)
    if look.outfit == 5 {
        let h = neck_y + 0.08 - 0.03;
        p.tri(0.0, 0.03 + h * 0.5, 0.5, h * 0.5, 0.0, rgb(0.72, 0.10, 0.20));
        p.tri(0.0, 0.03 + h * 0.5 - 0.01, 0.36, h * 0.5 - 0.02, 0.0, rgb(0.50, 0.06, 0.15));
    }

    // arms (behind the balls, roots tucked inside)
    let sh_y = tor.cy + tor.ry * if three { 0.22 } else { 0.38 };
    let sdy = ((sh_y - tor.cy) / tor.ry).clamp(-1.0, 1.0);
    let sh_x = tor.rx * (1.0 - sdy * sdy).sqrt() * 0.80;
    let (tip_l, tip_r);
    if look.item == 0 {
        let w1 = (t * 4.2).sin() * 0.16;
        let w2 = (t * 4.2 + 1.9).sin() * 0.16;
        let (ax, ay) = rotv(-0.2, 0.2, -w1);
        tip_l = [-sh_x + ax, sh_y + ay];
        let (bx, by) = rotv(0.2, 0.2, w2);
        tip_r = [sh_x + bx, sh_y + by];
    } else {
        tip_l = [-sh_x - 0.17, sh_y - 0.045 + (t * 1.3).sin() * 0.006];
        tip_r = [sh_x + 0.15, sh_y + 0.075];
    }
    arm(&mut p, [-sh_x, sh_y], tip_l);
    arm(&mut p, [sh_x, sh_y], tip_r);
    if look.outfit == 7 {
        for (tp, sx) in [(tip_l, -1.0f32), (tip_r, 1.0)] {
            p.ell(tp[0] + sx * 0.012, tp[1] + 0.005, 0.021, 0.014, rgb(0.8, 0.12, 0.16)); // thumb
            p.ball(tp[0], tp[1], 0.043, 0.04, rgb(0.84, 0.14, 0.18));
            p.rrect(tp[0] - sx * 0.018, tp[1] - 0.003, 0.012, 0.026, 0.0, 0.4, [1.0, 1.0, 1.0, 0.8]);
        }
    }
    if look.item != 0 {
        item(&mut p, look.item, tip_r, t);
    }

    // body
    for (i, b) in bl.iter().enumerate().take(n) {
        p.ball(0.0, b.cy, b.rx, b.ry, [1.0, 1.0, 1.0, 1.0]);
        if i == ti {
            torso(&mut p, look.outfit, tor);
        }
    }
    neckwear(&mut p, look.outfit, neck_y, neck_half);

    face(&mut p, look.face, head.cy, r, t);
    hat(&mut p, look.hat, head.cy, r);
}

fn arm(p: &mut Pen, s: [f32; 2], tip: [f32; 2]) {
    let (dx, dy) = (tip[0] - s[0], tip[1] - s[1]);
    let len = dx.hypot(dy).max(1e-4);
    let (mut nx, mut ny) = (-dy / len, dx / len);
    if ny < 0.0 {
        nx = -nx;
        ny = -ny;
    }
    let e = [(s[0] + tip[0]) * 0.5 + nx * 0.02, (s[1] + tip[1]) * 0.5 + ny * 0.02];
    let w = 0.017;
    p.line(s[0], s[1], e[0], e[1], w, WOOD);
    p.line(e[0], e[1], tip[0], tip[1], w * 0.9, WOOD);
    let a = (tip[1] - e[1]).atan2(tip[0] - e[0]);
    let q = [e[0] + (tip[0] - e[0]) * 0.55, e[1] + (tip[1] - e[1]) * 0.55];
    p.ray(q[0], q[1], a + 0.6, 0.05, w * 0.6, WOOD);
    p.ray(q[0], q[1], a - 0.55, 0.045, w * 0.6, WOOD);
    p.ray(tip[0], tip[1], a + 0.25, 0.02, w * 0.6, WOOD);
}

fn item(p: &mut Pen, id: u8, t: [f32; 2], time: f32) {
    let (tx, ty) = (t[0], t[1]);
    match id {
        1 => {
            // broom
            let straw = rgb(0.92, 0.75, 0.30);
            p.line(tx + 0.012, 0.13, tx - 0.012, ty + 0.24, 0.017, WOOD);
            p.tri(tx + 0.012, 0.072, 0.058, 0.074, 0.0, straw);
            p.line(tx + 0.012, 0.13, tx - 0.016, 0.005, 0.006, rgb(0.72, 0.52, 0.18));
            p.line(tx + 0.012, 0.13, tx + 0.04, 0.005, 0.006, rgb(0.72, 0.52, 0.18));
            p.rrect(tx + 0.011, 0.128, 0.026, 0.013, 0.0, 0.5, rgb(0.62, 0.20, 0.18));
        }
        2 => {
            // candy cane
            let y0 = 0.08;
            let ytop = ty + 0.15;
            let mut pts: [[f32; 2]; 12] = [[0.0; 2]; 12];
            for (i, pt) in pts.iter_mut().enumerate().take(6) {
                *pt = [tx, y0 + (ytop - y0) * i as f32 / 6.0];
            }
            for i in 0..6 {
                let a = PI * i as f32 / 5.0;
                pts[6 + i] = [tx - 0.05 + 0.05 * a.cos(), ytop + 0.05 * a.sin()];
            }
            pts[6] = [tx, ytop];
            for i in 0..11 {
                let c = if i % 2 == 0 { RED } else { rgb(0.98, 0.94, 0.94) };
                p.line(pts[i][0], pts[i][1], pts[i + 1][0], pts[i + 1][1], 0.03, c);
            }
            p.line(pts[11][0], pts[11][1], pts[11][0], pts[11][1] - 0.03, 0.03, RED);
        }
        3 => {
            // shovel
            let bx = tx - 0.02;
            p.line(bx + 0.02, 0.2, tx, ty + 0.22, 0.018, WOOD);
            p.line(tx - 0.03, ty + 0.22, tx + 0.03, ty + 0.22, 0.016, WOOD_D);
            p.tri(bx, 0.045, 0.07, 0.05, std::f32::consts::PI, STEEL);
            p.rrect(bx, 0.115, 0.07, 0.062, 0.0, 0.3, STEEL);
            p.rrect(bx - 0.03, 0.095, 0.011, 0.07, 0.0, 1.0, [1.0, 1.0, 1.0, 0.33]);
            p.rrect(bx, 0.172, 0.072, 0.012, 0.0, 0.8, rgb(0.42, 0.48, 0.58));
            p.ball(bx + 0.005, 0.2, 0.058, 0.036, [1.0, 1.0, 1.0, 1.0]);
        }
        4 => {
            // flag
            let top = ty + 0.34;
            p.line(tx, 0.0, tx, top, 0.016, rgb(0.78, 0.66, 0.45));
            p.ball(tx, top + 0.012, 0.014, 0.014, GOLD);
            for j in 0..5 {
                let jf = j as f32;
                let off = (time * 3.4 - jf * 0.9).sin() * 0.009 * (0.3 + jf * 0.3);
                // Winter-blue pennant (deliberately not any real country's flag).
                let c = if j % 2 == 0 { rgb(0.22, 0.5, 0.88) } else { rgb(0.17, 0.42, 0.8) };
                p.rrect(tx + 0.008 + 0.0325 * (jf + 0.5), top - 0.07 + off, 0.034, 0.07, 0.0, 0.0, c);
                if j == 2 {
                    p.star(tx + 0.008 + 0.0325 * 2.5, top - 0.07 + off, 0.045, 0.0, rgb(1.0, 1.0, 1.0));
                }
            }
        }
        5 => {
            // lantern: hangs from the hand, flickering glow
            let fl = 0.82 + 0.10 * (time * 9.0).sin() + 0.08 * (time * 23.0 + 1.3).sin();
            let k = 1.4;
            let cy = ty - 0.085 * k;
            let metal = rgb(0.28, 0.28, 0.32);
            p.glow(tx, cy, 0.17, [1.0, 0.72, 0.28, fl]);
            p.ring(tx, ty + 0.03, 0.026 * k, 0.04 * k, 0.2, metal);
            p.rrect(tx, cy, 0.04 * k, 0.056 * k, 0.0, 0.3, rgb(0.22, 0.22, 0.26));
            p.rrect(tx, cy, 0.029 * k, 0.043 * k, 0.0, 0.3, [1.0, 0.86 + 0.06 * fl, 0.42, 1.0]);
            p.rrect(tx, cy + 0.058 * k, 0.046 * k, 0.012 * k, 0.0, 0.8, metal);
            p.rrect(tx, cy - 0.058 * k, 0.046 * k, 0.012 * k, 0.0, 0.8, metal);
            p.line(tx, cy - 0.043 * k, tx, cy + 0.043 * k, 0.007, [0.3, 0.2, 0.1, 0.8]);
        }
        6 => {
            // gift box
            let (cx, cy) = (tx + 0.015, ty + 0.035);
            let green = rgb(0.16, 0.60, 0.36);
            p.rrect(cx, cy, 0.066, 0.058, 0.0, 0.12, green);
            p.rrect(cx, cy + 0.042, 0.072, 0.02, 0.0, 0.3, rgb(0.12, 0.50, 0.30));
            p.rrect(cx, cy, 0.013, 0.058, 0.0, 0.0, GOLD);
            p.ell_r(cx - 0.022, cy + 0.075, 0.025, 0.013, 0.6, GOLD);
            p.ell_r(cx + 0.022, cy + 0.075, 0.025, 0.013, -0.6, GOLD);
            p.ball(cx, cy + 0.066, 0.012, 0.012, rgb(0.9, 0.65, 0.12));
        }
        _ => {
            // mug with steam
            let k = 1.4;
            let (cx, cy) = (tx, ty + 0.05);
            let mug = rgb(0.90, 0.94, 1.0);
            p.ring(cx + 0.044 * k, cy, 0.022 * k, 0.028 * k, 0.34, rgb(0.82, 0.88, 0.98));
            p.rrect(cx, cy, 0.037 * k, 0.044 * k, 0.0, 0.25, mug);
            p.rrect(cx, cy, 0.038 * k, 0.012 * k, 0.0, 0.3, [0.85, 0.2, 0.2, 0.9]);
            p.ell(cx, cy + 0.042 * k, 0.031 * k, 0.008 * k, rgb(0.35, 0.2, 0.1));
            for i in 0..3u32 {
                let ph = (time * 0.4 + i as f32 / 3.0).fract();
                let x = cx + (ph * 7.0 + i as f32 * 2.0).sin() * 0.014 + (i as f32 - 1.0) * 0.016;
                let y = cy + 0.08 + ph * 0.14;
                let a = (1.0 - ph) * (ph * 8.0).min(1.0) * 0.6;
                p.ell(x, y, 0.014 + 0.016 * ph, 0.014 + 0.016 * ph, [1.0, 1.0, 1.0, a]);
            }
        }
    }
}

fn torso(p: &mut Pen, outfit: u8, tor: Ball) {
    let (rx, ry, cy) = (tor.rx, tor.ry, tor.cy);
    let mut buttons = true;
    match outfit {
        4 => {
            // vest: hugs the ball from the waist up, open V at the front showing a shirt
            let green = rgb(0.17, 0.42, 0.34);
            band(p, tor, -0.62, 0.92, green);
            // V opening: white shirt wedge from the top down to the chest
            p.tri(0.0, cy + ry * 0.5, rx * 0.30, ry * 0.46, PI, rgb(0.96, 0.97, 1.0));
            // hem: slightly darker strip that follows the curve
            band(p, tor, -0.62, -0.5, mul(green, 0.72));
            for i in 0..3 {
                p.ball(0.0, cy - ry * (0.02 + 0.2 * i as f32), 0.017, 0.017, GOLD);
            }
            buttons = false;
        }
        6 => {
            // sweater: covers the ball from the hips up, ribbed hem, two cream stripes, stars
            let red = rgb(0.82, 0.17, 0.24);
            let (lo, hi) = (-0.58f32, 0.95f32);
            band(p, tor, lo, hi, red);
            band(p, tor, lo, lo + 0.14, mul(red, 0.72));
            for i in 0..5 {
                // rib marks on the hem
                let fx = (i as f32 - 2.0) / 2.6;
                let y = cy + ry * (lo + 0.07);
                let half = chord(tor, lo + 0.07);
                p.rrect(fx * half, y, 0.005, ry * 0.055, 0.0, 1.0, mul(red, 0.55));
            }
            let cream = rgb(0.98, 0.93, 0.80);
            for d in [0.42f32, -0.12] {
                band(p, tor, d - 0.035, d + 0.035, cream);
            }
            for i in 0..3 {
                let x = (i as f32 - 1.0) * rx * 0.36;
                p.star(x, cy + ry * 0.15, 0.032, 0.0, cream);
            }
            buttons = false;
        }
        _ => {}
    }
    if buttons {
        for i in 0..3 {
            let y = cy + ry * (0.42 - 0.42 * i as f32);
            p.ball(0.012, y, 0.019, 0.019, COAL);
        }
    }
}

/// Half width of `b`'s silhouette at height `f` (-1 bottom .. 1 top of the ellipse).
fn chord(b: Ball, f: f32) -> f32 {
    b.rx * (1.0 - f.clamp(-1.0, 1.0).powi(2)).max(0.0).sqrt()
}

/// Fabric covering ball `b` between heights `lo..hi` (-1 bottom .. 1 top): one `Band`
/// primitive using the ball's own ellipse, so garments follow the body exactly with smooth
/// edges at any size. The shader adds the fabric shading (darker rim and lower part).
fn band(p: &mut Pen, b: Ball, lo: f32, hi: f32, color: Col) {
    if hi <= lo {
        return;
    }
    p.add(PrimKind::Band, 0.0, b.cy, b.rx * 0.992, b.ry * 0.992, 0.0, color, crate::band_round(lo, hi));
}

fn neckwear(p: &mut Pen, outfit: u8, ny: f32, half: f32) {
    match outfit {
        1 | 2 | 7 => {
            let (c1, c2) = match outfit {
                1 => (rgb(0.86, 0.14, 0.18), rgb(0.78, 0.10, 0.15)),
                2 => (rgb(0.20, 0.42, 0.85), rgb(1.0, 0.82, 0.25)),
                _ => (rgb(0.17, 0.62, 0.38), rgb(0.12, 0.52, 0.32)),
            };
            let w = half + 0.02;
            // tails first (behind the band)
            for (x, len, rot) in [(0.045f32, 0.085f32, 0.10f32), (0.085, 0.065, -0.12)] {
                for j in 0..3 {
                    let c = if outfit == 2 {
                        if j % 2 == 0 { c1 } else { c2 }
                    } else {
                        mul(c1, 1.0 - 0.07 * (j % 2) as f32)
                    };
                    let seg = len * 2.0 / 3.0;
                    let y = -0.02 - seg * 0.5 - seg * j as f32;
                    let (ox, oy) = rotv(0.0, y, rot);
                    p.rrect(x + ox, ny + oy, 0.027, seg * 0.55, rot, 0.1, c);
                }
            }
            let n = 7;
            for i in 0..n {
                let x = -w + 2.0 * w * (i as f32 + 0.5) / n as f32;
                let c = if outfit == 2 {
                    if i % 2 == 0 { c1 } else { c2 }
                } else {
                    mul(c1, 1.0 - 0.07 * (i % 2) as f32)
                };
                p.rrect(x, ny, w / n as f32 * 1.1, 0.034, 0.0, 0.5, c);
            }
            p.rrect(0.0, ny + 0.026, w * 0.96, 0.008, 0.0, 1.0, [1.0, 1.0, 1.0, 0.18]);
        }
        3 => {
            let blue = rgb(0.18, 0.32, 0.80);
            for sx in [-1.0f32, 1.0] {
                p.tri(sx * 0.062, ny - 0.004, 0.046, 0.058, -sx * FRAC_PI_2, blue);
                p.ell(sx * 0.07, ny - 0.002, 0.007, 0.007, [1.0, 1.0, 1.0, 0.9]);
                p.ell(sx * 0.1, ny + 0.014, 0.006, 0.006, [1.0, 1.0, 1.0, 0.9]);
            }
            p.rrect(0.0, ny - 0.004, 0.02, 0.026, 0.0, 0.5, rgb(0.12, 0.22, 0.62));
        }
        5 => {
            // clasp + collar
            p.rrect(0.0, ny, half + 0.018, 0.03, 0.0, 0.6, rgb(0.62, 0.08, 0.17));
            p.line(-0.03, ny - 0.002, 0.03, ny - 0.002, 0.012, GOLD);
            p.ball(0.0, ny - 0.002, 0.022, 0.022, GOLD);
        }
        6 => {
            let red = rgb(0.74, 0.14, 0.21);
            p.rrect(0.0, ny, half + 0.02, 0.034, 0.0, 0.6, red);
            for i in 0..6 {
                let x = -half + (i as f32 + 0.5) * (2.0 * half) / 6.0;
                p.rrect(x, ny, 0.004, 0.026, 0.0, 1.0, mul(red, 0.8));
            }
        }
        _ => {}
    }
}

fn coal(p: &mut Pen, x: f32, y: f32, r: f32) {
    p.ball(x, y, r, r, COAL);
}

#[allow(clippy::too_many_arguments)]
fn smile(p: &mut Pen, cy: f32, r: f32, from: f32, to: f32, rad: f32, size: f32, ymid: f32) {
    let n = 5;
    for i in 0..n {
        let a = (from + (to - from) * i as f32 / (n - 1) as f32).to_radians();
        coal(p, rad * a.cos() * r, cy + (ymid + rad * a.sin()) * r, size * r);
    }
}

fn face(p: &mut Pen, id: u8, cy: f32, r: f32, t: f32) {
    let g = |x: f32, y: f32| (x * r, cy + y * r);
    let eye = |p: &mut Pen, x: f32, y: f32, s: f32| {
        let (gx, gy) = g(x, y);
        coal(p, gx, gy, s * r);
    };
    let carrot = |p: &mut Pen, dy: f32| {
        let (gx, gy) = g(0.36, dy - 0.05);
        p.tri(gx, gy, 0.17 * r, 0.44 * r, -FRAC_PI_2 - 0.1, CARROT);
        p.tri(gx + 0.02 * r, gy - 0.06 * r, 0.09 * r, 0.34 * r, -FRAC_PI_2 - 0.1, rgb(0.9, 0.38, 0.08));
        let (x1, y1) = g(0.26, dy - 0.05);
        p.line(x1, y1 - 0.06 * r, x1 + 0.01 * r, y1 + 0.08 * r, 0.035 * r, rgb(0.82, 0.32, 0.06));
    };
    match id {
        1 => {
            // big grin
            eye(p, -0.36, 0.26, 0.14);
            eye(p, 0.36, 0.26, 0.14);
            let (mx, my) = g(0.0, -0.42);
            p.ell(mx, my, 0.52 * r, 0.30 * r, rgb(0.30, 0.07, 0.10));
            p.rrect(mx, my + 0.19 * r, 0.42 * r, 0.075 * r, 0.0, 0.8, rgb(0.97, 0.98, 1.0));
            p.ell(mx, my - 0.19 * r, 0.27 * r, 0.12 * r, rgb(0.94, 0.38, 0.46));
            carrot(p, 0.0);
        }
        2 => {
            // surprised
            eye(p, -0.36, 0.26, 0.16);
            eye(p, 0.36, 0.26, 0.16);
            let (a, b) = g(-0.62, 0.62);
            let (c, d) = g(-0.14, 0.7);
            p.line(a, b, c, d, 0.07 * r, WOOD_D);
            let (a, b) = g(0.62, 0.62);
            let (c, d) = g(0.14, 0.7);
            p.line(a, b, c, d, 0.07 * r, WOOD_D);
            let (mx, my) = g(0.0, -0.5);
            p.ell(mx, my, 0.15 * r, 0.21 * r, COAL);
            carrot(p, 0.0);
        }
        3 => {
            // wink
            eye(p, -0.36, 0.26, 0.14);
            let (a, b) = g(0.2, 0.22);
            let (c, d) = g(0.36, 0.36);
            let (e, f) = g(0.52, 0.22);
            p.line(a, b, c, d, 0.075 * r, COAL);
            p.line(c, d, e, f, 0.075 * r, COAL);
            smile(p, cy, r, 212.0, 328.0, 0.62, 0.085, 0.16);
            carrot(p, 0.02);
        }
        4 => {
            // sleepy
            for sx in [-1.0f32, 1.0] {
                let (a, b) = g(sx * 0.36 - 0.17, 0.3);
                let (c, d) = g(sx * 0.36, 0.2);
                let (e, f) = g(sx * 0.36 + 0.17, 0.3);
                p.line(a, b, c, d, 0.075 * r, COAL);
                p.line(c, d, e, f, 0.075 * r, COAL);
            }
            let (mx, my) = g(0.0, -0.42);
            p.ell(mx, my, 0.1 * r, 0.065 * r, COAL);
            carrot(p, 0.02);
            // floating z's
            for (k, sc, ph) in [(0, 1.0f32, 0.0f32), (1, 0.65, 0.5)] {
                let u = (t * 0.45 + ph).fract();
                let (zx, zy) = g(1.05 + 0.5 * k as f32 + u * 0.15, 0.9 + 0.75 * k as f32 + u * 0.4);
                let s = 0.15 * r * sc;
                let w = 0.055 * r * sc;
                let c = [0.55, 0.72, 1.0, (1.0 - u * u) * 0.95];
                p.line(zx - s, zy + s, zx + s, zy + s, w, c);
                p.line(zx + s, zy + s, zx - s, zy - s, w, c);
                p.line(zx - s, zy - s, zx + s, zy - s, w, c);
            }
        }
        5 => {
            // tongue out
            eye(p, -0.36, 0.26, 0.14);
            eye(p, 0.36, 0.26, 0.14);
            let (tx, ty) = g(0.14, -0.5);
            p.rrect(tx, ty, 0.14 * r, 0.24 * r, 0.0, 1.0, rgb(0.95, 0.40, 0.50));
            p.line(tx, ty + 0.08 * r, tx, ty - 0.14 * r, 0.03 * r, rgb(0.82, 0.28, 0.4));
            smile(p, cy, r, 212.0, 328.0, 0.62, 0.085, 0.16);
            carrot(p, 0.05);
        }
        6 => {
            // sunglasses
            for sx in [-1.0f32, 1.0] {
                let (lx, ly) = g(sx * 0.34, 0.26);
                p.rrect(lx, ly, 0.31 * r, 0.22 * r, 0.0, 0.5, rgb(0.07, 0.07, 0.10));
                let (a, b) = g(sx * 0.34 - 0.13, 0.35);
                let (c, d) = g(sx * 0.34 - 0.02, 0.14);
                p.line(a, b, c, d, 0.05 * r, [1.0, 1.0, 1.0, 0.55]);
                p.line(a + 0.1 * r, b, c + 0.1 * r, d, 0.025 * r, [1.0, 1.0, 1.0, 0.4]);
            }
            let (bx, by) = g(0.0, 0.3);
            p.line(bx - 0.1 * r, by, bx + 0.1 * r, by, 0.06 * r, rgb(0.07, 0.07, 0.10));
            smile(p, cy, r, 222.0, 330.0, 0.6, 0.07, 0.14);
            carrot(p, -0.1);
        }
        7 => {
            // rosy cheeks
            for sx in [-1.0f32, 1.0] {
                let (x, y) = g(sx * 0.66, -0.08);
                p.ell(x, y, 0.22 * r, 0.14 * r, [1.0, 0.45, 0.55, 0.6]);
            }
            eye(p, -0.34, 0.28, 0.14);
            eye(p, 0.34, 0.28, 0.14);
            smile(p, cy, r, 214.0, 326.0, 0.62, 0.08, 0.1);
            carrot(p, 0.06);
        }
        _ => {
            // classic coal smile
            eye(p, -0.36, 0.26, 0.14);
            eye(p, 0.36, 0.26, 0.14);
            smile(p, cy, r, 212.0, 328.0, 0.62, 0.085, 0.16);
            carrot(p, 0.04);
        }
    }
}

/// Half-ellipse-ish dome built from stacked rounded slices; bottom edge at `y0`.
fn dome(p: &mut Pen, x: f32, y0: f32, w: f32, h: f32, c: Col) {
    let n = 5;
    for j in 0..n {
        let um = (j as f32 + 0.35) / n as f32;
        let half_w = w * (1.0 - um * um).max(0.02).sqrt();
        let sh = h / n as f32;
        p.rrect(x, y0 + sh * (j as f32 + 0.8), half_w, sh * 0.85, 0.0, 0.7, c);
    }
}

fn hat(p: &mut Pen, id: u8, cy: f32, r: f32) {
    let g = |y: f32| cy + (y + 0.1) * r;
    match id {
        1 => {
            // top hat
            let c = rgb(0.14, 0.14, 0.19);
            p.rrect(0.0, g(0.56), 1.12 * r, 0.11 * r, 0.0, 1.0, c);
            p.rrect(0.0, g(0.56 + 0.64), 0.68 * r, 0.64 * r, 0.0, 0.16, c);
            p.rrect(0.0, g(0.56 + 0.22), 0.69 * r, 0.15 * r, 0.0, 0.1, rgb(0.78, 0.14, 0.2));
            p.rrect(-0.4 * r, g(1.35), 0.06 * r, 0.42 * r, 0.0, 1.0, [1.0, 1.0, 1.0, 0.13]);
            p.rrect(0.0, g(0.62), 1.06 * r, 0.03 * r, 0.0, 1.0, [1.0, 1.0, 1.0, 0.12]);
        }
        2 => {
            // beanie with pom-pom
            let c = rgb(0.22, 0.46, 0.86);
            dome(p, 0.0, g(0.42), 0.9 * r, 0.95 * r, c);
            let cuff = rgb(0.15, 0.30, 0.66);
            p.rrect(0.0, g(0.55), 1.0 * r, 0.22 * r, 0.0, 0.5, cuff);
            for j in 0..7 {
                p.rrect((-0.84 + 0.28 * j as f32) * r, g(0.55), 0.03 * r, 0.17 * r, 0.0, 1.0, [1.0, 1.0, 1.0, 0.2]);
            }
            p.ball(0.0, g(1.42), 0.27 * r, 0.27 * r, rgb(0.95, 0.96, 1.0));
        }
        3 => {
            // santa hat
            let red = rgb(0.84, 0.12, 0.17);
            let rot = -0.3;
            let (ax, ay) = rotv(0.0, 0.82 * r, rot);
            let c = [0.1 * r, g(0.55 + 0.8)];
            p.tri(c[0], c[1], 0.86 * r, 0.82 * r, rot, red);
            p.rrect(c[0] - 0.4 * r, c[1] - 0.05 * r, 0.05 * r, 0.5 * r, rot, 1.0, [1.0, 1.0, 1.0, 0.12]);
            let apex = [c[0] + ax, c[1] + ay];
            let arot = rot - 0.95;
            let (bx, by) = rotv(0.0, 0.2 * r, arot);
            p.tri(apex[0] + bx, apex[1] + by, 0.14 * r, 0.3 * r, arot, mul(red, 0.95));
            let (px, py) = rotv(0.0, 0.5 * r, arot);
            p.ball(apex[0] + px, apex[1] + py, 0.25 * r, 0.25 * r, FUR);
            p.rrect(0.0, g(0.52), 1.04 * r, 0.21 * r, 0.0, 1.0, FUR_EDGE);
            p.rrect(0.0, g(0.54), 1.0 * r, 0.185 * r, 0.0, 1.0, FUR);
        }
        4 => {
            // bucket
            let steel = rgb(0.62, 0.68, 0.76);
            p.ring(0.0, g(1.35), 0.62 * r, 0.5 * r, 0.1, rgb(0.28, 0.3, 0.36));
            p.rrect(0.0, g(1.0), 0.78 * r, 0.5 * r, 0.0, 0.18, steel);
            p.rrect(0.0, g(0.52), 0.86 * r, 0.075 * r, 0.0, 0.8, rgb(0.46, 0.52, 0.6));
            p.rrect(0.0, g(1.46), 0.83 * r, 0.07 * r, 0.0, 0.8, rgb(0.74, 0.8, 0.87));
            p.rrect(0.0, g(0.98), 0.8 * r, 0.025 * r, 0.0, 1.0, rgb(0.46, 0.52, 0.6));
            p.rrect(-0.42 * r, g(1.0), 0.07 * r, 0.4 * r, 0.0, 1.0, [1.0, 1.0, 1.0, 0.3]);
        }
        5 => {
            // cowboy hat
            let tan = rgb(0.80, 0.56, 0.30);
            let dark = rgb(0.52, 0.32, 0.16);
            p.line(-0.9 * r, g(0.55), -1.62 * r, g(0.82), 0.2 * r, tan);
            p.line(0.9 * r, g(0.55), 1.62 * r, g(0.82), 0.2 * r, tan);
            p.rrect(0.0, g(0.56), 1.05 * r, 0.11 * r, 0.0, 1.0, tan);
            dome(p, 0.0, g(0.55), 0.66 * r, 0.95 * r, tan);
            p.ell(0.0, g(1.45), 0.2 * r, 0.06 * r, mul(tan, 0.78));
            p.rrect(0.0, g(0.72), 0.67 * r, 0.09 * r, 0.0, 0.3, dark);
            p.rrect(0.0, g(0.72), 0.06 * r, 0.1 * r, 0.0, 0.2, GOLD);
        }
        6 => {
            // crown
            let gold = rgb(1.0, 0.80, 0.22);
            p.rrect(0.0, g(0.78), 0.76 * r, 0.2 * r, 0.0, 0.25, gold);
            for (x, h) in [(-0.52f32, 0.36f32), (0.0, 0.52), (0.52, 0.36)] {
                p.tri(x * r, g(0.9) + h * r, 0.24 * r, h * r, 0.0, gold);
                p.ball(x * r, g(0.9) + 2.0 * h * r, 0.075 * r, 0.075 * r, rgb(1.0, 0.9, 0.5));
            }
            p.rrect(0.0, g(0.7), 0.76 * r, 0.04 * r, 0.0, 1.0, rgb(0.85, 0.62, 0.12));
            p.ball(0.0, g(0.8), 0.09 * r, 0.09 * r, rgb(0.85, 0.12, 0.2));
            p.ball(-0.45 * r, g(0.8), 0.07 * r, 0.07 * r, rgb(0.2, 0.4, 0.9));
            p.ball(0.45 * r, g(0.8), 0.07 * r, 0.07 * r, rgb(0.15, 0.7, 0.4));
        }
        7 => {
            // earmuffs
            let band = rgb(0.24, 0.27, 0.34);
            let n = 10;
            for i in 0..n {
                let a0 = PI * (0.97 - 0.94 * i as f32 / n as f32);
                let a1 = PI * (0.97 - 0.94 * (i + 1) as f32 / n as f32);
                let rad = 1.1 * r;
                p.line(rad * a0.cos(), cy + rad * a0.sin() * 0.98, rad * a1.cos(), cy + rad * a1.sin() * 0.98, 0.13 * r, band);
            }
            for sx in [-1.0f32, 1.0] {
                p.ball(sx * 1.08 * r, g(0.02), 0.3 * r, 0.38 * r, rgb(0.88, 0.2, 0.26));
                p.ell(sx * 1.06 * r, g(0.02), 0.17 * r, 0.24 * r, rgb(0.98, 0.5, 0.52));
            }
        }
        _ => {}
    }
}
