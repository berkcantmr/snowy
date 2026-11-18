//! Friendly fluffy yeti, facing right in unit coordinates (1.0 = height incl. horns, y up).
//! `flip` mirrors it for dir = -1.

use crate::util::*;
use crate::{Prim, YetiPose};
use std::f32::consts::{PI, TAU};

pub fn build(base: [f32; 2], height: f32, pose: YetiPose, dir: f32, t: f32, out: &mut Vec<Prim>) {
    let mut p = Pen::new(out, base, height, dir);
    let fur = rgb(0.90, 0.95, 1.0);
    let fur_d = rgb(0.72, 0.83, 0.97);
    let white = rgb(1.0, 1.0, 1.0);
    let patch = rgb(0.19, 0.25, 0.43);
    let horn = rgb(0.99, 0.91, 0.72);
    let toe = rgb(0.60, 0.74, 0.95);
    let ink = rgb(0.07, 0.09, 0.16);
    let scared = pose.scared;

    let s = if pose.walk_phase.is_finite() { pose.walk_phase } else { 0.0 };
    let by = if scared { 0.012 * (t * 22.0).sin().abs() } else { 0.016 * (2.0 * s).sin().abs() };

    p.ell(0.0, 0.012, 0.44, 0.022, [0.1, 0.2, 0.4, 0.2]);

    // arms: a far one behind the body, a near one in front.
    let wob = (t * 20.0).sin() * 0.03;
    let arm_hand = |side: f32| -> (f32, f32) {
        if scared {
            (side * 0.46 + wob * side, 0.9 + wob)
        } else if pose.carrying {
            (side * 0.3, 0.97)
        } else {
            let a = (s + if side > 0.0 { PI } else { 0.0 }).sin() * 0.55;
            (side * 0.27 + a * 0.2, 0.28 - a.abs() * 0.04)
        }
    };
    let arm = |p: &mut Pen, side: f32, col: Col| {
        let (hx, hy) = arm_hand(side);
        let (sx, sy) = (side * 0.24, 0.52 + by);
        p.line(sx, sy, hx, hy, 0.17, col);
        p.ball(hx, hy, 0.095, 0.09, col);
        p.ball((sx + hx) * 0.5, (sy + hy) * 0.5, 0.065, 0.06, white);
    };
    arm(&mut p, -1.0, fur_d);

    // body
    p.ball(0.0, 0.38 + by, 0.35, 0.33, fur);
    p.ball(0.05, 0.33 + by, 0.21, 0.2, white);
    for i in 0..12 {
        let a = i as f32 / 12.0 * TAU + 0.25;
        if a.sin() > 0.55 {
            continue;
        }
        let j = hash(i, 7);
        p.ball(0.34 * a.cos(), 0.38 + by + 0.32 * a.sin(), 0.075 + 0.02 * j, 0.065 + 0.02 * j, fur);
    }

    // feet and short legs
    for side in [-1.0f32, 1.0] {
        let l = if scared {
            ((t * 18.0) + if side > 0.0 { PI } else { 0.0 }).sin()
        } else {
            (s + if side > 0.0 { 0.0 } else { PI }).sin()
        };
        let lift = l.max(0.0) * 0.06;
        let step = l * 0.07;
        let fx = side * 0.16 + step;
        let fy = 0.076 + lift;
        p.ball(fx * 0.9, 0.17 + lift * 0.5, 0.12, 0.1, fur_d);
        p.ball(fx + 0.04, fy, 0.18, 0.075, fur);
        for k in 0..3 {
            p.ball(fx + 0.15 + if k == 1 { 0.01 } else { 0.0 }, fy - 0.025 + 0.027 * k as f32, 0.035, 0.032, toe);
        }
    }

    // head with a crown of fluff
    let hx0 = 0.04;
    let hy0 = 0.70 + by;
    p.tri(hx0 - 0.12, hy0 + 0.24, 0.05, 0.08, 0.3, horn);
    p.tri(hx0 + 0.16, hy0 + 0.24, 0.05, 0.08, -0.25, horn);
    p.ball(hx0, hy0, 0.28, 0.22, fur);
    for (i, a) in [0.25f32, 0.7, 1.1, 1.55, 2.0, 2.45, 2.9].iter().enumerate() {
        let j = hash(i as u32, 11);
        p.ball(hx0 + 0.27 * a.cos(), hy0 + 0.2 * a.sin(), 0.065 + 0.015 * j, 0.06 + 0.015 * j, fur);
    }
    p.ball(hx0 - 0.02, hy0 + 0.2, 0.08, 0.06, white);

    // face patch
    let (fx, fy) = (hx0 + 0.04, hy0 - 0.02);
    p.ell(fx, fy, 0.2, 0.15, patch);
    p.ell(fx + 0.02, fy + 0.07, 0.14, 0.05, [0.28, 0.35, 0.55, 0.6]);
    let (er, pr) = if scared { (0.07, 0.016) } else { (0.056, 0.031) };
    for ex in [fx - 0.075, fx + 0.075] {
        p.ell(ex, fy + 0.04, er, er * 1.15, white);
        let off = if scared { 0.0 } else { 0.014 };
        p.ell(ex + off, fy + 0.035, pr, pr * 1.2, ink);
        if !scared {
            p.ell(ex + off + 0.01, fy + 0.05, 0.01, 0.011, white);
        }
    }
    p.ell(fx + 0.01, fy - 0.025, 0.022, 0.014, [0.36, 0.44, 0.65, 1.0]);
    if scared {
        p.ell(fx, fy - 0.08, 0.035, 0.045, rgb(0.55, 0.15, 0.25));
    } else {
        p.ell(fx, fy - 0.075, 0.085, 0.055, rgb(0.9, 0.38, 0.45));
        p.ell(fx, fy - 0.035, 0.1, 0.05, patch);
        p.rrect(fx, fy - 0.047, 0.06, 0.014, 0.0, 0.9, white);
    }
    p.ell(fx - 0.14, fy - 0.045, 0.035, 0.022, [1.0, 0.55, 0.62, 0.55]);
    p.ell(fx + 0.14, fy - 0.045, 0.035, 0.022, [1.0, 0.55, 0.62, 0.55]);

    arm(&mut p, 1.0, fur);
    if scared {
        let d = (t * 9.0).fract();
        p.ball(-0.26, 0.96 - 0.05 * d, 0.028, 0.04, [0.6, 0.85, 1.0, 1.0 - d * 0.5]);
    }
}
