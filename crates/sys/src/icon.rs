//! Procedural 32x32 snowflake icon.

pub const SIZE: u32 = 32;

fn seg_dist(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((px - a.0) * dx + (py - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// RGBA bytes, SIZE*SIZE*4: white six-armed snowflake with a soft blue outline.
pub fn snowflake_rgba() -> Vec<u8> {
    let c = (SIZE as f32 - 1.0) / 2.0;
    let arm = 13.5f32;
    let mut segs: Vec<((f32, f32), (f32, f32))> = Vec::new();
    for k in 0..6 {
        let ang = (k as f32) * std::f32::consts::FRAC_PI_3;
        let dir = (ang.cos(), ang.sin());
        segs.push(((c, c), (c + dir.0 * arm, c + dir.1 * arm)));
        // Two small branches at 60% of the arm.
        let base = (c + dir.0 * arm * 0.6, c + dir.1 * arm * 0.6);
        for s in [-1.0f32, 1.0] {
            let ba = ang + s * std::f32::consts::FRAC_PI_3;
            let len = 5.0;
            segs.push((base, (base.0 + ba.cos() * len, base.1 + ba.sin() * len)));
        }
    }
    let mut out = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (px, py) = (x as f32, y as f32);
            let d = segs.iter().map(|&(a, b)| seg_dist(px, py, a, b)).fold(f32::MAX, f32::min);
            let rgba = if d <= 0.8 {
                [255, 255, 255, 255]
            } else if d < 2.0 {
                let a = 1.0 - (d - 0.8) / 1.2;
                [70, 130, 220, (a * 230.0) as u8]
            } else {
                [0, 0, 0, 0]
            };
            out.extend_from_slice(&rgba);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_shape() {
        let v = snowflake_rgba();
        assert_eq!(v.len(), 32 * 32 * 4);
        assert!(v.chunks(4).any(|p| p[3] == 255));
        assert!(v.chunks(4).any(|p| p[3] == 0));
    }
}
