//! Procedural art for desktop props, as a renderer-neutral list of primitives.
//! The same primitives are drawn by the GPU (via `to_instance` + `SHADER`) on the desktop
//! and by egui in the editor previews, so both always look the same.
//! FROZEN CONTRACT: Prim, PrimKind, the builder signatures, to_instance, SHADER, bounds.
//! Builders are procedural and deterministic (hash-based, no RNG state).

use snowy_core::props::{LightsLook, SnowmanLook, TreeLook};
use snowy_core::Instance;

mod gift;
mod lights;
mod plough;
mod snowman;
mod tree;
mod util;
mod yeti;

/// Primitive shapes. Numeric value goes into Instance.param.w for the shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimKind {
    /// Flat filled ellipse (half extents = size).
    Ellipse = 0,
    /// Shaded sphere-like ellipse: lit from top-left, soft shadow bottom-right. For snowballs.
    Ball = 1,
    /// Rounded rectangle; `round` = corner radius as fraction of the smaller half extent.
    RoundRect = 2,
    /// Isosceles triangle pointing up (apex at top-center, base at bottom).
    Triangle = 3,
    /// Capsule/line: a stroke from left to right center of the box, thickness = 2*size.y.
    Capsule = 4,
    /// Five-pointed star filling the box.
    Star = 5,
    /// Soft glowing dot (additive-looking halo), e.g. tree lights. Not affected by `round`.
    Glow = 6,
    /// Ellipse outline ring; `round` = ring thickness as fraction of the radius.
    Ring = 7,
    /// The ellipse of the box, kept only between two heights: `round` packs the band as
    /// `band_round(lo, hi)` with lo/hi in -1 (bottom) .. 1 (top) of the ellipse. Shaded like
    /// fabric wrapped around a ball (darker rim and lower part). For clothes on snowmen.
    Band = 8,
}

/// Pack a vertical band (-1 bottom .. 1 top of an ellipse) into `Prim::round` for
/// `PrimKind::Band`. 10 bits per edge.
pub fn band_round(lo: f32, hi: f32) -> f32 {
    let q = |v: f32| ((v.clamp(-1.0, 1.0) * 0.5 + 0.5) * 1023.0).round();
    (q(lo) + q(hi) * 1024.0) / 1_048_576.0
}

/// Inverse of `band_round`: (lo, hi) in -1..1.
pub fn band_unpack(round: f32) -> (f32, f32) {
    let v = (round.clamp(0.0, 1.0) * 1_048_576.0).round();
    let hi = (v / 1024.0).floor();
    let lo = v - hi * 1024.0;
    (lo / 1023.0 * 2.0 - 1.0, hi / 1023.0 * 2.0 - 1.0)
}

/// One shape, in pixels. y grows downward (screen coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prim {
    pub kind: PrimKind,
    /// Center.
    pub pos: [f32; 2],
    /// Half extents (x, y) before rotation.
    pub size: [f32; 2],
    /// Radians, clockwise on screen.
    pub rot: f32,
    /// Straight RGBA 0..1.
    pub color: [f32; 4],
    /// Shape parameter, see PrimKind docs (0..1).
    pub round: f32,
}

/// Draw a snowman standing with its bottom-center at `base`. `height` is the total height
/// in px (hat included). `t` is seconds (for gentle animation, e.g. lantern flicker).
/// `balls` is 2 or 3. Appends primitives back-to-front.
pub fn snowman(look: &SnowmanLook, balls: u8, base: [f32; 2], height: f32, t: f32, out: &mut Vec<Prim>) {
    snowman::build(look, balls, base, sane(height), time(t), out);
}

/// Draw a Christmas tree with bottom-center (trunk bottom) at `base`, total height `height`.
/// `grow` 0..1 animates planting (0 = sapling, 1 = full size). `t` for twinkling lights.
pub fn tree(look: &TreeLook, base: [f32; 2], height: f32, grow: f32, t: f32, out: &mut Vec<Prim>) {
    let grow = if grow.is_finite() { grow.clamp(0.05, 1.0) } else { 1.0 };
    tree::build(look, base, sane(height) * grow, time(t), out);
}

/// Snowplough facing `dir` (+1 right, -1 left), bottom-center at `base`, height `height`.
/// `t` for wheel rotation / blinking light.
pub fn plough(base: [f32; 2], height: f32, dir: f32, t: f32, out: &mut Vec<Prim>) {
    plough::build(base, sane(height), if dir < 0.0 { -1.0 } else { 1.0 }, time(t), out);
}

