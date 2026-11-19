//! Custom-drawn previews for the "What's new" cards. Everything is painted with egui shapes and
//! `snowy_art` primitives (no images). `t` is the animation clock in seconds.

use super::art_paint::{paint_prims, Transform};
use super::icons;
use super::theme::{semibold, Palette};
use crate::whats_new_data::Preview;
use eframe::egui::{
    pos2, vec2, Align2, Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2,
};
use snowy_core::props::{LightsLook, SnowmanLook};
use std::f32::consts::TAU;

fn c(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

const WHITE: Color32 = Color32::WHITE;

/// Paints one preview into `rect` (rounded, clipped, with its own winter backdrop).
pub fn paint(painter: &Painter, rect: Rect, preview: Preview, t: f32, pal: &Palette) {
    let p = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    backdrop(&p, rect, pal);
    if preview != Preview::Smooth {
        flakes(&p, rect, t, pal);
    }
    match preview {
        Preview::Names => names(&p, rect, t, pal),
        Preview::Squash => squash(&p, rect, t, pal),
        Preview::CleanDesk => clean_desk(&p, rect, t, pal),
        Preview::Friends => friends(&p, rect, t, pal),
        Preview::Throw => throw(&p, rect, t, pal),
        Preview::SharedDesk => shared_desk(&p, rect, t, pal),
        Preview::Drifts => drifts(&p, rect, t, pal),
        Preview::Depth => depth(&p, rect, t, pal),
        Preview::Smooth => smooth(&p, rect, t, pal),
        Preview::EditMode => edit_mode(&p, rect, t, pal),
        Preview::Lights => lights(&p, rect, t, pal),
        Preview::Clicks => clicks(&p, rect, t, pal),
        Preview::Gpu => gpu(&p, rect, t, pal),
        Preview::Fit => fit(&p, rect, t, pal),
    }
    corner_masks(&p, rect, 11.0, pal.card);
}

/// Repaints the area outside the rounded corners in the card color (hills and flakes are
/// square-clipped).
fn corner_masks(p: &Painter, rect: Rect, r: f32, fill: Color32) {
    let corners = [(rect.left_top(), 0.5, vec2(r, r)), (rect.right_top(), 0.75, vec2(-r, r)), (rect.right_bottom(), 0.0, vec2(-r, -r)), (rect.left_bottom(), 0.25, vec2(r, -r))];
    for (corner, start, to_center) in corners {
        let center = corner + to_center;
        let mut mesh = Mesh::default();
        mesh.colored_vertex(corner, fill);
        for i in 0..=8 {
            let a = (start + 0.25 * i as f32 / 8.0) * TAU;
            mesh.colored_vertex(center + vec2(a.cos(), a.sin()) * r, fill);
        }
        for i in 0..8u32 {
            mesh.add_triangle(0, 1 + i, 2 + i);
        }
        p.add(Shape::mesh(mesh));
    }
}

// ---------------------------------------------------------------- shared helpers

fn rounded_points(rect: Rect, r: f32) -> Vec<Pos2> {
    let mut pts = Vec::new();
    let corners = [(rect.right() - r, rect.bottom() - r, 0.0), (rect.left() + r, rect.bottom() - r, 0.25), (rect.left() + r, rect.top() + r, 0.5), (rect.right() - r, rect.top() + r, 0.75)];
    for (cx, cy, start) in corners {
        for i in 0..=6 {
            let a = (start + 0.25 * i as f32 / 6.0) * TAU;
            pts.push(pos2(cx + a.cos() * r, cy + a.sin() * r));
        }
    }
    pts
}

/// Rounded rectangle with a vertical colour gradient.
fn gradient_rect(p: &Painter, rect: Rect, r: f32, top: Color32, bottom: Color32) {
    let pts = rounded_points(rect, r);
    let col = |y: f32| top.lerp_to_gamma(bottom, ((y - rect.top()) / rect.height()).clamp(0.0, 1.0));
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), col(rect.center().y));
    for q in &pts {
        mesh.colored_vertex(*q, col(q.y));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
}

fn backdrop(p: &Painter, rect: Rect, pal: &Palette) {
    let (top, bot) = if pal.dark { (c(27, 42, 68, 255), c(46, 72, 108, 255)) } else { (c(112, 160, 222, 255), c(186, 214, 244, 255)) };
    gradient_rect(p, rect, 11.0, top, bot);
    // Snowy hill along the bottom.
    let hill = if pal.dark { c(206, 222, 242, 255) } else { c(250, 252, 255, 255) };
    let shade = if pal.dark { c(168, 190, 220, 255) } else { c(214, 228, 244, 255) };
    let w = rect.width();
    p.add(Shape::ellipse_filled(pos2(rect.center().x - w * 0.18, rect.bottom() + 20.0), vec2(w * 0.75, 38.0), shade));
    p.add(Shape::ellipse_filled(pos2(rect.center().x + w * 0.12, rect.bottom() + 26.0), vec2(w * 0.7, 40.0), hill));
}

/// Y of the ground at the horizontal center (where things stand).
fn ground(rect: Rect) -> f32 {
    rect.bottom() - 16.0
}

fn hash(i: u32) -> f32 {
    ((i as f32 * 12.9898).sin() * 43758.547).fract().abs()
}

/// Slowly falling snow, behind everything else.
fn flakes(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let alpha = if pal.dark { 150 } else { 215 };
    for i in 0..22u32 {
        let (hx, hy, hs) = (hash(i * 3 + 1), hash(i * 3 + 2), hash(i * 3 + 3));
        let speed = 6.0 + hs * 9.0;
        let y = (hy * rect.height() + t * speed).rem_euclid(rect.height() + 10.0) - 5.0;
        let x = rect.left() + hx * rect.width() + (t * 0.6 + i as f32).sin() * 5.0;
        let r = 1.0 + hs * 1.6;
        p.circle_filled(pos2(x, rect.top() + y), r, c(255, 255, 255, (alpha as f32 * (0.55 + 0.45 * hs)) as u8));
    }
}

