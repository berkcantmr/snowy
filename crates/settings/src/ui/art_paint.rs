//! Draws `snowy_art` primitives with egui shapes, approximating the GPU look.

use eframe::egui::{pos2, vec2, Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use snowy_art::{Prim, PrimKind};

/// Maps art pixel space to screen space: screen = art * scale + offset.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub scale: f32,
    pub offset: Vec2,
}

impl Transform {
    /// Fits the art rect `[x0, y0, x1, y1]` into `target`, centered, keeping the aspect ratio.
    pub fn fit(art: [f32; 4], target: Rect) -> Self {
        let (w, h) = ((art[2] - art[0]).max(1.0), (art[3] - art[1]).max(1.0));
        let scale = (target.width() / w).min(target.height() / h);
        let center_art = vec2((art[0] + art[2]) / 2.0, (art[1] + art[3]) / 2.0);
        Self { scale, offset: target.center().to_vec2() - center_art * scale }
    }

    pub fn point(&self, p: [f32; 2]) -> Pos2 {
        pos2(p[0] * self.scale + self.offset.x, p[1] * self.scale + self.offset.y)
    }
}

fn color(c: [f32; 4], alpha_mul: f32) -> Color32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(b(c[0]), b(c[1]), b(c[2]), b(c[3] * alpha_mul))
}

fn rotate(v: [f32; 2], rot: f32) -> [f32; 2] {
    let (s, c) = rot.sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
}

/// Local (unrotated, centered) point -> screen.
fn place(p: &Prim, tf: &Transform, local: [f32; 2]) -> Pos2 {
    let r = rotate(local, p.rot);
    tf.point([p.pos[0] + r[0], p.pos[1] + r[1]])
}

fn ellipse_points(p: &Prim, tf: &Transform, center: [f32; 2], rx: f32, ry: f32) -> Vec<Pos2> {
    const N: usize = 32;
    (0..N)
        .map(|i| {
            let a = i as f32 / N as f32 * std::f32::consts::TAU;
            place(p, tf, [center[0] + a.cos() * rx, center[1] + a.sin() * ry])
        })
        .collect()
}

fn filled_ellipse(painter: &Painter, p: &Prim, tf: &Transform, center: [f32; 2], rx: f32, ry: f32, fill: Color32) {
    if p.rot.abs() < 1e-3 {
        let c = tf.point([p.pos[0] + center[0], p.pos[1] + center[1]]);
        painter.add(Shape::ellipse_filled(c, vec2(rx, ry) * tf.scale, fill));
    } else {
        painter.add(Shape::convex_polygon(ellipse_points(p, tf, center, rx, ry), fill, Stroke::NONE));
    }
}

/// Paints all primitives back-to-front.
pub fn paint_prims(painter: &Painter, prims: &[Prim], tf: &Transform) {
    for p in prims {
        paint_prim(painter, p, tf);
    }
}

