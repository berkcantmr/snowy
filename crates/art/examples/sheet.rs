//! Renders a preview sheet of every prop option to a PNG without the GPU.
//! A tiny CPU rasterizer evaluates the same shape math as `shaders/props.hlsl` per pixel,
//! starting from `to_instance` so the quad padding / param packing is exercised too.
//!
//! Usage: sheet [out_dir] ; env SCALE (default 1) supersizes the output.

use snowy_art::*;
use snowy_core::props::{LightsLook, SnowmanLook, TreeLook};
use std::f32::consts::PI;

type V2 = [f32; 2];

fn len(v: V2) -> f32 {
    v[0].hypot(v[1])
}
fn sat(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = sat((x - a) / (b - a));
    t * t * (3.0 - 2.0 * t)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

fn sd_ellipse(p: V2, h: V2) -> f32 {
    (len([p[0] / h[0], p[1] / h[1]]) - 1.0) * h[0].min(h[1])
}
fn sd_round_rect(p: V2, h: V2, rad: f32) -> f32 {
    let q = [p[0].abs() - h[0] + rad, p[1].abs() - h[1] + rad];
    len([q[0].max(0.0), q[1].max(0.0)]) + q[0].max(q[1]).min(0.0) - rad
}
fn sd_triangle(p: V2, h: V2, rad: f32) -> f32 {
    let q = [p[0].abs(), p[1] + h[1]];
    let nl = len([2.0 * h[1], -h[0]]);
    let n = [2.0 * h[1] / nl, -h[0] / nl];
    let d1 = q[0] * n[0] + q[1] * n[1];
    let d2 = p[1] - h[1];
    let v = [d1 + rad, d2 + rad];
    len([v[0].max(0.0), v[1].max(0.0)]) + v[0].max(v[1]).min(0.0) - rad
}
fn sd_capsule(p: V2, h: V2) -> f32 {
    let a = (h[0] - h[1]).max(0.0);
    len([p[0] - p[0].clamp(-a, a), p[1]]) - h[1]
}
fn sd_star(p: V2, h: V2) -> f32 {
    let n = [p[0] / h[0], p[1] / h[1]];
    let mut ang = n[0].atan2(-n[1]);
    let sector = 2.0 * PI / 5.0;
    ang = ((ang + sector * 0.5 + 10.0 * sector) % sector - sector * 0.5).abs();
    let l = len(n);
    let pt = [l * ang.cos(), l * ang.sin()];
    let a = [1.0, 0.0];
    let b = [0.42 * (PI / 5.0).cos(), 0.42 * (PI / 5.0).sin()];
    let e = [b[0] - a[0], b[1] - a[1]];
    let w = [pt[0] - a[0], pt[1] - a[1]];
    let t = sat((w[0] * e[0] + w[1] * e[1]) / (e[0] * e[0] + e[1] * e[1]));
    let dd = len([w[0] - e[0] * t, w[1] - e[1] * t]);
    let side = e[0] * w[1] - e[1] * w[0];
    (if side > 0.0 { -dd } else { dd }) * h[0].min(h[1])
}

/// Pixel shader mirror. `uv` in -1..1 over the padded quad. Returns premultiplied RGBA.
fn ps(i: &snowy_core::Instance, uv: V2) -> [f32; 4] {
    let h = [(i.param[0] * i.size).max(0.01), (i.param[1] * i.size).max(0.01)];
    let p = [uv[0] * i.size, uv[1] * i.size];
    let mh = h[0].min(h[1]);
    let rnd = i.param[2];
    let kind = (i.param[3] + 0.5) as i32;
    let mut rgb = [i.color[0], i.color[1], i.color[2]];
    let alpha = i.color[3];
    let sd;
    match kind {
        0 => sd = sd_ellipse(p, h),
        1 => {
            sd = sd_ellipse(p, h);
            let n = [p[0] / h[0], p[1] / h[1]];
            let d = len(n);
            let nz = (1.0 - d * d).max(0.0).sqrt();
            let l = [-0.5f32, -0.6, 0.62];
            let ll = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
            let diff = sat((n[0] * l[0] + n[1] * l[1] + nz * l[2]) / ll);
            let cool = [0.70, 0.78, 0.97];
            let k = lerp3(cool, [1.0; 3], smoothstep(0.0, 0.7, diff));
            rgb = [rgb[0] * k[0], rgb[1] * k[1], rgb[2] * k[2]];
            let nd = [n[0] / d.max(0.001), n[1] / d.max(0.001)];
            let rim = smoothstep(0.78, 1.0, d) * (0.45 + 0.55 * sat(nd[0] * 0.6 + nd[1] * 0.8));
            rgb = lerp3(rgb, [rgb[0] * 0.6, rgb[1] * 0.68, rgb[2] * 0.9], rim * 0.7);
            let hl = smoothstep(0.3, 0.0, len([n[0] + 0.38, n[1] + 0.42]));
            rgb = lerp3(rgb, [1.0; 3], hl * 0.45);
        }
        2 => sd = sd_round_rect(p, h, rnd * mh),
        3 => sd = sd_triangle(p, h, rnd * mh * 0.5),
        4 => sd = sd_capsule(p, h),
        5 => sd = sd_star(p, h),
        6 => {
            let r = len([p[0] / h[0], p[1] / h[1]]);
            let halo = sat(1.0 - r).powf(2.2);
            let core = 1.0 - smoothstep(0.16, 0.36, r);
            rgb = lerp3(rgb, [1.0; 3], core * 0.6);
            let a = sat(halo * 0.75 + core) * alpha;
            return [rgb[0] * a, rgb[1] * a, rgb[2] * a, a];
        }
        8 => {
            let (lo, hi) = snowy_art::band_unpack(rnd);
            let e = sd_ellipse(p, h);
            let band = (-hi * h[1] - p[1]).max(p[1] + lo * h[1]);
            sd = e.max(band);
            let n = [p[0] / h[0], p[1] / h[1]];
            let d = len(n);
            let k = (0.84 + 0.16 * sat(-n[1] * 0.5 + 0.5)) * (1.0 - smoothstep(0.72, 1.0, d) * (0.18 + 0.14 * sat(n[0])));
            rgb = [rgb[0] * k, rgb[1] * k, rgb[2] * k];
        }
        _ => {
            let e = sd_ellipse(p, h);
            sd = e.max(-e - rnd * mh);
        }
    }
    let a = sat(0.5 - sd) * alpha;
    [rgb[0] * a, rgb[1] * a, rgb[2] * a, a]
}

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn draw(&mut self, prims: &[Prim], scale: f32, off: V2) {
        for pr in prims {
            let mut q = *pr;
            q.pos = [q.pos[0] * scale + off[0], q.pos[1] * scale + off[1]];
            q.size = [q.size[0] * scale, q.size[1] * scale];
            let inst = to_instance(&q);
            let (s, c) = inst.rot.sin_cos();
            let ext = inst.size * (s.abs() + c.abs());
            let x0 = ((inst.pos[0] - ext).floor().max(0.0)) as usize;
            let x1 = ((inst.pos[0] + ext).ceil().min(self.w as f32)) as usize;
            let y0 = ((inst.pos[1] - ext).floor().max(0.0)) as usize;
            let y1 = ((inst.pos[1] + ext).ceil().min(self.h as f32)) as usize;
            for y in y0..y1 {
                for x in x0..x1 {
                    let d = [x as f32 + 0.5 - inst.pos[0], y as f32 + 0.5 - inst.pos[1]];
                    // inverse of the vertex shader's rotation
                    let l = [d[0] * c + d[1] * s, -d[0] * s + d[1] * c];
                    let uv = [l[0] / inst.size, l[1] / inst.size];
                    if uv[0].abs() > 1.0 || uv[1].abs() > 1.0 {
                        continue;
                    }
                    let src = ps(&inst, uv);
                    let dst = &mut self.px[y * self.w + x];
                    for k in 0..4 {
                        dst[k] = src[k] + dst[k] * (1.0 - src[3]);
                    }
                }
            }
        }
    }
}