fn snowman(p: &Painter, base: Pos2, height: f32, t: f32) {
    let look = SnowmanLook { hat: 2, face: 1, item: 0, outfit: 1 };
    let mut out = Vec::new();
    snowy_art::snowman(&look, 3, [base.x, base.y], height, t, &mut out);
    paint_prims(p, &out, &Transform { scale: 1.0, offset: Vec2::ZERO });
}

fn key_colors(pal: &Palette) -> (Color32, Color32, Color32) {
    // (face, bottom edge, legend)
    if pal.dark {
        (c(78, 92, 114, 255), c(30, 36, 48, 255), c(240, 245, 252, 255))
    } else {
        (c(255, 255, 255, 255), c(150, 166, 190, 255), c(30, 42, 60, 255))
    }
}

enum Cap<'a> {
    Text(&'a str),
    Win,
}

/// A keycap like the toast's: lighter face over a darker bottom edge. Returns its width.
fn keycap(p: &Painter, left_center: Pos2, cap: Cap, pal: &Palette) -> f32 {
    let (face, edge, legend) = key_colors(pal);
    let h = 30.0;
    let w = match cap {
        Cap::Text(s) => (s.chars().count() as f32 * 9.0 + 18.0).max(32.0),
        Cap::Win => 34.0,
    };
    let top = left_center.y - h / 2.0 - 1.5;
    let body = Rect::from_min_size(pos2(left_center.x, top), vec2(w, h));
    p.rect_filled(body.translate(vec2(0.0, 3.0)), 7, edge);
    p.rect_filled(body, 7, face);
    p.rect_stroke(body, 7, Stroke::new(1.0, c(0, 0, 0, 30)), StrokeKind::Inside);
    match cap {
        Cap::Text(s) => {
            p.text(body.center() + vec2(0.0, -0.5), Align2::CENTER_CENTER, s, semibold(13.5), legend);
        }
        Cap::Win => {
            // Windows logo: 2x2 squares.
            let (s, g) = (6.5, 1.8);
            let o = body.center() - vec2(s + g / 2.0, s + g / 2.0);
            for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                p.rect_filled(Rect::from_min_size(o + vec2(dx * (s + g), dy * (s + g)), vec2(s, s)), 1, legend);
            }
        }
    }
    w
}

fn file_icon(p: &Painter, tl: Pos2, tint: Color32, selected: bool) {
    let (w, h, fold) = (28.0, 34.0, 9.0);
    if selected {
        let sel = Rect::from_min_size(tl - vec2(10.0, 6.0), vec2(w + 20.0, h + 24.0));
        p.rect_filled(sel, 5, c(255, 255, 255, 46));
        p.rect_stroke(sel, 5, Stroke::new(1.0, c(255, 255, 255, 110)), StrokeKind::Inside);
    }
    let body = vec![tl, tl + vec2(w - fold, 0.0), tl + vec2(w, fold), tl + vec2(w, h), tl + vec2(0.0, h)];
    p.add(Shape::convex_polygon(vec![body[0] + vec2(1.0, 2.0), body[1] + vec2(1.0, 2.0), body[2] + vec2(1.0, 2.0), body[3] + vec2(1.0, 2.0), body[4] + vec2(1.0, 2.0)], c(0, 20, 50, 50), Stroke::NONE));
    p.add(Shape::convex_polygon(body, c(246, 249, 253, 255), Stroke::NONE));
    p.add(Shape::convex_polygon(vec![tl + vec2(w - fold, 0.0), tl + vec2(w - fold, fold), tl + vec2(w, fold)], c(196, 210, 230, 255), Stroke::NONE));
    for (i, len) in [0.62, 0.8, 0.5].into_iter().enumerate() {
        let y = tl.y + 16.0 + i as f32 * 5.5;
        p.rect_filled(Rect::from_min_size(pos2(tl.x + 5.0, y), vec2((w - 10.0) * len, 2.6)), 1, tint);
    }
    // Name under the icon.
    p.rect_filled(Rect::from_center_size(pos2(tl.x + w / 2.0, tl.y + h + 9.0), vec2(w * 0.85, 4.0)), 2, c(255, 255, 255, 190));
}

fn lock_badge(p: &Painter, center: Pos2, pal: &Palette) {
    p.circle_filled(center + vec2(0.0, 1.5), 14.0, c(0, 20, 50, 50));
    p.circle_filled(center, 13.5, pal.accent);
    p.circle_stroke(center, 13.5, Stroke::new(1.5, c(255, 255, 255, 200)));
    let col = pal.on_accent;
    // Shackle (arc) then body.
    let sc = center + vec2(0.0, -2.6);
    let pts: Vec<Pos2> = (0..=12).map(|i| {
        let a = std::f32::consts::PI + std::f32::consts::PI * i as f32 / 12.0;
        sc + vec2(a.cos() * 4.2, a.sin() * 4.6)
    }).collect();
    p.add(Shape::line(pts, Stroke::new(1.9, col)));
    p.rect_filled(Rect::from_center_size(center + vec2(0.0, 3.4), vec2(11.0, 8.5)), 2, col);
    p.circle_filled(center + vec2(0.0, 3.2), 1.3, pal.accent);
}

