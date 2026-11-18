//! Generates assets/snowy.ico and assets/snowy-256.png.
//! Run from repo root: cargo run --manifest-path assets/gen/Cargo.toml --release
use std::f32::consts::TAU;

const MASTER: usize = 1024;
const SIZES: [usize; 7] = [16, 24, 32, 48, 64, 128, 256];

type Seg = ((f32, f32), (f32, f32), f32);

fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

fn flake_segments(s: f32, bold: f32, detail: bool) -> Vec<Seg> {
    let c = (s / 2.0, s / 2.0);
    let r = s * 0.335;
    let main_w = s * 0.026 * bold;
    let br_w = s * 0.019 * bold;
    let mut v = Vec::new();
    let at = |a: f32, d: f32| (c.0 + a.cos() * d, c.1 + a.sin() * d);
    for i in 0..6 {
        let a = -TAU / 4.0 + i as f32 * TAU / 6.0;
        v.push((c, at(a, r), main_w));
        let list: &[(f32, f32)] = if detail { &[(0.52, 0.30), (0.76, 0.20)] } else { &[(0.58, 0.30)] };
        for &(d, len) in list {
            let b = at(a, r * d);
            for sgn in [-1.0f32, 1.0] {
                let ba = a + sgn * 0.95;
                v.push((b, (b.0 + ba.cos() * r * len, b.1 + ba.sin() * r * len), br_w));
            }
        }
    }
    v
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Premultiplied-free RGBA float render at MASTER size.
fn render(bold: f32, detail: bool) -> Vec<[f32; 4]> {
    let s = MASTER as f32;
    let segs = flake_segments(s, bold, detail);
    let margin = s * 0.04;
    let half = s / 2.0 - margin;
    let radius = s * 0.22;
    let outline = s * 0.011 * bold.max(1.0);
    let mut img = vec![[0.0f32; 4]; MASTER * MASTER];
    for y in 0..MASTER {
        for x in 0..MASTER {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            // rounded-square SDF
            let q = ((p.0 - s / 2.0).abs() - (half - radius), (p.1 - s / 2.0).abs() - (half - radius));
            let tile_d = q.0.max(0.0).hypot(q.1.max(0.0)) + q.0.max(q.1).min(0.0) - radius;
            let tile_a = (0.5 - tile_d).clamp(0.0, 1.0);
            if tile_a <= 0.0 {
                continue;
            }
            let ty = y as f32 / s;
            let tx = x as f32 / s;
            // diagonal winter-blue gradient
            let g = (ty * 0.8 + tx * 0.2).clamp(0.0, 1.0);
            let mut col = [lerp(0.36, 0.10, g), lerp(0.69, 0.35, g), lerp(0.97, 0.74, g)];
            // soft top highlight
            let hl = (1.0 - ty * 2.2).max(0.0) * 0.14;
            for ch in &mut col {
                *ch = *ch + (1.0 - *ch) * hl;
            }
            // inner edge darkening for depth
            let edge = ((tile_d + s * 0.02) / (s * 0.02)).clamp(0.0, 1.0) * 0.10;
            for ch in &mut col {
                *ch *= 1.0 - edge;
            }
            // snowflake
            let mut d = f32::MAX;
            for &(a, b, w) in &segs {
                d = d.min(seg_dist(p, a, b) - w);
            }
            // drop shadow
            let mut sd = f32::MAX;
            let sp = (p.0 - s * 0.006, p.1 - s * 0.014);
            for &(a, b, w) in &segs {
                sd = sd.min(seg_dist(sp, a, b) - w);
            }
            let sh = (1.0 - (sd / (s * 0.03)).clamp(0.0, 1.0)).powi(2) * 0.28;
            for ch in &mut col {
                *ch *= 1.0 - sh;
            }
            let out_a = (0.5 - (d - outline)).clamp(0.0, 1.0) * 0.85;
            let white_a = (0.5 - d).clamp(0.0, 1.0);
            let outline_col = [0.20, 0.45, 0.80];
            for k in 0..3 {
                col[k] = lerp(col[k], outline_col[k], out_a);
                col[k] = lerp(col[k], 1.0, white_a);
            }
            img[y * MASTER + x] = [col[0], col[1], col[2], tile_a];
        }
    }
    img
}

fn downscale(img: &[[f32; 4]], size: usize) -> Vec<u8> {
    let f = MASTER / size;
    let mut out = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let (mut r, mut g, mut b, mut a) = (0.0, 0.0, 0.0, 0.0);
            for dy in 0..f {
                for dx in 0..f {
                    let p = img[(y * f + dy) * MASTER + x * f + dx];
                    r += p[0] * p[3];
                    g += p[1] * p[3];
                    b += p[2] * p[3];
                    a += p[3];
                }
            }
            let n = (f * f) as f32;
            let (r, g, b) = if a > 0.0 { (r / a, g / a, b / a) } else { (0.0, 0.0, 0.0) };
            for v in [r, g, b, a / n] {
                out.push((v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
    }
    out
}

fn encode_png(rgba: &[u8], size: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, size as u32, size as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Best);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(rgba).unwrap();
    }
    buf
}

fn main() {
    let detailed = render(1.0, true);
    let bold = render(1.7, false);
    let mut pngs = Vec::new();
    for &s in &SIZES {
        let src = if s <= 32 { &bold } else { &detailed };
        let rgba = downscale(src, s);
        pngs.push((s, encode_png(&rgba, s)));
    }
    std::fs::create_dir_all("assets").unwrap();
    std::fs::write("assets/snowy-256.png", &pngs.last().unwrap().1).unwrap();
    // ICO with PNG-compressed entries.
    let mut ico = vec![0u8, 0, 1, 0];
    ico.extend((pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len();
    for (s, data) in &pngs {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend([dim, dim, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((data.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, d) in &pngs {
        ico.extend(d);
    }
    std::fs::write("assets/snowy.ico", ico).unwrap();
    println!("wrote assets/snowy.ico and assets/snowy-256.png");
}