fn save(path: &std::path::Path, cv: &Canvas) {
    let mut buf = Vec::with_capacity(cv.w * cv.h * 3);
    for px in &cv.px {
        for v in &px[..3] {
            buf.push((v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), cv.w as u32, cv.h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&buf).unwrap();
}

fn new_canvas(w: usize, h: usize) -> Canvas {
    let mut px = vec![[0.0; 4]; w * h];
    for y in 0..h {
        let t = y as f32 / h as f32;
        let c = lerp3([0.30, 0.44, 0.66], [0.42, 0.58, 0.78], t);
        for x in 0..w {
            px[y * w + x] = [c[0], c[1], c[2], 1.0];
        }
    }
    Canvas { w, h, px }
}

enum Item {
    Snow(SnowmanLook, u8),
    Tree(TreeLook),
    Plough(f32),
    Yeti(YetiPose, bool),
    Gift(f32, bool),
}

fn render_row(items: &[Item], scale: f32, t: f32) -> Canvas {
    let (cw, ch) = (230.0 * scale, 270.0 * scale);
    let mut cv = new_canvas((cw * items.len() as f32) as usize, ch as usize);
    for (n, it) in items.iter().enumerate() {
        let x = cw * (n as f32 + 0.5);
        let base = [x, ch - 14.0 * scale];
        let mut out = Vec::new();
        match it {
            Item::Snow(l, b) => snowman(l, *b, base, 215.0 * scale, t, &mut out),
            Item::Tree(l) => tree(l, base, 238.0 * scale, 1.0, t, &mut out),
            Item::Plough(d) => plough(base, 88.0 * scale, *d, t, &mut out),
            Item::Yeti(pose, carry) => {
                let h = 170.0 * scale;
                yeti(base, h, *pose, 1.0, t, &mut out);
                if *carry {
                    let l = SnowmanLook { hat: 1, face: 1, item: 1, outfit: 1 };
                    snowman(&l, 3, [base[0] + 0.02 * h, base[1] - 1.0 * h], 0.5 * h, t, &mut out);
                }
            }
            Item::Gift(open, chute) => {
                let h = 70.0 * scale;
                let b = [base[0], base[1]];
                if *chute {
                    gift_chute([b[0], b[1] - 40.0 * scale], h, t, &mut out);
                    gift([b[0], b[1] - 40.0 * scale], h, *open, t, &mut out);
                } else {
                    gift(b, h, *open, t, &mut out);
                }
            }
        }
        cv.draw(&out, 1.0, [0.0, 0.0]);
    }
    cv
}


/// Four columns of strings (one row per option): bulbs, colors, patterns (each at its own
/// moment in time), wires.
fn lights_sheet(scale: f32, which: &[usize]) -> Canvas {
    let (colw, rowh) = (660.0 * scale, 110.0 * scale);
    let mut cv = new_canvas((colw * which.len() as f32) as usize, (rowh * 8.0) as usize);
    for (ci, &slot) in which.iter().enumerate() {
        for v in 0..8u8 {
            let mut look = LightsLook { colors: 0, bulbs: 0, pattern: 0, wire: 0 };
            // a mixed base so every column shows the option against a neutral rest
            look.colors = 1;
            look.set(slot, v);
            let x0 = colw * ci as f32 + 40.0 * scale;
            let y = rowh * v as f32 + 26.0 * scale;
            let mut out = Vec::new();
            let t = 0.7 + v as f32 * 0.37;
            snowy_art::lights_scaled(&look, [x0, y], [x0 + colw - 80.0 * scale, y + 6.0 * scale], 40.0 * scale, t, 0.0, scale, &mut out);
            cv.draw(&out, 1.0, [0.0, 0.0]);
        }
    }
    cv
}

fn stack(rows: Vec<Canvas>) -> Canvas {
    let w = rows[0].w;
    let h: usize = rows.iter().map(|r| r.h).sum();
    let mut px = Vec::with_capacity(w * h);
    for r in rows {
        px.extend(r.px);
    }
    Canvas { w, h, px }
}

fn main() {
    let dir = std::env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::env::temp_dir().join("snowy-art")
    });
    std::fs::create_dir_all(&dir).unwrap();
    let scale: f32 = std::env::var("SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let t = 0.7;

    let snow_row = |slot: usize, balls: u8| -> Vec<Item> {
        (0..8u8)
            .map(|v| {
                let mut l = SnowmanLook { face: 0, ..Default::default() };
                l.set(slot, v);
                Item::Snow(l, balls)
            })
            .collect()
    };
    let tree_row = |slot: usize| -> Vec<Item> {
        (0..8u8)
            .map(|v| {
                let mut l = TreeLook::default();
                l.set(slot, v);
                Item::Tree(l)
            })
            .collect()
    };

    let names = ["hat", "face", "item", "outfit"];
    let mut sections: Vec<(String, Vec<Canvas>)> = Vec::new();
    let mut snow_rows = Vec::new();
    for (s, name) in names.iter().enumerate() {
        let c = render_row(&snow_row(s, if s % 2 == 0 { 3 } else { 2 }), scale, t);
        save(&dir.join(format!("art_{name}.png")), &c);
        snow_rows.push(c);
    }
    sections.push(("snow".into(), snow_rows));
    let tnames = ["lights", "ornaments", "topper", "garland"];
    let mut tree_rows = Vec::new();
    for (s, name) in tnames.iter().enumerate() {
        let c = render_row(&tree_row(s), scale, t);
        save(&dir.join(format!("art_{name}.png")), &c);
        tree_rows.push(c);
    }
    sections.push(("tree".into(), tree_rows));
    let full = vec![
        Item::Snow(SnowmanLook { hat: 3, face: 1, item: 5, outfit: 1 }, 3),
        Item::Snow(SnowmanLook { hat: 1, face: 6, item: 0, outfit: 3 }, 2),
        Item::Snow(SnowmanLook { hat: 2, face: 3, item: 7, outfit: 7 }, 3),
        Item::Tree(TreeLook { lights: 2, ornaments: 3, topper: 1, garland: 4 }),
        Item::Tree(TreeLook { lights: 5, ornaments: 6, topper: 7, garland: 1 }),
        Item::Tree(TreeLook { lights: 1, ornaments: 1, topper: 6, garland: 7 }),
        Item::Plough(1.0),
        Item::Plough(-1.0),
    ];
    let misc = render_row(&full, scale, t);
    save(&dir.join("art_misc.png"), &misc);
    sections.push(("misc".into(), vec![misc]));

    let walk = |ph: f32| Item::Yeti(YetiPose { walk_phase: ph, ..Default::default() }, false);
    let visit = vec![
        walk(0.0),
        walk(PI * 0.5),
        walk(PI),
        walk(PI * 1.5),
        Item::Yeti(YetiPose { walk_phase: 0.0, carrying: true, scared: false }, true),
        Item::Yeti(YetiPose { walk_phase: 0.0, carrying: false, scared: true }, false),
        Item::Gift(0.0, true),
        Item::Gift(0.0, false),
        Item::Gift(0.5, false),
        Item::Gift(1.0, false),
    ];
    let vrow = render_row(&visit, scale, t);
    save(&dir.join("visitor_sheet.png"), &vrow);

    save(&dir.join("lights_sheet.png"), &lights_sheet(scale, &[1, 0]));
    save(&dir.join("lights_sheet2.png"), &lights_sheet(scale, &[2, 3]));

    let all: Vec<Canvas> = sections.into_iter().flat_map(|(_, v)| v).collect();
    save(&dir.join("art_sheet.png"), &stack(all));
    println!("wrote sheets to {}", dir.display());
}