fn window_frame(p: &Painter, rect: Rect, pal: &Palette) {
    let (fill, bar, line) = if pal.dark {
        (c(40, 49, 66, 255), c(52, 63, 84, 255), c(88, 102, 128, 255))
    } else {
        (c(255, 255, 255, 255), c(228, 236, 247, 255), c(196, 208, 226, 255))
    };
    p.rect_filled(rect.translate(vec2(0.0, 4.0)).expand(1.0), 9, c(0, 18, 46, 60));
    p.rect_filled(rect, 8, fill);
    let bar_rect = Rect::from_min_size(rect.min, vec2(rect.width(), 20.0));
    p.rect_filled(bar_rect, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, bar);
    for (i, col) in [c(240, 120, 110, 255), c(244, 192, 90, 255), c(110, 200, 130, 255)].into_iter().enumerate() {
        p.circle_filled(pos2(rect.left() + 12.0 + i as f32 * 13.0, rect.top() + 10.0), 3.6, col);
    }
    p.rect_stroke(rect, 8, Stroke::new(1.0, c(0, 0, 0, 40)), StrokeKind::Inside);
    // Content lines.
    let mut y = rect.top() + 32.0;
    for len in [0.7, 0.5, 0.62, 0.38] {
        if y + 6.0 > rect.bottom() - 6.0 {
            break;
        }
        p.rect_filled(Rect::from_min_size(pos2(rect.left() + 12.0, y), vec2((rect.width() - 24.0) * len, 5.0)), 2, line);
        y += 12.0;
    }
}

fn cursor(p: &Painter, tip: Pos2) {
    let pts = [(0.0, 0.0), (0.0, 17.0), (4.2, 13.2), (7.2, 20.0), (10.2, 18.7), (7.3, 12.2), (12.8, 12.0)];
    let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| tip + vec2(x, y)).collect();
    p.add(Shape::convex_polygon(pts.iter().map(|q| *q + vec2(1.0, 1.5)).collect(), c(0, 0, 0, 60), Stroke::NONE));
    p.add(Shape::closed_line(pts.clone(), Stroke::new(2.2, c(18, 24, 36, 255))));
    p.add(Shape::convex_polygon(pts, WHITE, Stroke::NONE));
}

fn ease(x: f32) -> f32 {
    0.5 - 0.5 * (x * TAU).cos()
}

// ---------------------------------------------------------------- the five previews

fn edit_mode(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let g = ground(rect);
    // Desktop icons, in a little column-grid on the left.
    let ox = cx - 215.0;
    let tints = [pal.accent, c(240, 160, 90, 255), c(110, 190, 140, 255)];
    file_icon(p, pos2(ox, rect.top() + 18.0), tints[0], true);
    file_icon(p, pos2(ox + 58.0, rect.top() + 18.0), tints[1], false);
    file_icon(p, pos2(ox, rect.top() + 84.0 - 6.0), tints[2], false);
    // The snowman next to them, with a lock badge.
    snowman(p, pos2(cx - 78.0, g), 104.0, t);
    lock_badge(p, pos2(cx - 36.0, g - 78.0), pal);
    // Keycaps: Win + Alt + S.
    let y = rect.top() + 66.0;
    let plus = |p: &Painter, x: f32| {
        p.text(pos2(x, y), Align2::CENTER_CENTER, "+", semibold(15.0), c(255, 255, 255, 220));
    };
    let mut x = cx + 38.0;
    // A gentle press: the whole row dips a pixel in turn.
    let k = |i: f32| ((t * 1.4 - i * 0.18).rem_euclid(3.0) < 0.25) as i32 as f32;
    x += keycap(p, pos2(x, y + k(0.0)), Cap::Win, pal) + 11.0;
    plus(p, x - 5.5);
    x += keycap(p, pos2(x, y + k(1.0)), Cap::Text("Alt"), pal) + 11.0;
    plus(p, x - 5.5);
    keycap(p, pos2(x, y + k(2.0)), Cap::Text("S"), pal);
}

fn lights(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let joint = pos2(cx, rect.top() + 82.0);
    let left = pos2(rect.left() + 24.0, rect.top() + 34.0);
    let right = pos2(rect.right() - 24.0, rect.top() + 40.0);
    let look = LightsLook { colors: 1, bulbs: 0, pattern: 1, wire: 2 };
    let mut out = Vec::new();
    let sway = 0.02 * (t * 0.8).sin();
    snowy_art::lights_scaled(&look, [left.x, left.y], [joint.x, joint.y], 22.0, t, sway, 1.3, &mut out);
    snowy_art::lights_scaled(&look, [joint.x, joint.y], [right.x, right.y], 20.0, t + 1.7, sway, 1.3, &mut out);
    paint_prims(p, &out, &Transform { scale: 1.0, offset: Vec2::ZERO });
    // Pins at the loose ends.
    for q in [left, right] {
        p.circle_filled(q, 4.0, c(20, 30, 46, 255));
        p.circle_stroke(q, 4.0, Stroke::new(1.0, c(255, 255, 255, 120)));
    }
    // The joint, with a pulsing ring.
    let ph = (t * 0.9).rem_euclid(1.0);
    p.circle_stroke(joint, 8.0 + 14.0 * ph, Stroke::new(2.0, WHITE.gamma_multiply((1.0 - ph) * 0.85)));
    p.circle_filled(joint, 7.0, c(0, 20, 50, 60));
    p.circle_filled(joint, 6.2, WHITE);
    p.circle_filled(joint, 3.4, pal.accent);
}

