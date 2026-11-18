//! A string of lights hanging between two points: sagging wire, end clips and evenly spaced
//! bulbs with a glow halo each. Works directly in screen pixels (y down); `unit` scales all
//! sizes (1.0 = 1080p).

use crate::util::*;
use crate::{Prim, PrimKind};
use snowy_core::props::LightsLook;
use std::f32::consts::{PI, TAU};

/// Bulb spacing in units, per bulb style.
const SPACING: [f32; 8] = [46.0, 46.0, 24.0, 46.0, 50.0, 44.0, 40.0, 50.0];
/// Most bulbs per string (keeps the primitive count of a long string bounded).
const MAX_BULBS: [usize; 8] = [38, 38, 70, 38, 38, 38, 38, 38];
const MAX_WIRE_SEGS: usize = 48;

fn finite2(p: [f32; 2]) -> [f32; 2] {
    [if p[0].is_finite() { p[0] } else { 0.0 }, if p[1].is_finite() { p[1] } else { 0.0 }]
}

/// Base color of bulb `i` of `n` for the `colors` option.
fn bulb_color(colors: u8, i: usize, n: usize, t: f32) -> Col {
    match colors % 8 {
        0 => rgb(1.0, 0.82, 0.45),
        1 => [rgb(1.0, 0.22, 0.25), rgb(0.25, 0.9, 0.35), rgb(0.3, 0.55, 1.0), rgb(1.0, 0.85, 0.25), rgb(1.0, 0.5, 0.82)][i % 5],
        2 => [rgb(0.22, 0.48, 1.0), rgb(0.55, 0.82, 1.0)][i % 2],
        3 => [rgb(1.0, 0.2, 0.25), rgb(0.2, 0.88, 0.35)][i % 2],
        4 => hsv(i as f32 / n.max(1) as f32 * 1.3 - t * 0.12, 0.85, 1.0),
        5 => {
            if i.is_multiple_of(2) {
                rgb(1.0, 0.76, 0.2)
            } else {
                rgb(1.0, 0.86, 0.45)
            }
        }
        6 => [rgb(0.68, 0.32, 1.0), rgb(0.9, 0.55, 1.0)][i % 2],
        _ => [rgb(0.96, 0.18, 0.28), rgb(1.0, 1.0, 1.0)][i % 2],
    }
}

/// How lit bulb `i` is (0.2..1.0) at time `t` for the `pattern` option.
fn level(pattern: u8, i: usize, t: f32) -> f32 {
    let f = i as f32;
    let h = hash(i as u32, 11);
    let l = match pattern % 8 {
        0 => 1.0,
        1 => 0.5 + 0.5 * (t * (1.6 + 2.4 * h) + h * TAU).sin(),
        2 => 0.5 + 0.5 * (t * 0.9).sin(),
        3 => {
            let d = (f - t * 4.0).rem_euclid(4.0);
            (1.0 - d / 1.6).clamp(0.0, 1.0)
        }
        4 => {
            let phase = ((t / 0.8).floor() as i64).rem_euclid(2) as usize;
            if phase == i % 2 {
                1.0
            } else {
                0.0
            }
        }
        5 => {
            let slot = (t * 4.0 + h * 10.0).floor();
            if hash(i as u32, slot as i32 as u32) > 0.82 {
                1.0
            } else {
                0.3
            }
        }
        6 => 0.5 + 0.5 * (t * 3.2 - f * 0.55).sin(),
        _ => {
            let ph = (t / 1.4).fract() * 1.4;
            let bump = |c: f32| (-((ph - c) / 0.07).powi(2)).exp();
            (bump(0.1) + 0.75 * bump(0.38)).min(1.0)
        }
    };
    0.2 + 0.8 * l.clamp(0.0, 1.0)
}

fn wire_color(wire: u8) -> Option<Col> {
    Some(match wire % 8 {
        0 => rgb(0.1, 0.42, 0.2),
        1 => rgb(0.95, 0.96, 0.98),
        2 => rgb(0.08, 0.08, 0.1),
        3 => rgb(0.95, 0.74, 0.22),
        4 => rgb(0.78, 0.45, 0.26),
        5 => rgb(0.78, 0.8, 0.86),
        6 => rgb(0.85, 0.14, 0.2),
        _ => return None,
    })
}

