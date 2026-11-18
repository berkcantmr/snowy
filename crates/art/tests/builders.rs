use snowy_art::*;
use snowy_core::props::{SnowmanLook, TreeLook};

const H: f32 = 200.0;
const BASE: [f32; 2] = [500.0, 800.0];

fn check(prims: &[Prim], what: &str, max_w: Option<f32>, max_count: usize) {
    assert!(!prims.is_empty(), "{what}: empty");
    assert!(prims.len() <= max_count, "{what}: {} prims > {max_count}", prims.len());
    for p in prims {
        assert!(p.pos.iter().all(|v| v.is_finite()), "{what}: pos {p:?}");
        assert!(p.size.iter().all(|v| v.is_finite() && *v > 0.0), "{what}: size {p:?}");
        assert!(p.rot.is_finite(), "{what}: rot {p:?}");
        assert!(p.color.iter().all(|v| (0.0..=1.0).contains(v)), "{what}: color {p:?}");
        assert!((0.0..=1.0).contains(&p.round), "{what}: round {p:?}");
    }
    let b = bounds(prims);
    assert!(b.iter().all(|v| v.is_finite()), "{what}: bounds {b:?}");
    assert!(b[3] <= BASE[1] + 2.0, "{what}: goes below the ground: {b:?}");
    if let Some(w) = max_w {
        assert!(b[2] - b[0] <= w, "{what}: too wide {}", b[2] - b[0]);
        assert!(b[1] >= BASE[1] - 1.3 * H, "{what}: too tall {b:?}");
    }
}

#[test]
fn every_snowman_look_is_valid() {
    let mut out = Vec::new();
    let mut maxn = 0;
    for balls in [2u8, 3] {
        for code in 0..8u32.pow(4) {
            let look = SnowmanLook {
                hat: (code % 8) as u8,
                face: (code / 8 % 8) as u8,
                item: (code / 64 % 8) as u8,
                outfit: (code / 512 % 8) as u8,
            };
            out.clear();
            snowman(&look, balls, BASE, H, 1.3, &mut out);
            check(&out, &format!("{look:?}/{balls}"), Some(1.6 * H), 130);
            maxn = maxn.max(out.len());
            let mut again = Vec::new();
            snowman(&look, balls, BASE, H, 1.3, &mut again);
            assert_eq!(out, again, "snowman not deterministic");
        }
    }
    println!("max snowman prims: {maxn}");
}

#[test]
fn every_tree_look_is_valid() {
    let mut out = Vec::new();
    let mut maxn = 0;
    for code in 0..8u32.pow(4) {
        let look = TreeLook {
            lights: (code % 8) as u8,
            ornaments: (code / 8 % 8) as u8,
            topper: (code / 64 % 8) as u8,
            garland: (code / 512 % 8) as u8,
        };
        for grow in [0.05, 0.5, 1.0] {
            out.clear();
            tree(&look, BASE, H, grow, 2.1, &mut out);
            check(&out, &format!("{look:?} grow {grow}"), Some(1.6 * H * grow), 220);
            maxn = maxn.max(out.len());
        }
        let mut again = Vec::new();
        tree(&look, BASE, H, 1.0, 2.1, &mut again);
        out.clear();
        tree(&look, BASE, H, 1.0, 2.1, &mut out);
        assert_eq!(out, again, "tree not deterministic");
    }
    println!("max tree prims: {maxn}");
}

#[test]
fn plough_both_directions() {
    for dir in [1.0f32, -1.0] {
        for t in [0.0f32, 0.3, 0.9, 5.5] {
            let mut out = Vec::new();
            plough(BASE, 80.0, dir, t, &mut out);
            check(&out, "plough", None, 40);
        }
    }
    // mirrored: facing left extends to the left of the base, facing right to the right
    let (mut l, mut r) = (Vec::new(), Vec::new());
    plough(BASE, 80.0, -1.0, 0.0, &mut l);
    plough(BASE, 80.0, 1.0, 0.0, &mut r);
    let (bl, br) = (bounds(&l), bounds(&r));
    assert!(bl[0] < BASE[0] - (br[2] - BASE[0]) * 0.9 && br[2] > bl[2]);
}

#[test]
fn degenerate_inputs_do_not_panic() {
    let mut out = Vec::new();
    snowman(&SnowmanLook::default(), 9, BASE, 0.0, f32::NAN, &mut out);
    tree(&TreeLook::default(), BASE, f32::NAN, f32::NAN, 0.0, &mut out);
    plough(BASE, -3.0, 0.0, 1e9, &mut out);
    assert!(out.iter().all(|p| p.pos.iter().all(|v| v.is_finite())));
}

#[test]
fn instance_packing_roundtrips() {
    let p = Prim { kind: PrimKind::Capsule, pos: [10.0, 20.0], size: [30.0, 5.0], rot: 0.3, color: [1.0, 0.5, 0.2, 1.0], round: 0.4 };
    let i = to_instance(&p);
    assert!(i.size >= 30.0);
    assert!((i.param[0] * i.size - 30.0).abs() < 1e-3 && (i.param[1] * i.size - 5.0).abs() < 1e-3);
    assert_eq!(i.param[3], 4.0);
    assert_eq!(i.param[2], 0.4);
}

/// Build-time budget: a fully decorated tree. Run with `cargo test -p snowy-art --release -- --ignored`.
#[test]
#[ignore]
fn bench_tree_build() {
    let look = TreeLook { lights: 5, ornaments: 6, topper: 7, garland: 1 };
    let mut out = Vec::with_capacity(256);
    let n = 20000;
    let t0 = std::time::Instant::now();
    for i in 0..n {
        out.clear();
        tree(&look, BASE, H, 1.0, i as f32 * 0.016, &mut out);
        std::hint::black_box(&out);
    }
    let per = t0.elapsed().as_secs_f64() * 1e6 / n as f64;
    println!("tree build: {per:.2} us, {} prims", out.len());
    assert!(per < 50.0, "{per} us");
}

#[test]
fn yeti_all_poses() {
    let mut maxn = 0;
    for dir in [1.0f32, -1.0] {
        for (carrying, scared) in [(false, false), (true, false), (false, true)] {
            for k in 0..8 {
                let pose = YetiPose { walk_phase: k as f32 * 0.8, carrying, scared };
                let mut out = Vec::new();
                yeti(BASE, H, pose, dir, 1.7, &mut out);
                check(&out, "yeti", Some(1.2 * H), 90);
                maxn = maxn.max(out.len());
                let mut again = Vec::new();
                yeti(BASE, H, pose, dir, 1.7, &mut again);
                assert_eq!(out, again, "yeti not deterministic");
            }
        }
    }
    let mut out = Vec::new();
    yeti(BASE, -3.0, YetiPose { walk_phase: f32::NAN, ..Default::default() }, 0.0, f32::INFINITY, &mut out);
    check(&out, "yeti degenerate", None, 90);
    println!("max yeti prims: {maxn}");
}

#[test]
fn gift_all_stages() {
    for open in [0.0f32, 0.2, 0.5, 0.8, 1.0, 7.0, f32::NAN] {
        let mut out = Vec::new();
        gift(BASE, H, open, 0.9, &mut out);
        check(&out, "gift", None, 60);
        let mut again = Vec::new();
        gift(BASE, H, open, 0.9, &mut again);
        assert_eq!(out, again);
    }
    let mut out = Vec::new();
    gift_chute([BASE[0], BASE[1] - 50.0], H, 0.4, &mut out);
    assert!(!out.is_empty() && out.len() <= 60);
    assert!(out.iter().all(|p| p.pos.iter().all(|v| v.is_finite())));
}