fn clicks(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let g = ground(rect);
    snowman(p, pos2(cx + 70.0, g + 2.0), 112.0, t);
    let win = Rect::from_min_size(pos2(cx - 150.0, rect.top() + 22.0), vec2(210.0, 104.0));
    window_frame(p, win, pal);
    // Cursor on the window, clicking now and then.
    let tip = pos2(win.left() + 128.0, win.top() + 58.0);
    let ph = (t * 0.8).rem_euclid(1.0);
    let ring = Stroke::new(2.0, pal.accent.gamma_multiply((1.0 - ph) * 0.9));
    p.circle_stroke(tip, 4.0 + 18.0 * ph, ring);
    cursor(p, tip);
}

fn chip(p: &Painter, center: Pos2, size: f32, alpha: f32, pal: &Palette) {
    let a = |col: Color32| col.gamma_multiply(alpha);
    let body = Rect::from_center_size(center, vec2(size, size));
    for i in 0..4 {
        let f = (i as f32 + 1.0) / 5.0;
        let (x, y) = (body.left() + size * f, body.top() + size * f);
        for (a0, a1) in [(pos2(x, body.top() - 7.0), pos2(x, body.top() + 2.0)), (pos2(x, body.bottom() - 2.0), pos2(x, body.bottom() + 7.0)), (pos2(body.left() - 7.0, y), pos2(body.left() + 2.0, y)), (pos2(body.right() - 2.0, y), pos2(body.right() + 7.0, y))] {
            p.line_segment([a0, a1], Stroke::new(2.6, a(c(226, 236, 250, 255))));
        }
    }
    p.rect_filled(body, 9, a(c(236, 243, 252, 255)));
    p.rect_stroke(body, 9, Stroke::new(1.0, c(0, 0, 0, (40.0 * alpha) as u8)), StrokeKind::Inside);
    let die = body.shrink(size * 0.24);
    p.rect_filled(die, 5, a(pal.accent.lerp_to_gamma(c(70, 120, 190, 255), if pal.dark { 0.25 } else { 0.0 })));
}

fn gpu(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let cy = rect.center().y - 8.0;
    // Two chips: the power-hungry one dimmed, the power-saving one picked.
    chip(p, pos2(cx - 150.0, cy), 56.0, 0.38, pal);
    let good = pos2(cx - 62.0, cy);
    chip(p, good, 56.0, 1.0, pal);
    icons::snowflake(p, good, 13.0, t * 0.25, WHITE, 1.8);
    let badge = good + vec2(26.0, -26.0);
    p.circle_filled(badge, 10.5, c(70, 190, 130, 255));
    p.circle_stroke(badge, 10.5, Stroke::new(1.5, WHITE));
    icons::check(p, badge, 11.0, WHITE);
    // Arrow chip -> battery.
    let ay = cy;
    p.line_segment([pos2(cx - 20.0, ay), pos2(cx + 6.0, ay)], Stroke::new(2.5, c(255, 255, 255, 200)));
    p.add(Shape::convex_polygon(vec![pos2(cx + 12.0, ay), pos2(cx + 4.0, ay - 6.0), pos2(cx + 4.0, ay + 6.0)], c(255, 255, 255, 220), Stroke::NONE));
    // Battery, slowly "breathing" at a healthy level.
    let b = Rect::from_center_size(pos2(cx + 108.0, cy), vec2(112.0, 58.0));
    p.rect_filled(Rect::from_center_size(pos2(b.right() + 5.0, cy), vec2(8.0, 20.0)), 3, c(255, 255, 255, 230));
    p.rect_filled(b, 12, c(10, 26, 54, if pal.dark { 120 } else { 70 }));
    p.rect_stroke(b, 12, Stroke::new(3.0, WHITE), StrokeKind::Inside);
    let level = 0.66 + 0.06 * ease(t * 0.2);
    let inner = b.shrink(7.0);
    let fill = Rect::from_min_size(inner.min, vec2(inner.width() * level, inner.height()));
    p.rect_filled(fill, 6, c(96, 208, 140, 255));
    p.rect_filled(Rect::from_min_size(fill.min, vec2(fill.width(), fill.height() * 0.42)), CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, c(255, 255, 255, 50));
}

fn dashed_rect(p: &Painter, r: Rect, stroke: Stroke) {
    let (dash, gap) = (5.0, 4.0);
    let edge = |a: Pos2, b: Pos2| {
        let len = (b - a).length();
        let dir = (b - a) / len;
        let mut d = 0.0;
        while d < len {
            p.line_segment([a + dir * d, a + dir * (d + dash).min(len)], stroke);
            d += dash + gap;
        }
    };
    edge(r.left_top(), r.right_top());
    edge(r.right_top(), r.right_bottom());
    edge(r.right_bottom(), r.left_bottom());
    edge(r.left_bottom(), r.left_top());
}

