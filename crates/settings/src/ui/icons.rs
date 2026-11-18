//! Procedural icons painted with egui's Painter (no image files).
//! `t` is an animation clock in seconds; pass 0.0 for the static pose.

use eframe::egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};
use snowy_core::{EffectChoice, EffectKind};
use std::f32::consts::{PI, TAU};

use super::theme::Palette;

const ICE: Color32 = Color32::from_rgb(110, 175, 235);
const RAIN: Color32 = Color32::from_rgb(70, 140, 225);
const GLOW: Color32 = Color32::from_rgb(235, 215, 70);
const PINK: Color32 = Color32::from_rgb(244, 140, 178);
const PINK_DARK: Color32 = Color32::from_rgb(214, 96, 140);
const LEAF: Color32 = Color32::from_rgb(230, 130, 40);
const STAR: Color32 = Color32::from_rgb(250, 200, 60);
const GREEN: Color32 = Color32::from_rgb(110, 185, 90);

/// Whether an icon actually changes with `t` (used to decide if repaints are needed).
pub fn is_animated(choice: EffectChoice) -> bool {
    matches!(choice, EffectChoice::Fixed(_) | EffectChoice::Weather)
}

/// Sun peeking out from behind a cloud, with a couple of drops falling from it.
fn weather(p: &Painter, rect: Rect, t: f32) {
    let r = rect.width().min(rect.height()) * 0.5;
    let c = rect.center();
    // Sun (upper right), rays turn slowly while animated.
    let sun = c + vec2(r * 0.3, -r * 0.28);
    let sr = r * 0.34;
    let ray = Stroke::new((r * 0.09).max(1.3), GLOW);
    for i in 0..8 {
        let a = t * 0.5 + i as f32 * TAU / 8.0;
        p.line_segment([at(sun, a, sr * 1.3), at(sun, a, sr * 1.75)], ray);
    }
    p.circle_filled(sun, sr, GLOW);
    p.circle_filled(sun, sr * 0.7, Color32::from_rgb(255, 238, 130));
    // Cloud (lower left) overlapping the sun.
    let cloud = Color32::from_rgb(232, 238, 246);
    let edge = Stroke::new(1.0, Color32::from_rgb(160, 175, 195));
    let base = c + vec2(-r * 0.1, r * 0.1);
    let bob = (t * 1.5).sin() * r * 0.03;
    let puffs = [(-0.42, 0.1, 0.26), (-0.12, -0.12, 0.34), (0.22, 0.0, 0.27)];
    for (x, y, pr) in puffs {
        p.circle_stroke(base + vec2(x * r, y * r + bob), pr * r, edge);
    }
    let body = Rect::from_min_max(base + vec2(-0.42 * r, 0.0 + bob), base + vec2(0.22 * r, 0.36 * r + bob));
    for (x, y, pr) in puffs {
        p.circle_filled(base + vec2(x * r, y * r + bob), pr * r, cloud);
    }
    p.rect_filled(body, 0.0, cloud);
    p.line_segment([body.left_bottom(), body.right_bottom()], edge);
    // Drops under the cloud.
    for (i, x) in [-0.3f32, 0.05].into_iter().enumerate() {
        let fall = if t == 0.0 { 0.0 } else { wrap01(t * 0.7 + i as f32 * 0.5) };
        let alpha = if t == 0.0 { 1.0 } else { (fall.min(1.0 - fall) * 4.0).clamp(0.0, 1.0) };
        let tip = base + vec2(x * r, 0.5 * r + fall * r * 0.25);
        drop(p, tip, r * 0.3, RAIN.gamma_multiply(alpha));
    }
}