fn paint_prim(painter: &Painter, p: &Prim, tf: &Transform) {
    let [sx, sy] = p.size;
    match p.kind {
        PrimKind::Ellipse => filled_ellipse(painter, p, tf, [0.0, 0.0], sx, sy, color(p.color, 1.0)),
        PrimKind::Ball => {
            filled_ellipse(painter, p, tf, [0.0, 0.0], sx, sy, color(p.color, 1.0));
            // Soft shadow bottom-right (stays inside the ball), highlight top-left.
            let a = p.color[3];
            filled_ellipse(painter, p, tf, [sx * 0.12, sy * 0.12], sx * 0.84, sy * 0.84, Color32::from_rgba_unmultiplied(40, 60, 100, (a * 34.0) as u8));
            filled_ellipse(painter, p, tf, [-sx * 0.12, -sy * 0.12], sx * 0.78, sy * 0.78, color(p.color, 1.0));
            filled_ellipse(painter, p, tf, [-sx * 0.36, -sy * 0.38], sx * 0.3, sy * 0.24, Color32::from_rgba_unmultiplied(255, 255, 255, (a * 120.0) as u8));
        }
        PrimKind::RoundRect => {
            let r = (p.round.clamp(0.0, 1.0) * sx.min(sy)).max(0.0);
            let corners = [([sx - r, sy - r], 0.0f32), ([-sx + r, sy - r], 0.25), ([-sx + r, -sy + r], 0.5), ([sx - r, -sy + r], 0.75)];
            let mut pts = Vec::new();
            for (c, start) in corners {
                for i in 0..=5 {
                    let a = (start + 0.25 * i as f32 / 5.0) * std::f32::consts::TAU;
                    pts.push(place(p, tf, [c[0] + a.cos() * r, c[1] + a.sin() * r]));
                }
            }
            painter.add(Shape::convex_polygon(pts, color(p.color, 1.0), Stroke::NONE));
        }
        PrimKind::Triangle => {
            let pts = vec![place(p, tf, [0.0, -sy]), place(p, tf, [sx, sy]), place(p, tf, [-sx, sy])];
            painter.add(Shape::convex_polygon(pts, color(p.color, 1.0), Stroke::NONE));
        }
        PrimKind::Capsule => {
            let c = color(p.color, 1.0);
            let half = (sx - sy).max(0.0);
            let (a, b) = (place(p, tf, [-half, 0.0]), place(p, tf, [half, 0.0]));
            let w = (sy * 2.0 * tf.scale).max(1.0);
            painter.add(Shape::line_segment([a, b], Stroke::new(w, c)));
            painter.circle_filled(a, w / 2.0, c);
            painter.circle_filled(b, w / 2.0, c);
        }
        PrimKind::Star => {
            // 10-point outline, concave: filled as a triangle fan around the center.
            let col = color(p.color, 1.0);
            let mut mesh = Mesh::default();
            mesh.colored_vertex(place(p, tf, [0.0, 0.0]), col);
            for i in 0..10 {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let r = if i % 2 == 0 { 1.0 } else { 0.42 };
                mesh.colored_vertex(place(p, tf, [a.cos() * r * sx, a.sin() * r * sy]), col);
            }
            for i in 0..10u32 {
                mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 10);
            }
            painter.add(Shape::mesh(mesh));
        }
        PrimKind::Glow => {
            let layers = 6;
            for i in 0..layers {
                let k = 1.0 - i as f32 / layers as f32;
                let alpha = 0.10 + 0.5 * (i as f32 / layers as f32).powi(2);
                filled_ellipse(painter, p, tf, [0.0, 0.0], sx * k, sy * k, color(p.color, alpha));
            }
            filled_ellipse(painter, p, tf, [0.0, 0.0], sx * 0.22, sy * 0.22, color([1.0, 1.0, 1.0, p.color[3]], 0.9));
        }
        PrimKind::Band => {
            // Ellipse slice between lo..hi (+1 = top); screen y grows downward.
            let (lo, hi) = snowy_art::band_unpack(p.round);
            const N: usize = 24;
            let mut pts = Vec::with_capacity(2 * N + 2);
            for i in 0..=N {
                let f = hi + (lo - hi) * i as f32 / N as f32; // right side, top → bottom
                pts.push(place(p, tf, [sx * (1.0 - f * f).max(0.0).sqrt(), -f * sy]));
            }
            for i in 0..=N {
                let f = lo + (hi - lo) * i as f32 / N as f32; // left side, bottom → top
                pts.push(place(p, tf, [-sx * (1.0 - f * f).max(0.0).sqrt(), -f * sy]));
            }
            painter.add(Shape::convex_polygon(pts, color(p.color, 1.0), Stroke::NONE));
        }
        PrimKind::Ring => {
            let th = (p.round.clamp(0.02, 1.0) * sx.min(sy)).max(0.5);
            let pts = ellipse_points(p, tf, [0.0, 0.0], (sx - th / 2.0).max(0.1), (sy - th / 2.0).max(0.1));
            painter.add(Shape::closed_line(pts, Stroke::new((th * tf.scale).max(1.0), color(p.color, 1.0))));
        }
    }
}