fn fit(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let mc = pos2(rect.center().x - 46.0, rect.top() + 72.0);
    let bezel = Rect::from_center_size(mc, vec2(236.0, 118.0));
    // Stand.
    let (stand, base) = if pal.dark { (c(26, 34, 50, 255), c(22, 29, 43, 255)) } else { (c(58, 76, 104, 255), c(50, 66, 92, 255)) };
    p.rect_filled(Rect::from_center_size(pos2(mc.x, bezel.bottom() + 4.0), vec2(36.0, 10.0)), 2, stand);
    p.rect_filled(Rect::from_center_size(pos2(mc.x, bezel.bottom() + 9.5), vec2(78.0, 5.0)), 2.5, base);
    // Bezel + screen.
    let (frame, screen_top, screen_bot) = if pal.dark {
        (c(26, 34, 50, 255), c(58, 92, 138, 255), c(84, 124, 176, 255))
    } else {
        (c(58, 76, 104, 255), c(150, 190, 236, 255), c(196, 220, 246, 255))
    };
    p.rect_filled(bezel, 9, frame);
    let screen = bezel.shrink(5.0);
    gradient_rect(p, screen, 5.0, screen_top, screen_bot);
    // Taskbar and the work area above it.
    let bar = Rect::from_min_max(pos2(screen.left(), screen.bottom() - 11.0), screen.right_bottom());
    p.rect_filled(bar, CornerRadius { nw: 0, ne: 0, sw: 5, se: 5 }, c(12, 20, 36, 200));
    for i in 0..4 {
        p.rect_filled(Rect::from_center_size(pos2(bar.center().x + (i as f32 - 1.5) * 12.0, bar.center().y), vec2(7.0, 5.0)), 2, c(255, 255, 255, if i == 1 { 230 } else { 120 }));
    }
    let work = Rect::from_min_max(screen.left_top() + vec2(5.0, 5.0), pos2(screen.right() - 5.0, bar.top() - 4.0));
    dashed_rect(p, work, Stroke::new(1.3, c(255, 255, 255, 150)));
    // The window fits inside the work area, "settling" in with a tiny breathing motion.
    let k = 0.94 + 0.06 * ease(t * 0.35);
    let win = Rect::from_center_size(work.center(), work.size() * k - vec2(8.0, 6.0));
    window_frame(p, win, pal);
    // Scale pills: the active one cycles through 125 / 150 / 175 %.
    let active = ((t / 1.6) as usize) % 3;
    for (i, label) in ["125%", "150%", "175%"].into_iter().enumerate() {
        let r = Rect::from_center_size(pos2(rect.center().x + 126.0, rect.top() + 40.0 + i as f32 * 36.0), vec2(66.0, 26.0));
        if i == active {
            p.rect_filled(r, 13, pal.accent);
            p.text(r.center(), Align2::CENTER_CENTER, label, semibold(13.0), pal.on_accent);
        } else {
            p.rect_filled(r, 13, c(8, 24, 52, if pal.dark { 90 } else { 70 }));
            p.text(r.center(), Align2::CENTER_CENTER, label, semibold(13.0), c(255, 255, 255, 215));
        }
    }
}

// ---------------------------------------------------------------- 0.1.5

/// A taskbar along the bottom of the preview; returns its top edge.
fn taskbar(p: &Painter, rect: Rect, pal: &Palette) -> f32 {
    let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 22.0), rect.right_bottom());
    p.rect_filled(bar, 0, if pal.dark { c(14, 20, 32, 245) } else { c(28, 40, 62, 235) });
    for i in 0..5 {
        let on = i == 2;
        let r = Rect::from_center_size(pos2(bar.center().x + (i as f32 - 2.0) * 22.0, bar.center().y), vec2(13.0, 11.0));
        p.rect_filled(r, 3, c(255, 255, 255, if on { 230 } else { 110 }));
    }
    bar.top()
}

/// Drift shape 0..1 at x; the pattern slides right as `t` grows (the wind pushing it).
fn drift_shape(x: f32, t: f32) -> f32 {
    let a = (x * 0.026 - t * 0.55).sin();
    let b = (x * 0.071 - t * 0.9 + 1.3).sin();
    let c = (x * 0.17 - t * 1.4 + 0.4).sin();
    (0.52 + 0.3 * a + 0.13 * b + 0.05 * c).clamp(0.0, 1.0)
}

/// Snow resting on the line `base_y`, `height(x)` tall, white on top and faintly blue below.
fn snow_bank(p: &Painter, left: f32, right: f32, base_y: f32, pal: &Palette, height: impl Fn(f32) -> f32) {
    let top_col = WHITE;
    let bot_col = if pal.dark { c(200, 216, 238, 255) } else { c(222, 234, 248, 255) };
    let mut mesh = Mesh::default();
    let step = 3.0;
    let mut x = left;
    let mut i = 0u32;
    while x <= right + step {
        let h = height(x).max(0.0);
        mesh.colored_vertex(pos2(x, base_y - h), top_col);
        mesh.colored_vertex(pos2(x, base_y + 1.0), bot_col);
        if i > 0 {
            let k = i * 2;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k + 1, k);
        }
        x += step;
        i += 1;
    }
    p.add(Shape::mesh(mesh));
    // A soft shadow line under the crest for depth.
    let mut pts = Vec::new();
    let mut x = left;
    while x <= right {
        pts.push(pos2(x, base_y - height(x).max(0.0) + 1.5));
        x += 4.0;
    }
    p.add(Shape::line(pts, Stroke::new(1.0, c(150, 180, 220, 70))));
}

/// Short wind streaks drifting to the right.
fn wind_streaks(p: &Painter, rect: Rect, t: f32, alpha: f32) {
    for i in 0..6u32 {
        let (hx, hy) = (hash(i * 7 + 11), hash(i * 7 + 12));
        let len = 26.0 + hash(i * 7 + 13) * 30.0;
        let x = rect.left() + ((hx * rect.width() + t * 60.0) % (rect.width() + len)) - len;
        let y = rect.top() + 20.0 + hy * (rect.height() - 70.0);
        let a = (alpha * (0.35 + 0.5 * hash(i * 7 + 14)) * 255.0) as u8;
        p.line_segment([pos2(x, y), pos2(x + len, y)], Stroke::new(1.6, c(255, 255, 255, a)));
    }
}

fn drifts(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let base = taskbar(p, rect, pal);
    wind_streaks(p, rect, t, 0.8);
    snow_bank(p, rect.left(), rect.right(), base, pal, |x| 6.0 + 38.0 * drift_shape(x - rect.left(), t));
}

