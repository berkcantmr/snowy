//! Snowplough truck. Built facing right in unit coordinates (1.0 = truck height, y up);
//! `flip` mirrors it for dir = -1.

use crate::util::*;
use crate::Prim;

pub fn build(base: [f32; 2], height: f32, dir: f32, t: f32, out: &mut Vec<Prim>) {
    let mut p = Pen::new(out, base, height, dir);
    let orange = rgb(0.98, 0.56, 0.12);
    let orange_d = rgb(0.86, 0.42, 0.08);
    let dark = rgb(0.16, 0.17, 0.20);
    let steel = rgb(0.38, 0.47, 0.60);

    p.ell(0.1, 0.012, 1.0, 0.012, [0.12, 0.22, 0.45, 0.2]);

    // plough frame + blade (front, behind the body edge)
    p.line(0.55, 0.36, 0.92, 0.33, 0.06, dark);
    p.line(0.5, 0.27, 0.93, 0.13, 0.04, dark);
    p.rrect(0.98, 0.35, 0.11, 0.32, 0.26, 0.3, steel);
    p.rrect(0.95, 0.38, 0.025, 0.27, 0.26, 1.0, [1.0, 1.0, 1.0, 0.3]);
    p.rrect(1.04, 0.06, 0.11, 0.03, 0.26, 0.5, rgb(0.78, 0.8, 0.84));
    p.rrect(1.04, 0.56, 0.07, 0.035, 0.26, 0.3, rgb(1.0, 0.85, 0.2));
    p.ball(1.2, 0.07, 0.1, 0.07, [1.0, 1.0, 1.0, 1.0]);
    p.ball(1.32, 0.04, 0.07, 0.04, [1.0, 1.0, 1.0, 1.0]);

    // body: rear bed, chassis, cab, hood
    p.rrect(-0.4, 0.43, 0.42, 0.21, 0.0, 0.2, orange_d);
    p.rrect(-0.4, 0.66, 0.34, 0.05, 0.0, 1.0, [1.0, 1.0, 1.0, 1.0]);
    p.rrect(0.0, 0.31, 0.84, 0.12, 0.0, 0.5, dark);
    p.rrect(0.52, 0.46, 0.3, 0.17, 0.0, 0.3, orange);
    p.rrect(0.0, 0.6, 0.27, 0.3, 0.0, 0.25, orange);
    p.rrect(-0.2, 0.55, 0.04, 0.21, 0.0, 1.0, [1.0, 1.0, 1.0, 0.12]);

    // window with a tiny snowman driver
    p.rrect(0.04, 0.7, 0.17, 0.15, 0.0, 0.3, rgb(0.66, 0.86, 0.96));
    p.ball(0.04, 0.68, 0.09, 0.09, [1.0, 1.0, 1.0, 1.0]);
    p.ball(0.01, 0.7, 0.013, 0.013, rgb(0.1, 0.1, 0.13));
    p.ball(0.075, 0.7, 0.013, 0.013, rgb(0.1, 0.1, 0.13));
    p.tri(0.115, 0.665, 0.012, 0.035, -1.57, rgb(1.0, 0.5, 0.1));
    p.line(0.1, 0.8, 0.16, 0.74, 0.02, [1.0, 1.0, 1.0, 0.35]);

    // head light + grille
    p.ell(0.8, 0.5, 0.05, 0.045, rgb(1.0, 0.95, 0.7));
    p.rrect(0.83, 0.38, 0.03, 0.05, 0.0, 0.5, dark);

    // beacon: blinks
    let blink = ((t * 2.0).fract() < 0.5) as u32 as f32;
    let k = 0.2 + 0.8 * blink;
    p.rrect(0.0, 0.915, 0.07, 0.03, 0.0, 0.4, dark);
    p.ell(0.0, 0.965, 0.045, 0.04, [1.0, 0.65 + 0.1 * blink, 0.1, 0.5 + 0.5 * blink]);
    p.glow(0.0, 0.965, 0.2, [1.0, 0.7, 0.1, k * blink]);

    // wheels (hub spins, rolling forward)
    for wx in [-0.5, 0.52] {
        let r = 0.18;
        p.ell(wx, r, r, r, rgb(0.1, 0.1, 0.12));
        p.ell(wx, r, 0.1, 0.1, rgb(0.72, 0.75, 0.8));
        let a = -t * 6.0;
        p.line(wx + 0.08 * a.cos(), r + 0.08 * a.sin(), wx - 0.08 * a.cos(), r - 0.08 * a.sin(), 0.025, rgb(0.35, 0.38, 0.45));
        p.line(wx - 0.08 * a.sin(), r + 0.08 * a.cos(), wx + 0.08 * a.sin(), r - 0.08 * a.cos(), 0.025, rgb(0.35, 0.38, 0.45));
        p.ell(wx, r, 0.03, 0.03, rgb(0.3, 0.32, 0.38));
    }
}
