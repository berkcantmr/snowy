//! Gift box with ribbon and bow, plus its little parachute. Unit coordinates: 1.0 = box
//! height including the bow, y up, centered on x = 0.

use crate::util::*;
use crate::{band_round, Prim, PrimKind};

pub fn build(base: [f32; 2], height: f32, open: f32, t: f32, out: &mut Vec<Prim>) {
    let e = open * open * (3.0 - 2.0 * open);
    let mut p = Pen::new(out, base, height, 1.0);
    let red = rgb(0.86, 0.17, 0.25);
    let red_d = rgb(0.68, 0.10, 0.20);
    let gold = rgb(1.0, 0.82, 0.25);
    let gold_d = rgb(0.88, 0.62, 0.12);

    p.ell(0.0, 0.012, 0.52, 0.02, [0.1, 0.2, 0.4, 0.22]);
    if open > 0.0 {
        p.glow(0.0, 0.85, 0.8 * e, [1.0, 0.9, 0.5, 0.8 * e]);
    }
    // box body
    p.rrect(0.0, 0.33, 0.44, 0.33, 0.0, 0.12, red);
    p.rrect(-0.3, 0.33, 0.1, 0.3, 0.0, 0.3, red_d);
    p.rrect(0.0, 0.33, 0.085, 0.33, 0.0, 0.1, gold);
    p.rrect(0.0, 0.33, 0.03, 0.31, 0.0, 1.0, [1.0, 1.0, 1.0, 0.35]);
    if open > 0.0 {
        p.ell(0.0, 0.655, 0.4, 0.05, [0.18, 0.04, 0.08, 1.0]);
    }

    // lid + bow as one group that pops up and tilts around its center
    let (cx, cy) = (0.0f32, 0.74f32);
    let tilt = 0.75 * e;
    let (dx, dy) = (0.22 * e, 0.55 * e * (1.0 - 0.25 * e));
    let (sn, cs) = tilt.sin_cos();
    let lid = |x: f32, y: f32| -> (f32, f32) {
        let (lx, ly) = (x - cx, y - cy);
        (cx + dx + cs * lx - sn * ly, cy + dy + sn * lx + cs * ly)
    };
    let sway = (t * 2.6).sin() * 0.012;
    let (x, y) = lid(0.0, 0.74);
    p.rrect(x, y, 0.5, 0.115, tilt, 0.25, red_d);
    p.rrect(x, y + 0.02 * cs, 0.48, 0.07, tilt, 0.4, red);
    p.rrect(x, y, 0.085, 0.115, tilt, 0.1, gold);
    for side in [-1.0f32, 1.0] {
        let (x, y) = lid(side * 0.15, 0.9 + sway * side);
        p.ell_r(x, y, 0.14, 0.075, tilt + side * 0.5, gold);
        p.ell_r(x, y, 0.07, 0.035, tilt + side * 0.5, gold_d);
    }
    let (x, y) = lid(0.0, 0.88);
    p.ball(x, y, 0.065, 0.06, gold);

    // sparkle burst
    if open > 0.0 {
        let fade = (open * std::f32::consts::PI).sin().max(0.0);
        for i in 0..10u32 {
            let a = 0.4 + 2.3 * hash(i, 3);
            let r = (0.3 + 0.9 * hash(i, 5)) * e;
            let (px, py) = (a.cos() * r * 0.9, 0.7 + a.sin() * r * 1.1);
            let sz = 0.04 + 0.05 * hash(i, 9);
            let col = match i % 3 {
                0 => [1.0, 0.95, 0.6, fade],
                1 => [0.8, 0.95, 1.0, fade],
                _ => [1.0, 0.7, 0.85, fade],
            };
            if i % 2 == 0 {
                p.star(px, py, sz * (1.0 + 0.3 * (t * 9.0 + i as f32).sin()), t * 2.0 + i as f32, col);
            } else {
                p.glow(px, py, sz * 2.2, col);
            }
        }
    }
}

/// Parachute canopy and strings above a box of the same `height`, hooked to its top.
pub fn chute(base: [f32; 2], height: f32, t: f32, out: &mut Vec<Prim>) {
    let mut p = Pen::new(out, base, height, 1.0);
    let sw = (t * 2.2).sin() * 0.03;
    let top = 0.98;
    let cy = 2.05;
    let red = rgb(0.95, 0.3, 0.35);
    let white = rgb(1.0, 1.0, 1.0);
    for sx in [-0.7f32, -0.25, 0.25, 0.7] {
        p.line(sx + sw, cy - 0.05, sw * 0.2, top - 0.02, 0.012, [1.0, 1.0, 1.0, 0.8]);
    }
    let br = band_round(-0.05, 1.0);
    p.add(PrimKind::Band, sw, cy, 0.9, 0.62, 0.0, red, br);
    p.add(PrimKind::Band, sw, cy + 0.01, 0.62, 0.62, 0.0, white, br);
    p.add(PrimKind::Band, sw, cy + 0.02, 0.3, 0.62, 0.0, red, br);
    for k in 0..4 {
        let x = -0.675 + 0.45 * k as f32;
        p.ell(x + sw, cy - 0.03, 0.115, 0.06, if k % 2 == 0 { red } else { white });
    }
}