fn depth(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let base = taskbar(p, rect, pal);
    // The setting slowly goes up and down; the snow follows.
    let v = 0.3 + 0.7 * ease(t * 0.11);
    let max_h = 64.0 * v;
    snow_bank(p, rect.left(), rect.right(), base, pal, |x| max_h * (0.35 + 0.65 * drift_shape(x - rect.left(), 0.0)));
    // A ruler at the highest hill.
    let peak_x = (0..120).map(|i| rect.left() + 20.0 + i as f32 * 4.0).fold(rect.left(), |best, x| {
        if drift_shape(x - rect.left(), 0.0) > drift_shape(best - rect.left(), 0.0) {
            x
        } else {
            best
        }
    });
    let top = base - max_h;
    let rx = peak_x + 16.0;
    let ruler = Stroke::new(1.6, pal.accent.lerp_to_gamma(WHITE, 0.15));
    p.line_segment([pos2(rx, top), pos2(rx, base - 2.0)], ruler);
    for y in [top, base - 2.0] {
        p.line_segment([pos2(rx - 5.0, y), pos2(rx + 5.0, y)], ruler);
    }
    // The slider, in a frosted panel.
    let panel = Rect::from_center_size(pos2(rect.center().x, rect.top() + 34.0), vec2(250.0, 36.0));
    p.rect_filled(panel, 18, c(8, 24, 52, if pal.dark { 120 } else { 80 }));
    let (x0, x1, y) = (panel.left() + 40.0, panel.right() - 22.0, panel.center().y);
    icons::snowflake(p, pos2(panel.left() + 21.0, y), 8.0, t * 0.2, WHITE, 1.6);
    p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(4.0, c(255, 255, 255, 90)));
    let knob = pos2(x0 + (x1 - x0) * v, y);
    p.line_segment([pos2(x0, y), knob], Stroke::new(4.0, pal.accent));
    p.circle_filled(knob, 9.0, WHITE);
    p.circle_stroke(knob, 9.0, Stroke::new(1.0, c(0, 0, 0, 40)));
}

fn smooth(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let g = ground(rect);
    // Weather off, decorations still moving smoothly.
    let tree = snowy_core::props::TreeLook { lights: 2, ornaments: 3, topper: 1, garland: 1 };
    let mut out = Vec::new();
    snowy_art::tree(&tree, [cx - 110.0, g + 2.0], 118.0, 1.0, t, &mut out);
    paint_prims(p, &out, &Transform { scale: 1.0, offset: Vec2::ZERO });
    snowman(p, pos2(cx - 20.0, g), 100.0, t);
    // "Weather off" badge: a crossed-out snowflake.
    let b = pos2(cx + 92.0, rect.top() + 44.0);
    p.circle_filled(b, 19.0, c(8, 24, 52, if pal.dark { 120 } else { 80 }));
    icons::snowflake(p, b, 10.0, 0.0, c(255, 255, 255, 200), 1.6);
    p.line_segment([b + vec2(-12.0, 12.0), b + vec2(12.0, -12.0)], Stroke::new(2.4, c(255, 120, 120, 240)));
    // Frame-rate pill with a steady, smooth meter.
    let pill = Rect::from_center_size(pos2(cx + 150.0, rect.top() + 44.0), vec2(84.0, 30.0));
    p.rect_filled(pill, 15, c(70, 190, 130, 255));
    p.text(pill.center(), Align2::CENTER_CENTER, "60 FPS", semibold(13.5), WHITE);
    let graph = Rect::from_center_size(pos2(cx + 121.0, rect.top() + 98.0), vec2(150.0, 40.0));
    p.rect_filled(graph, 8, c(8, 24, 52, if pal.dark { 110 } else { 70 }));
    let mut pts = Vec::new();
    for i in 0..=30 {
        let f = i as f32 / 30.0;
        let wobble = ((t * 3.0 + f * 9.0).sin() * 1.2).round() * 0.5;
        pts.push(pos2(graph.left() + 8.0 + f * (graph.width() - 16.0), graph.center().y + wobble));
    }
    p.add(Shape::line(pts, Stroke::new(2.0, c(120, 230, 170, 255))));
}

// ---------------------------------------------------------------- 0.1.6

/// A small monitor with a desktop and a taskbar; returns the screen area above the taskbar.
fn mini_screen(p: &Painter, center: Pos2, size: Vec2, pal: &Palette) -> Rect {
    let (frame, top, bot) = if pal.dark {
        (c(26, 34, 50, 255), c(58, 92, 138, 255), c(84, 124, 176, 255))
    } else {
        (c(58, 76, 104, 255), c(150, 190, 236, 255), c(196, 220, 246, 255))
    };
    let bezel = Rect::from_center_size(center, size);
    p.rect_filled(Rect::from_center_size(pos2(center.x, bezel.bottom() + 4.0), vec2(26.0, 8.0)), 2, frame);
    p.rect_filled(Rect::from_center_size(pos2(center.x, bezel.bottom() + 8.5), vec2(56.0, 4.0)), 2, frame);
    p.rect_filled(bezel, 8, frame);
    let screen = bezel.shrink(4.0);
    gradient_rect(p, screen, 4.0, top, bot);
    let bar = Rect::from_min_max(pos2(screen.left(), screen.bottom() - 8.0), screen.right_bottom());
    p.rect_filled(bar, CornerRadius { nw: 0, ne: 0, sw: 4, se: 4 }, c(12, 20, 36, 200));
    Rect::from_min_max(screen.min, pos2(screen.right(), bar.top()))
}

/// A person: round head and shoulders.
fn person(p: &Painter, center: Pos2, r: f32, color: Color32) {
    p.circle_filled(center + vec2(0.0, -r * 0.55), r * 0.42, color);
    let body = Rect::from_center_size(center + vec2(0.0, r * 0.45), vec2(r * 1.5, r * 0.85));
    p.rect_filled(body, CornerRadius { nw: (r * 0.7) as u8, ne: (r * 0.7) as u8, sw: 2, se: 2 }, color);
}