pub fn paint_choice(p: &Painter, rect: Rect, choice: EffectChoice, t: f32, pal: &Palette) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.5;
    match choice {
        EffectChoice::Fixed(EffectKind::Snow) => snowflake(p, c, r * 0.85, t * 0.6, ICE, r * 0.1),
        EffectChoice::Fixed(EffectKind::HeavySnow) => heavy_snow(p, rect, t),
        EffectChoice::Fixed(EffectKind::Rain) => rain(p, rect, t),
        EffectChoice::Fixed(EffectKind::Fireflies) => fireflies(p, rect, t, pal.dark),
        EffectChoice::Fixed(EffectKind::Sakura) => blossom(p, c, r * 0.9, t * 0.5),
        EffectChoice::Fixed(EffectKind::Leaves) => {
            leaf(p, c + vec2(0.0, (t * 2.0).sin() * r * 0.06), r * 1.55, 0.6 + (t * 1.5).sin() * 0.15)
        }
        EffectChoice::Fixed(EffectKind::Starfall) => starfall(p, rect, t, pal.dark),
        EffectChoice::Season => seasons(p, c, r * 0.85),
        EffectChoice::Random => dice(p, c, r * 0.8, pal),
        EffectChoice::Weather => weather(p, rect, t),
        EffectChoice::Off => off(p, c, r * 0.75, pal),
    }
}

/// Painted logo: snowflake on a rounded accent square.
pub fn logo(p: &Painter, rect: Rect, pal: &Palette) {
    p.rect_filled(rect, rect.width() * 0.28, pal.accent);
    snowflake(p, rect.center(), rect.width() * 0.34, 0.0, pal.on_accent, rect.width() * 0.07);
}

/// Painted check mark (does not depend on font glyphs).
pub fn check(p: &Painter, center: Pos2, size: f32, color: Color32) {
    let s = size * 0.5;
    p.add(Shape::line(
        vec![center + vec2(-s * 0.8, 0.05 * s), center + vec2(-s * 0.2, 0.65 * s), center + vec2(s * 0.85, -0.6 * s)],
        Stroke::new((size * 0.16).max(1.5), color),
    ));
}

fn at(c: Pos2, angle: f32, dist: f32) -> Pos2 {
    c + vec2(angle.cos(), angle.sin()) * dist
}

pub fn snowflake(p: &Painter, c: Pos2, r: f32, rot: f32, color: Color32, width: f32) {
    let st = Stroke::new(width.max(1.2), color);
    for i in 0..6 {
        let a = rot + i as f32 * TAU / 6.0;
        p.line_segment([c, at(c, a, r)], st);
        for (d, len) in [(0.55, 0.32), (0.8, 0.2)] {
            let b = at(c, a, r * d);
            p.line_segment([b, at(b, a + 0.9, r * len)], st);
            p.line_segment([b, at(b, a - 0.9, r * len)], st);
        }
    }
}

fn wrap01(x: f32) -> f32 {
    x - x.floor()
}

fn heavy_snow(p: &Painter, rect: Rect, t: f32) {
    let r = rect.width().min(rect.height()) * 0.5;
    let flakes = [
        (0.2, 0.15, 0.17, 0.30),
        (0.55, 0.0, 0.22, 0.40),
        (0.85, 0.4, 0.15, 0.25),
        (0.35, 0.55, 0.2, 0.35),
        (0.7, 0.75, 0.16, 0.28),
        (0.12, 0.8, 0.13, 0.22),
    ];
    for (x, y0, size, speed) in flakes {
        let y = wrap01(y0 + t * speed);
        let pos = pos2(rect.left() + (x + (t * 0.8 + y0 * 9.0).sin() * 0.04) * rect.width(), rect.top() + y * rect.height());
        let edge = (y.min(1.0 - y) * 6.0).clamp(0.0, 1.0);
        snowflake(p, pos, r * size * 1.3, y * 3.0, ICE.gamma_multiply(edge), r * 0.06);
    }
}

