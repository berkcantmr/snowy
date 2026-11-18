//! Shared helpers for the effect simulations.

use fastrand::Rng;

pub const REF_W: f32 = 1920.0;
pub const REF_H: f32 = 1080.0;

/// Clamp dt to a safe range, tolerating NaN/inf.
pub fn clamp_dt(dt: f32) -> f32 {
    if dt.is_finite() {
        dt.clamp(0.0, 0.1)
    } else {
        0.0
    }
}

pub fn sanitize(w: f32, h: f32) -> (f32, f32) {
    let f = |v: f32| if v.is_finite() { v.max(1.0) } else { 1.0 };
    (f(w), f(h))
}

/// Pixel-size scale relative to 1080p height.
pub fn ui_scale(_w: f32, h: f32) -> f32 {
    (h / REF_H).clamp(0.6, 3.0)
}

/// Particle count scaled by screen area and intensity.
pub fn target_count(base: f32, w: f32, h: f32, intensity: f32) -> usize {
    let area = (w * h) / (REF_W * REF_H);
    let i = if intensity.is_finite() { intensity.clamp(0.1, 4.0) } else { 1.0 };
    ((base * area * i).round() as usize).max(1)
}

pub fn speed_mul(speed: f32) -> f32 {
    if speed.is_finite() {
        speed.clamp(0.1, 4.0)
    } else {
        1.0
    }
}

pub fn wind_val(wind: f32) -> f32 {
    if wind.is_finite() {
        wind.clamp(-2.0, 2.0)
    } else {
        0.0
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn range(rng: &mut Rng, a: f32, b: f32) -> f32 {
    a + (b - a) * rng.f32()
}

/// Rotation that makes the quad's local +Y axis point along (vx, vy).
pub fn rot_for(vx: f32, vy: f32) -> f32 {
    (-vx).atan2(vy)
}

pub fn wrap(v: f32, lo: f32, hi: f32) -> f32 {
    let span = hi - lo;
    if v < lo {
        v + span
    } else if v > hi {
        v - span
    } else {
        v
    }
}