fn friends(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let cy = rect.top() + 64.0;
    let green = c(70, 190, 120, 255);
    // Two people far apart, each with an online dot.
    for (dx, phase) in [(-150.0, 0.0), (150.0, 1.3)] {
        let at = pos2(cx + dx, cy + (t * 1.6 + phase).sin() * 2.0);
        p.circle_filled(at, 30.0, c(8, 24, 52, if pal.dark { 110 } else { 70 }));
        person(p, at, 20.0, WHITE);
        p.circle_filled(at + vec2(21.0, 20.0), 7.0, if pal.dark { c(26, 34, 50, 255) } else { WHITE });
        p.circle_filled(at + vec2(21.0, 20.0), 5.0, green);
    }
    // A dotted link with sparks travelling both ways.
    let (a, b) = (pos2(cx - 112.0, cy), pos2(cx + 112.0, cy));
    let mut x = a.x;
    while x < b.x {
        p.circle_filled(pos2(x, cy), 1.6, c(255, 255, 255, 150));
        x += 9.0;
    }
    for k in 0..2 {
        let f = (t * 0.45 + k as f32 * 0.5).rem_euclid(1.0);
        let (from, to) = if k == 0 { (a, b) } else { (b, a) };
        icons::snowflake(p, from + (to - from) * f, 6.0, t, WHITE.gamma_multiply((f * std::f32::consts::PI).sin()), 1.5);
    }
    // The friend code in the middle.
    let pill = Rect::from_center_size(pos2(cx, cy + 42.0), vec2(112.0, 28.0));
    p.rect_filled(pill, 14, pal.accent);
    p.text(pill.center(), Align2::CENTER_CENTER, "KAR-7F3Q", eframe::egui::FontId::monospace(14.0), pal.on_accent);
}

fn throw(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let left = mini_screen(p, pos2(cx - 128.0, rect.top() + 62.0), vec2(170.0, 104.0), pal);
    let right = mini_screen(p, pos2(cx + 128.0, rect.top() + 62.0), vec2(170.0, 104.0), pal);
    // A snowball leaves the left screen and lands on the right one, then a gift floats down.
    let cycle = 3.2;
    let k = (t / cycle).floor() as i32;
    let f = (t / cycle).fract();
    if k % 2 == 0 {
        let from = pos2(left.center().x + 20.0, left.center().y);
        let to = pos2(right.center().x - 10.0, right.bottom() - 9.0);
        let g = (f * 1.3).min(1.0);
        let pos = from + (to - from) * g + vec2(0.0, -60.0 * 4.0 * g * (1.0 - g));
        let r = 7.0;
        p.circle_filled(pos + vec2(1.0, 1.5), r, c(0, 20, 50, 50));
        p.circle_filled(pos, r, WHITE);
        p.circle_filled(pos + vec2(-2.0, -2.0), r * 0.35, c(220, 232, 248, 255));
        cursor(p, pos2(from.x - 30.0, from.y + 6.0));
    } else {
        let g = (f * 1.4).min(1.0);
        let base = pos2(right.center().x - 18.0, right.top() + 6.0 + (right.height() - 22.0) * g);
        if g < 1.0 {
            p.circle_filled(base + vec2(0.0, -16.0), 9.0, c(240, 110, 100, 255));
            p.line_segment([base + vec2(-8.0, -14.0), base + vec2(-4.0, -4.0)], Stroke::new(1.0, WHITE));
            p.line_segment([base + vec2(8.0, -14.0), base + vec2(4.0, -4.0)], Stroke::new(1.0, WHITE));
        }
        let box_ = Rect::from_center_size(base + vec2(0.0, 2.0), vec2(14.0, 12.0));
        p.rect_filled(box_, 2, c(226, 70, 70, 255));
        p.rect_filled(Rect::from_center_size(box_.center(), vec2(3.0, 12.0)), 0, c(250, 210, 90, 255));
    }
    // The notice on the receiving side.
    let bubble = Rect::from_min_size(pos2(right.right() - 66.0, right.top() + 6.0), vec2(60.0, 20.0));
    p.rect_filled(bubble, 6, if pal.dark { c(30, 38, 54, 240) } else { c(255, 255, 255, 240) });
    person(p, bubble.left_center() + vec2(10.0, 1.0), 6.0, pal.accent);
    p.rect_filled(Rect::from_min_size(bubble.left_top() + vec2(20.0, 6.0), vec2(32.0, 3.0)), 1, pal.text_weak);
    p.rect_filled(Rect::from_min_size(bubble.left_top() + vec2(20.0, 12.0), vec2(22.0, 3.0)), 1, pal.text_weak);
}

fn shared_desk(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    // The same decorations on both screens; the snowman walks in step on both.
    let x = 0.3 + 0.4 * ease(t * 0.12);
    for dx in [-128.0, 128.0] {
        let s = mini_screen(p, pos2(cx + dx, rect.top() + 62.0), vec2(170.0, 104.0), pal);
        let g = s.bottom();
        let tree = snowy_core::props::TreeLook { lights: 2, ornaments: 1, topper: 1, garland: 1 };
        let mut out = Vec::new();
        snowy_art::tree(&tree, [s.left() + s.width() * 0.78, g], 58.0, 1.0, t, &mut out);
        paint_prims(p, &out, &Transform { scale: 1.0, offset: Vec2::ZERO });
        snowman(p, pos2(s.left() + s.width() * x * 0.8, g), 44.0, t);
    }
    // Two-way arrow between them.
    let y = rect.top() + 62.0;
    let col = c(255, 255, 255, 220);
    p.line_segment([pos2(cx - 30.0, y), pos2(cx + 30.0, y)], Stroke::new(2.5, col));
    p.add(Shape::convex_polygon(vec![pos2(cx + 36.0, y), pos2(cx + 27.0, y - 6.0), pos2(cx + 27.0, y + 6.0)], col, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![pos2(cx - 36.0, y), pos2(cx - 27.0, y - 6.0), pos2(cx - 27.0, y + 6.0)], col, Stroke::NONE));
}

