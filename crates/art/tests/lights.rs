use snowy_art::*;
use snowy_core::props::LightsLook;

fn glows(p: &[Prim]) -> usize {
    p.iter().filter(|q| q.kind == PrimKind::Glow).count()
}

fn valid(p: &[Prim], what: &str) {
    assert!(!p.is_empty());
    for q in p {
        assert!(q.pos.iter().chain(&q.size).all(|v| v.is_finite()), "{what}: {q:?}");
        assert!(q.size.iter().all(|v| *v > 0.0), "{what}: {q:?}");
        assert!(q.rot.is_finite());
        assert!(q.color.iter().all(|v| (0.0..=1.0).contains(v)), "{what}: {q:?}");
        assert!((0.0..=1.0).contains(&q.round));
    }
    assert!(bounds(p).iter().all(|v| v.is_finite()));
}

#[test]
fn every_option_is_valid_and_deterministic() {
    for code in 0..8u32.pow(4) {
        let look = LightsLook { colors: (code % 8) as u8, bulbs: (code / 8 % 8) as u8, pattern: (code / 64 % 8) as u8, wire: (code / 512 % 8) as u8 };
        for t in [0.0f32, 1.7, 123.4] {
            let (mut a, mut b) = (Vec::new(), Vec::new());
            lights(&look, [100.0, 200.0], [900.0, 260.0], 48.0, t, 0.05, &mut a);
            lights(&look, [100.0, 200.0], [900.0, 260.0], 48.0, t, 0.05, &mut b);
            valid(&a, &format!("{look:?}"));
            assert_eq!(a, b);
        }
    }
}

#[test]
fn degenerate_input_stays_finite() {
    let look = LightsLook::default();
    for (a, b, sag, t, sway) in [
        ([5.0, 5.0], [5.0, 5.0], 0.0, 0.0, 0.0),
        ([f32::NAN, 0.0], [10.0, f32::INFINITY], f32::NAN, f32::NAN, f32::NAN),
        ([0.0, 0.0], [1.0, 0.0], -5.0, 1e9, 9.0),
    ] {
        let mut out = Vec::new();
        lights(&look, a, b, sag, t, sway, &mut out);
        valid(&out, "degenerate");
    }
}

#[test]
fn prim_cap_for_long_strings() {
    let mut worst = 0;
    for bulbs in 0..8u8 {
        for wire in [0u8, 7] {
            let look = LightsLook { colors: 1, bulbs, pattern: 1, wire };
            let mut out = Vec::new();
            lights(&look, [0.0, 100.0], [3840.0, 100.0], 230.0, 2.0, 0.0, &mut out);
            worst = worst.max(out.len());
            assert!(out.len() <= 260, "bulbs {bulbs}: {}", out.len());
        }
    }
    assert!(worst > 100);
}

#[test]
fn bulb_count_scales_with_length() {
    let look = LightsLook::default();
    let count = |len: f32| {
        let mut out = Vec::new();
        lights(&look, [0.0, 0.0], [len, 0.0], 0.06 * len, 0.0, 0.0, &mut out);
        glows(&out)
    };
    let (s, m, l) = (count(200.0), count(600.0), count(1200.0));
    assert!(s >= 2 && s < m && m < l, "{s} {m} {l}");
    // sizes scale with the unit: bigger unit, fewer bulbs for the same length
    let mut out = Vec::new();
    lights_scaled(&look, [0.0, 0.0], [1200.0, 0.0], 72.0, 0.0, 0.0, 2.0, &mut out);
    assert!(glows(&out) < l);
}

#[test]
fn patterns_animate_and_steady_does_not() {
    let at = |pattern: u8, t: f32| {
        let look = LightsLook { colors: 0, bulbs: 0, pattern, wire: 0 };
        let mut out = Vec::new();
        lights(&look, [0.0, 0.0], [800.0, 0.0], 48.0, t, 0.0, &mut out);
        out
    };
    assert_eq!(at(0, 0.0), at(0, 5.0));
    for p in 1..8u8 {
        assert!((0..40).any(|k| at(p, k as f32 * 0.31) != at(p, 0.0)), "pattern {p} is static");
    }
}

#[test]
fn sway_moves_the_middle_not_the_ends() {
    let look = LightsLook { wire: 1, ..Default::default() };
    let (mut a, mut b) = (Vec::new(), Vec::new());
    lights(&look, [0.0, 0.0], [800.0, 0.0], 48.0, 0.0, 0.0, &mut a);
    lights(&look, [0.0, 0.0], [800.0, 0.0], 48.0, 0.0, 0.07, &mut b);
    assert_ne!(a, b);
    assert_eq!(a[0].pos[1], b[0].pos[1]);
}