struct Px<'a> {
    out: &'a mut Vec<Prim>,
}

impl Px<'_> {
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, kind: PrimKind, pos: [f32; 2], hx: f32, hy: f32, rot: f32, color: Col, round: f32) {
        self.out.push(Prim { kind, pos, size: [hx.max(1e-3), hy.max(1e-3)], rot, color, round });
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build(look: &LightsLook, a: [f32; 2], b: [f32; 2], sag: f32, t: f32, sway: f32, unit: f32, out: &mut Vec<Prim>) {
    let a = finite2(a);
    let b = finite2(b);
    let sag = if sag.is_finite() { sag.max(0.0) } else { 0.0 };
    let sway = if sway.is_finite() { sway.clamp(-0.5, 0.5) } else { 0.0 };
    let u = if unit.is_finite() { unit.clamp(0.25, 8.0) } else { 1.0 };
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0].hypot(d[1]).max(1.0);
    let style = (look.bulbs % 8) as usize;
    let sway_px = sway * (0.15 * len + 2.0 * sag);
    let at = |s: f32| -> [f32; 2] {
        let k = 4.0 * s * (1.0 - s);
        [a[0] + d[0] * s + sway_px * k, a[1] + d[1] * s + sag * k]
    };
    let mut px = Px { out };

    // Wire.
    let wire = wire_color(look.wire);
    if let Some(wc) = wire {
        let segs = ((len / (40.0 * u)).ceil() as usize).clamp(4, MAX_WIRE_SEGS);
        let w = 1.2 * u;
        let mut prev = at(0.0);
        for k in 1..=segs {
            let cur = at(k as f32 / segs as f32);
            let (dx, dy) = (cur[0] - prev[0], cur[1] - prev[1]);
            let l = dx.hypot(dy);
            px.add(PrimKind::Capsule, [(prev[0] + cur[0]) * 0.5, (prev[1] + cur[1]) * 0.5], l * 0.5 + w * 0.5, w * 0.5, dy.atan2(dx), wc, 0.0);
            prev = cur;
        }
    }

    // End clips.
    let clip = rgb(0.62, 0.64, 0.7);
    let clip_d = rgb(0.34, 0.36, 0.42);
    for (s, e) in [(0.0f32, a), (1.0, b)] {
        let (p0, p1) = (at((s - 0.01).max(0.0)), at((s + 0.01).min(1.0)));
        let rot = (p1[1] - p0[1]).atan2(p1[0] - p0[0]);
        px.add(PrimKind::RoundRect, e, 5.5 * u, 3.4 * u, rot, clip_d, 0.5);
        px.add(PrimKind::Ring, e, 3.4 * u, 3.4 * u, 0.0, clip, 0.4);
    }

    // Bulbs.
    let spacing = SPACING[style] * u;
    let n = ((len / spacing).round() as usize).clamp(2, MAX_BULBS[style]);
    let socket_col = wire.map(|c| mul(c, 0.7)).unwrap_or(rgb(0.3, 0.32, 0.36));
    let u = u * 1.25;
    for i in 0..n {
        let s = 0.03 + 0.94 * (i as f32 + 0.5) / n as f32;
        let p = at(s);
        let tilt = sway * 3.0 * (0.7 + 0.6 * hash(i as u32, 1));
        let (sn, cs) = tilt.sin_cos();
        let down = [-sn, cs];
        let right = [cs, sn];
        // local (x right, y down along the bulb) -> world
        let at_l = |x: f32, y: f32| [p[0] + (right[0] * x + down[0] * y) * u, p[1] + (right[1] * x + down[1] * y) * u];
        let lv = level(look.pattern, i, t);
        let base = bulb_color(look.colors, i, n, t);
        let lit = [
            base[0] * (0.3 + 0.7 * lv),
            base[1] * (0.3 + 0.7 * lv),
            base[2] * (0.3 + 0.7 * lv),
            1.0,
        ];
        let glow_a = (1.0 * lv * lv).min(1.0);
        let glow = [base[0], base[1], base[2], glow_a];
        let hi = [1.0, 1.0, 1.0, 0.45 * lv];
        if style != 2 {
            px.add(PrimKind::RoundRect, at_l(0.0, 3.0), 2.6 * u, 3.0 * u, tilt, socket_col, 0.4);
        }
        match style {
            0 => {
                let c = at_l(0.0, 12.0);
                px.add(PrimKind::Glow, c, 16.0 * u * (0.7 + 0.4 * lv), 16.0 * u * (0.7 + 0.4 * lv), 0.0, glow, 0.0);
                px.add(PrimKind::Ellipse, c, 4.8 * u, 6.2 * u, tilt, lit, 0.0);
                px.add(PrimKind::Ellipse, at_l(-1.6, 9.5), 1.3 * u, 2.0 * u, tilt, hi, 0.0);
            }
            1 => {
                let c = at_l(0.0, 11.0);
                px.add(PrimKind::Glow, c, 16.0 * u * (0.7 + 0.4 * lv), 16.0 * u * (0.7 + 0.4 * lv), 0.0, glow, 0.0);
                px.add(PrimKind::Ellipse, at_l(0.0, 10.0), 4.6 * u, 4.4 * u, tilt, lit, 0.0);
                px.add(PrimKind::Triangle, at_l(0.0, 15.0), 4.6 * u, 5.4 * u, tilt + PI, lit, 0.0);
            }
            2 => {
                let c = at_l(0.0, 3.5);
                px.add(PrimKind::Glow, c, 10.0 * u * (0.7 + 0.4 * lv), 10.0 * u * (0.7 + 0.4 * lv), 0.0, glow, 0.0);
                px.add(PrimKind::Ellipse, c, 2.7 * u, 2.7 * u, 0.0, lit, 0.0);
            }
            3 => {
                let c = at_l(0.0, 13.0);
                px.add(PrimKind::Glow, c, 17.0 * u * (0.7 + 0.4 * lv), 17.0 * u * (0.7 + 0.4 * lv), 0.0, glow, 0.0);
                px.add(PrimKind::Star, c, 8.0 * u, 8.0 * u, tilt, lit, 0.0);
            }
            4 => {
                let c = at_l(0.0, 13.0);
                px.add(PrimKind::Glow, c, 16.0 * u * (0.7 + 0.4 * lv), 16.0 * u * (0.7 + 0.4 * lv), 0.0, glow, 0.0);
                for k in 0..3 {
                    let ang = tilt + k as f32 * PI / 3.0 + PI / 2.0;
                    px.add(PrimKind::Capsule, c, 7.0 * u, 1.0 * u, ang, lit, 0.0);
                }
            }
            5 => {
                let c = at_l(0.0, 12.0);
                px.add(PrimKind::Glow, c, 16.0 * u * (0.7 + 0.4 * lv), 16.0 * u * (0.7 + 0.4 * lv), 0.0, glow, 0.0);
                px.add(PrimKind::Ellipse, at_l(-2.8, 10.0), 3.8 * u, 3.8 * u, tilt, lit, 0.0);
                px.add(PrimKind::Ellipse, at_l(2.8, 10.0), 3.8 * u, 3.8 * u, tilt, lit, 0.0);
                px.add(PrimKind::Triangle, at_l(0.0, 14.6), 6.6 * u, 6.0 * u, tilt + PI, lit, 0.0);
            }
            6 => {
                let c = at_l(0.0, 14.0);
                px.add(PrimKind::Glow, c, 14.0 * u * (0.7 + 0.4 * lv), 14.0 * u * (0.7 + 0.4 * lv), 0.0, glow, 0.0);
                px.add(PrimKind::Triangle, at_l(0.0, 15.0), 3.6 * u, 9.0 * u, tilt + PI, lit, 0.0);
            }
            _ => {
                let c = at_l(0.0, 14.0);
                px.add(PrimKind::Glow, c, 17.0 * u * (0.7 + 0.4 * lv), 17.0 * u * (0.7 + 0.4 * lv), 0.0, glow, 0.0);
                let cap = rgb(0.25, 0.22, 0.2);
                px.add(PrimKind::RoundRect, at_l(0.0, 7.8), 4.0 * u, 1.4 * u, tilt, cap, 0.5);
                px.add(PrimKind::RoundRect, c, 4.8 * u, 5.4 * u, tilt, lit, 0.3);
                px.add(PrimKind::RoundRect, at_l(0.0, 20.2), 3.4 * u, 1.2 * u, tilt, cap, 0.5);
            }
        }
    }
}