// ---------------------------------------------------------------- 0.1.7

fn names(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let g = ground(rect);
    snowman(p, pos2(cx - 90.0, g), 112.0, t);
    // A name field, as in the snowman's editor, being typed into.
    let field = Rect::from_min_size(pos2(cx - 10.0, rect.top() + 44.0), vec2(190.0, 34.0));
    p.rect_filled(field, 8, if pal.dark { c(30, 38, 54, 240) } else { c(255, 255, 255, 245) });
    p.rect_stroke(field, 8, Stroke::new(1.5, pal.accent), StrokeKind::Inside);
    let full = "Kartopu";
    let n = ((t * 3.0) as usize % (full.len() + 6)).min(full.len());
    let shown = &full[..n];
    let text_col = if pal.dark { c(240, 245, 252, 255) } else { c(30, 42, 60, 255) };
    let galley = p.layout_no_wrap(shown.to_string(), semibold(16.0), text_col);
    let tx = field.left() + 12.0;
    p.galley(pos2(tx, field.center().y - galley.size().y / 2.0), galley.clone(), text_col);
    if (t * 2.0).fract() < 0.6 {
        let x = tx + galley.size().x + 2.0;
        p.line_segment([pos2(x, field.top() + 8.0), pos2(x, field.bottom() - 8.0)], Stroke::new(1.5, pal.accent));
    }
    // A dotted line from the field to the snowman.
    let (a, b) = (pos2(field.left() - 6.0, field.center().y), pos2(cx - 62.0, g - 70.0));
    for k in 0..8 {
        let f = k as f32 / 7.0;
        p.circle_filled(a + (b - a) * f, 1.6, c(255, 255, 255, 170));
    }
}

fn squash(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let base = taskbar(p, rect, pal);
    // Two balls: the big one sits deeper in the snow and pushes up a bigger rim.
    let balls = [(rect.center().x - 90.0, 22.0), (rect.center().x + 80.0, 40.0)];
    let depth = 26.0;
    let sink = |r: f32| r * (0.08 + 0.20 * ((r - 6.0) / 34.0).clamp(0.0, 1.0));
    let settle = ease(t * 0.12).max(0.0);
    let height = |x: f32| {
        let mut h = depth;
        for (bx, r) in balls {
            let s = sink(r) * settle;
            let d = (x - bx).abs();
            if d < r {
                // Squashed under the ball, down to its outline.
                let outline = depth - s + (r - (r * r - d * d).sqrt());
                h = h.min(outline);
            } else if d < r * 1.6 {
                h += s * 1.2 * (1.0 - (d - r) / (r * 0.6));
            }
        }
        h
    };
    snow_bank(p, rect.left(), rect.right(), base, pal, height);
    for (bx, r) in balls {
        let cy = base - depth + sink(r) * settle - r;
        let at = pos2(bx, cy);
        p.circle_filled(at + vec2(2.0, 3.0), r, c(0, 20, 50, 40));
        p.circle_filled(at, r, WHITE);
        p.circle_filled(at + vec2(-r * 0.3, -r * 0.3), r * 0.35, c(220, 232, 248, 255));
        p.circle_stroke(at, r, Stroke::new(1.0, c(150, 175, 210, 120)));
    }
}

fn clean_desk(p: &Painter, rect: Rect, t: f32, pal: &Palette) {
    let cx = rect.center().x;
    let g = ground(rect);
    // Desktop icons fade out while edit mode is on, and come back after.
    let on = (t / 3.0).floor() as i32 % 2 == 0;
    let k = ((t / 3.0).fract() * 4.0).min(1.0);
    let alpha = if on { 1.0 - k } else { k };
    let tints = [pal.accent, c(240, 160, 90, 255), c(110, 190, 140, 255)];
    for (i, tint) in tints.into_iter().enumerate() {
        let tl = pos2(rect.left() + 26.0, rect.top() + 14.0 + i as f32 * 42.0);
        let a = alpha.clamp(0.0, 1.0);
        if a > 0.02 {
            let body = Rect::from_min_size(tl, vec2(26.0, 32.0));
            p.rect_filled(body, 4, tint.gamma_multiply(a));
            p.rect_filled(Rect::from_min_size(tl + vec2(5.0, 8.0), vec2(16.0, 3.0)), 1, WHITE.gamma_multiply(a));
            p.rect_filled(Rect::from_min_size(tl + vec2(5.0, 15.0), vec2(12.0, 3.0)), 1, WHITE.gamma_multiply(a));
        }
    }
    snowman(p, pos2(cx - 30.0 + 40.0 * ease(t * 0.16), g), 100.0, t);
    // The edit mode badge.
    let pill = Rect::from_center_size(pos2(cx + 140.0, rect.top() + 40.0), vec2(120.0, 30.0));
    let (fill, text) = if on { (pal.accent, pal.on_accent) } else { (c(8, 24, 52, if pal.dark { 110 } else { 70 }), WHITE) };
    p.rect_filled(pill, 15, fill);
    p.text(pill.center(), Align2::CENTER_CENTER, "Win + Alt + S", semibold(12.5), text);
}
