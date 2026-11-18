//! Private drawing helpers shared by the builders. Builders work in "unit" coordinates:
//! x right, y UP, origin at the prop's base, 1.0 = the prop's height; angles are
//! counter-clockwise (math convention). `Pen` converts to screen pixels.

#![allow(clippy::too_many_arguments)]

use crate::{Prim, PrimKind};

pub type Col = [f32; 4];

pub const fn rgb(r: f32, g: f32, b: f32) -> Col {
    [r, g, b, 1.0]
}

pub fn mul(c: Col, f: f32) -> Col {
    [(c[0] * f).clamp(0.0, 1.0), (c[1] * f).clamp(0.0, 1.0), (c[2] * f).clamp(0.0, 1.0), c[3]]
}

pub fn hsv(h: f32, s: f32, v: f32) -> Col {
    let h = h.rem_euclid(1.0) * 6.0;
    let f = h - h.floor();
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match h as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b, 1.0]
}

/// Deterministic hash of (i, salt) to 0..1. No RNG state anywhere.
pub fn hash(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B).wrapping_add(0x27D4_EB2F);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

pub struct Pen<'a> {
    pub out: &'a mut Vec<Prim>,
    base: [f32; 2],
    k: f32,
    flip: f32,
}

impl<'a> Pen<'a> {
    /// `k` = pixels per unit; `flip` = +1 or -1 mirrors horizontally.
    pub fn new(out: &'a mut Vec<Prim>, base: [f32; 2], k: f32, flip: f32) -> Self {
        Pen { out, base, k, flip }
    }

    pub fn add(&mut self, kind: PrimKind, x: f32, y: f32, hx: f32, hy: f32, rot: f32, color: Col, round: f32) {
        self.out.push(Prim {
            kind,
            pos: [self.base[0] + x * self.flip * self.k, self.base[1] - y * self.k],
            size: [(hx * self.k).max(1e-3), (hy * self.k).max(1e-3)],
            rot: -rot * self.flip,
            color,
            round,
        });
    }

    pub fn ell(&mut self, x: f32, y: f32, hx: f32, hy: f32, c: Col) {
        self.add(PrimKind::Ellipse, x, y, hx, hy, 0.0, c, 0.0);
    }
    pub fn ell_r(&mut self, x: f32, y: f32, hx: f32, hy: f32, rot: f32, c: Col) {
        self.add(PrimKind::Ellipse, x, y, hx, hy, rot, c, 0.0);
    }
    pub fn ball(&mut self, x: f32, y: f32, hx: f32, hy: f32, c: Col) {
        self.add(PrimKind::Ball, x, y, hx, hy, 0.0, c, 0.0);
    }
    pub fn rrect(&mut self, x: f32, y: f32, hx: f32, hy: f32, rot: f32, round: f32, c: Col) {
        self.add(PrimKind::RoundRect, x, y, hx, hy, rot, c, round);
    }
    /// Triangle with apex up before rotation; `hx`/`hy` = half width / half height.
    pub fn tri(&mut self, x: f32, y: f32, hx: f32, hy: f32, rot: f32, c: Col) {
        self.add(PrimKind::Triangle, x, y, hx, hy, rot, c, 0.0);
    }
    pub fn star(&mut self, x: f32, y: f32, r: f32, rot: f32, c: Col) {
        self.add(PrimKind::Star, x, y, r, r, rot, c, 0.0);
    }
    pub fn glow(&mut self, x: f32, y: f32, r: f32, c: Col) {
        self.add(PrimKind::Glow, x, y, r, r, 0.0, c, 0.0);
    }
    /// `thick` = ring thickness as fraction of the radius.
    pub fn ring(&mut self, x: f32, y: f32, hx: f32, hy: f32, thick: f32, c: Col) {
        self.add(PrimKind::Ring, x, y, hx, hy, 0.0, c, thick);
    }
    pub fn ring_r(&mut self, x: f32, y: f32, hx: f32, hy: f32, rot: f32, thick: f32, c: Col) {
        self.add(PrimKind::Ring, x, y, hx, hy, rot, c, thick);
    }
    /// Round-capped stroke of width `w` from (x0,y0) to (x1,y1).
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: Col) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = dx.hypot(dy);
        self.add(PrimKind::Capsule, (x0 + x1) * 0.5, (y0 + y1) * 0.5, len * 0.5 + w * 0.5, w * 0.5, dy.atan2(dx), c, 0.0);
    }
    /// Stroke from a point at angle `a` and length `len`.
    pub fn ray(&mut self, x: f32, y: f32, a: f32, len: f32, w: f32, c: Col) {
        self.line(x, y, x + a.cos() * len, y + a.sin() * len, w, c);
    }
}