/// Teardrop with its tip at `tip`, `h` tall.
fn drop(p: &Painter, tip: Pos2, h: f32, color: Color32) {
    let w = h * 0.3;
    let center = tip + vec2(0.0, h - w);
    let beta = (w / (h - w)).clamp(-1.0, 1.0).acos();
    let n = 14;
    let mut pts = vec![tip];
    for i in 0..=n {
        let a = -PI / 2.0 + beta + (i as f32 / n as f32) * (TAU - 2.0 * beta);
        pts.push(at(center, a, w));
    }
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

fn rain(p: &Painter, rect: Rect, t: f32) {
    let h = rect.height();
    for (x, y0, speed, len) in [(0.25, 0.1, 0.9, 0.42), (0.52, 0.5, 1.1, 0.5), (0.78, 0.8, 0.8, 0.38)] {
        let y = wrap01(y0 + t * speed);
        let top = rect.top() + (y * 1.2 - 0.2) * h;
        let alpha = (y.min(1.0 - y) * 5.0).clamp(0.0, 1.0);
        drop(p, pos2(rect.left() + x * rect.width(), top), h * len, RAIN.gamma_multiply(alpha));
    }
}

fn glow_dot(p: &Painter, c: Pos2, r: f32, strength: f32, dark: bool) {
    if dark {
        for (k, a) in [(2.6, 0.06), (1.9, 0.12), (1.3, 0.25)] {
            p.circle_filled(c, r * k, GLOW.gamma_multiply(a * strength));
        }
        p.circle_filled(c, r, Color32::from_rgb(255, 245, 160).gamma_multiply(0.4 + 0.6 * strength));
    } else {
        // On white a pale yellow glow disappears: use a warm amber halo, a deeper core and a
        // thin outline so the fireflies read at a glance.
        let halo = Color32::from_rgb(240, 170, 30);
        for (k, a) in [(2.4, 0.10), (1.7, 0.20)] {
            p.circle_filled(c, r * k, halo.gamma_multiply(a * strength));
        }
        let core = Color32::from_rgb(232, 160, 18).lerp_to_gamma(Color32::from_rgb(255, 214, 90), 0.5 * strength);
        p.circle_filled(c, r, core);
        p.circle_stroke(c, r, Stroke::new(1.0, Color32::from_rgb(176, 112, 8)));
    }
}

fn fireflies(p: &Painter, rect: Rect, t: f32, dark: bool) {
    let r = rect.width().min(rect.height()) * 0.09;
    for (i, (x, y)) in [(0.3, 0.35), (0.7, 0.28), (0.55, 0.7), (0.25, 0.75), (0.85, 0.65)].into_iter().enumerate() {
        let ph = t * 2.2 + i as f32 * 1.7;
        let pos = pos2(rect.left() + x * rect.width() + (ph * 0.7).sin() * 2.0, rect.top() + y * rect.height() + (ph * 0.5).cos() * 2.0);
        glow_dot(p, pos, r * (0.8 + 0.15 * (i % 3) as f32), (0.55 + 0.45 * ph.sin()).clamp(0.1, 1.0), dark);
    }
}

fn ellipse(c: Pos2, rx: f32, ry: f32, rot: f32) -> Vec<Pos2> {
    let (s, co) = rot.sin_cos();
    (0..20)
        .map(|i| {
            let a = i as f32 / 20.0 * TAU;
            let (x, y) = (a.cos() * rx, a.sin() * ry);
            c + vec2(x * co - y * s, x * s + y * co)
        })
        .collect()
}

fn blossom(p: &Painter, c: Pos2, r: f32, rot: f32) {
    for i in 0..5 {
        let a = rot + i as f32 * TAU / 5.0 - PI / 2.0;
        let pc = at(c, a, r * 0.46);
        p.add(Shape::convex_polygon(ellipse(pc, r * 0.30, r * 0.46, a + PI / 2.0), PINK, Stroke::new(1.0, PINK_DARK)));
    }
    p.circle_filled(c, r * 0.16, Color32::from_rgb(250, 215, 90));
    for i in 0..5 {
        p.circle_filled(at(c, rot + i as f32 * TAU / 5.0, r * 0.26), r * 0.04, PINK_DARK);
    }
}

fn leaf(p: &Painter, c: Pos2, r: f32, rot: f32) {
    let (s, co) = rot.sin_cos();
    let xf = |x: f32, y: f32| c + vec2(x * co - y * s, x * s + y * co) * r;
    // Almond outline: two arcs between the tip and the base.
    let n = 10;
    let mut pts = Vec::new();
    for i in 0..=n {
        let u = i as f32 / n as f32;
        pts.push(xf(-0.55 + 1.1 * u, -(u * PI).sin() * 0.42));
    }
    for i in (1..n).rev() {
        let u = i as f32 / n as f32;
        pts.push(xf(-0.55 + 1.1 * u, (u * PI).sin() * 0.42));
    }
    p.add(Shape::convex_polygon(pts, LEAF, Stroke::new(1.0, Color32::from_rgb(170, 80, 20))));
    let vein = Stroke::new(1.3, Color32::from_rgb(150, 70, 15));
    p.line_segment([xf(-0.75, 0.0), xf(0.45, 0.0)], vein);
    for x in [-0.15, 0.1, 0.3] {
        p.line_segment([xf(x, 0.0), xf(x + 0.14, -0.2)], vein);
        p.line_segment([xf(x, 0.0), xf(x + 0.14, 0.2)], vein);
    }
}

fn star(p: &Painter, c: Pos2, r: f32, rot: f32, color: Color32) {
    let tips: Vec<Pos2> = (0..5).map(|i| at(c, rot + i as f32 * TAU / 5.0 - PI / 2.0, r)).collect();
    let inner: Vec<Pos2> = (0..5).map(|i| at(c, rot + (i as f32 + 0.5) * TAU / 5.0 - PI / 2.0, r * 0.42)).collect();
    for i in 0..5 {
        let prev = inner[(i + 4) % 5];
        p.add(Shape::convex_polygon(vec![c, prev, tips[i], inner[i]], color, Stroke::NONE));
    }
}

fn starfall(p: &Painter, rect: Rect, t: f32, dark: bool) {
    let r = rect.width().min(rect.height()) * 0.5;
    let shift = if t == 0.0 { 0.0 } else { (wrap01(t * 0.5) - 0.5) * r * 0.5 };
    let head = rect.center() + vec2(r * 0.28 + shift, -r * 0.2 + shift * 0.6);
    let dir = vec2(-0.8, 0.6).normalized();
    for i in 0..8 {
        let u = i as f32 / 8.0;
        p.circle_filled(head + dir * (r * 0.1 + u * r * 0.95), r * 0.11 * (1.0 - u * 0.8), STAR.gamma_multiply((1.0 - u) * 0.7));
    }
    // Slightly larger head; on light backgrounds a darker gold underlay keeps its edge crisp.
    if !dark {
        star(p, head, r * 0.56, 0.2, Color32::from_rgb(196, 140, 20));
    }
    star(p, head, r * 0.52, 0.2, STAR);
}

fn quarter(p: &Painter, c: Pos2, r: f32, start: f32, color: Color32) {
    let mut pts = vec![c];
    for i in 0..=8 {
        pts.push(at(c, start + i as f32 / 8.0 * PI / 2.0, r));
    }
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

fn seasons(p: &Painter, c: Pos2, r: f32) {
    // Summer (green) top-right, autumn (orange) bottom-right, winter (blue) bottom-left, spring (pink) top-left.
    quarter(p, c, r, -PI / 2.0, GREEN);
    quarter(p, c, r, 0.0, LEAF);
    quarter(p, c, r, PI / 2.0, ICE);
    quarter(p, c, r, PI, PINK);
    p.circle_filled(c, r * 0.22, Color32::WHITE.gamma_multiply(0.85));
    p.circle_stroke(c, r, Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.6)));
}

fn dice(p: &Painter, c: Pos2, r: f32, pal: &Palette) {
    let rect = Rect::from_center_size(c, vec2(r * 1.8, r * 1.8));
    p.rect_filled(rect, r * 0.35, pal.accent);
    let d = r * 0.5;
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (0.0, 0.0), (-1.0, 1.0), (1.0, 1.0)] {
        p.circle_filled(c + vec2(x * d, y * d), r * 0.17, pal.on_accent);
    }
}

fn off(p: &Painter, c: Pos2, r: f32, pal: &Palette) {
    let st = Stroke::new((r * 0.16).max(2.0), pal.text_weak);
    p.circle_stroke(c, r, st);
    let d = r * std::f32::consts::FRAC_1_SQRT_2;
    p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], st);
}