/// Pose of the yeti visitor.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct YetiPose {
    /// Walk cycle phase in radians (one full step cycle = TAU).
    pub walk_phase: f32,
    /// Arms raised to carry something above its head.
    pub carrying: bool,
    /// Wide eyes, round mouth, arms flung up.
    pub scared: bool,
}

/// Friendly yeti facing `dir` (+1 right, -1 left), bottom-center at `base`, total height
/// `height` (horns included). Carried items are drawn by the caller above its head.
pub fn yeti(base: [f32; 2], height: f32, pose: YetiPose, dir: f32, t: f32, out: &mut Vec<Prim>) {
    yeti::build(base, sane(height), pose, if dir < 0.0 { -1.0 } else { 1.0 }, time(t), out);
}

/// Gift box (ribbon and bow) centered on `base.x`, `height` px tall. `open` 0 = closed ..
/// 1 = fully open (lid pops up and tilts, sparkles burst out).
pub fn gift(base: [f32; 2], height: f32, open: f32, t: f32, out: &mut Vec<Prim>) {
    let open = if open.is_finite() { open.clamp(0.0, 1.0) } else { 0.0 };
    gift::build(base, sane(height), open, time(t), out);
}

/// Parachute canopy + strings that go with `gift` (same `base`/`height`), drawn above the box.
pub fn gift_chute(base: [f32; 2], height: f32, t: f32, out: &mut Vec<Prim>) {
    gift::chute(base, sane(height), time(t), out);
}

/// A string of lights from `a_px` to `b_px` (screen pixels), the wire sagging `sag_px` below
/// the straight line at its middle. Bulbs are evenly spaced (count grows with the length),
/// animated by `look.pattern` from `t`; `sway` (about -0.1..0.1) swings the string sideways.
/// Sizes are for 1080p; see `lights_scaled`.
pub fn lights(look: &LightsLook, a_px: [f32; 2], b_px: [f32; 2], sag_px: f32, t: f32, sway: f32, out: &mut Vec<Prim>) {
    lights::build(look, a_px, b_px, sag_px, time(t), sway, 1.0, out);
}

/// Like `lights`, with all sizes multiplied by `unit` (monitor height / 1080).
#[allow(clippy::too_many_arguments)]
pub fn lights_scaled(look: &LightsLook, a_px: [f32; 2], b_px: [f32; 2], sag_px: f32, t: f32, sway: f32, unit: f32, out: &mut Vec<Prim>) {
    lights::build(look, a_px, b_px, sag_px, time(t), sway, unit, out);
}

/// Non-finite times would poison every animated position; treat them as 0.
fn time(t: f32) -> f32 {
    if t.is_finite() {
        t
    } else {
        0.0
    }
}

fn sane(h: f32) -> f32 {
    if h.is_finite() {
        h.max(1.0)
    } else {
        1.0
    }
}

/// Axis-aligned bounds [x0, y0, x1, y1] of the primitives (rotation-safe, conservative:
/// the rotated bounding box of every primitive's box).
pub fn bounds(prims: &[Prim]) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in prims {
        let (s, c) = p.rot.sin_cos();
        let hw = c.abs() * p.size[0] + s.abs() * p.size[1];
        let hh = s.abs() * p.size[0] + c.abs() * p.size[1];
        b[0] = b[0].min(p.pos[0] - hw);
        b[1] = b[1].min(p.pos[1] - hh);
        b[2] = b[2].max(p.pos[0] + hw);
        b[3] = b[3].max(p.pos[1] + hh);
    }
    if prims.is_empty() { [0.0; 4] } else { b }
}

/// Extra quad margin in px so box-filling shapes get anti-aliased edges inside the quad.
const PAD: f32 = 1.5;

/// GPU instance for a primitive. Instance.size = max half extent (+ a small anti-aliasing
/// margin); param = [sx/size, sy/size, round, kind as f32] so the shader can rebuild the
/// box (half extents = param.xy * size) inside the square quad.
pub fn to_instance(p: &Prim) -> Instance {
    let s = p.size[0].max(p.size[1]).max(0.01) + PAD;
    Instance {
        pos: p.pos,
        size: s,
        rot: p.rot,
        color: p.color,
        param: [p.size[0] / s, p.size[1] / s, p.round, p.kind as u8 as f32],
    }
}

/// HLSL drawing every PrimKind, following snowy_core::SHADER_CONTRACT.
pub const SHADER: &str = include_str!("shaders/props.hlsl");
