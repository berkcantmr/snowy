//! Pure logic of the prop look editor (no egui types): prop lookup, merging an edit into a
//! freshly loaded file, random looks and thumbnail variants.

use snowy_art::Prim;
use snowy_core::props::{LightsLook, Prop, PropKind, PropsFile, SnowmanLook, TreeLook, LIGHTS_SLOTS, OPTIONS, SNOWMAN_SLOTS, TREE_SLOTS};

pub const SCALE_MIN: f32 = 0.6;
pub const SCALE_MAX: f32 = 1.6;

pub fn find_prop(file: &PropsFile, id: u64) -> Option<Prop> {
    file.props.iter().find(|p| p.id == id).copied()
}

pub fn is_snowman(kind: &PropKind) -> bool {
    matches!(kind, PropKind::Snowman { .. })
}

pub fn is_tree(kind: &PropKind) -> bool {
    matches!(kind, PropKind::Tree { .. })
}

pub fn is_lights(kind: &PropKind) -> bool {
    matches!(kind, PropKind::Lights { .. })
}

/// Applies only the edited look / balls / scale to the prop with the same id in `file`
/// (its position and everything else stay as the file has them). False if the prop is gone.
pub fn merge_edit(file: &mut PropsFile, edited: &Prop) -> bool {
    let Some(p) = file.props.iter_mut().find(|p| p.id == edited.id) else {
        return false;
    };
    match (&mut p.kind, &edited.kind) {
        // the endpoints belong to the main process (dragging); only the look is edited
        (PropKind::Lights { look, .. }, PropKind::Lights { look: new, .. }) => *look = *new,
        (kind, new) => *kind = *new,
    }
    p.scale = clamp_scale(edited.scale);
    true
}

pub fn remove_prop(file: &mut PropsFile, id: u64) -> bool {
    let before = file.props.len();
    file.props.retain(|p| p.id != id);
    file.names.remove(&id);
    file.props.len() != before
}

pub fn clamp_scale(s: f32) -> f32 {
    if s.is_finite() { s.clamp(SCALE_MIN, SCALE_MAX) } else { 1.0 }
}

pub fn slot_names(kind: &PropKind) -> [&'static str; 4] {
    match kind {
        PropKind::Tree { .. } => TREE_SLOTS,
        PropKind::Lights { .. } => LIGHTS_SLOTS,
        PropKind::Snowman { .. } => SNOWMAN_SLOTS,
    }
}

pub fn get_slot(kind: &PropKind, slot: usize) -> u8 {
    match kind {
        PropKind::Snowman { look, .. } => look.get(slot),
        PropKind::Tree { look } => look.get(slot),
        PropKind::Lights { look, .. } => look.get(slot),
    }
}

/// The same prop with one slot set to `value`.
pub fn with_slot(kind: &PropKind, slot: usize, value: u8) -> PropKind {
    let mut k = *kind;
    match &mut k {
        PropKind::Snowman { look, .. } => look.set(slot, value),
        PropKind::Tree { look } => look.set(slot, value),
        PropKind::Lights { look, .. } => look.set(slot, value),
    }
    k
}

/// The 8 variants of `kind` that differ only in `slot` (index = option).
pub fn thumb_variants(kind: &PropKind, slot: usize) -> Vec<PropKind> {
    (0..OPTIONS).map(|o| with_slot(kind, slot, o)).collect()
}

/// Region of the prop's bounds shown in a thumbnail: fractions [x0, y0, x1, y1].
pub fn thumb_crop(kind: &PropKind, slot: usize) -> [f32; 4] {
    if is_lights(kind) {
        [0.0, 0.0, 1.0, 1.0]
    } else if is_tree(kind) {
        match slot {
            2 => [0.1, 0.0, 0.9, 0.34],
            _ => [0.0, 0.1, 1.0, 1.0],
        }
    } else {
        match slot {
            0 => [0.0, 0.0, 1.0, 0.36],
            1 => [0.0, 0.08, 1.0, 0.46],
            3 => [0.0, 0.3, 1.0, 0.78],
            _ => [0.0, 0.0, 1.0, 1.0],
        }
    }
}

/// Crop fractions applied to concrete bounds.
pub fn crop_bounds(b: [f32; 4], frac: [f32; 4]) -> [f32; 4] {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    [b[0] + w * frac[0], b[1] + h * frac[1], b[0] + w * frac[2], b[1] + h * frac[3]]
}

/// Primitives of a string of lights for a thumbnail: a short string centered on `center`,
/// `height` is the reference size (100 = about three bulbs).
pub fn lights_thumb(look: &LightsLook, center: [f32; 2], height: f32, t: f32) -> Vec<Prim> {
    let u = height / 100.0;
    let w = 150.0 * u;
    let mut out = Vec::new();
    snowy_art::lights_scaled(look, [center[0] - w / 2.0, center[1] - 20.0 * u], [center[0] + w / 2.0, center[1] - 20.0 * u], 0.16 * w, t, 0.0, u, &mut out);
    out
}

/// Primitives of a prop standing with its bottom-center at `base` (a lights string hangs
/// from there instead, see `lights_thumb`).
pub fn build_prims(kind: &PropKind, base: [f32; 2], height: f32, t: f32) -> Vec<Prim> {
    let mut out = Vec::new();
    match kind {
        PropKind::Lights { look, .. } => return lights_thumb(look, base, height, t),
        PropKind::Snowman { look, balls } => snowy_art::snowman(look, *balls, base, height, t, &mut out),
        PropKind::Tree { look } => snowy_art::tree(look, base, height, 1.0, t, &mut out),
    }
    out
}

/// Small deterministic PRNG (splitmix64).
pub struct Rng(u64);

impl Rng {
    #[cfg(test)]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn from_time() -> Self {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        Self(n)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: u8) -> u8 {
        (self.next_u64() % n.max(1) as u64) as u8
    }
}

/// A random look of the same kind (snowman keeps its ball count). Differs from `kind` when possible.
pub fn random_kind(kind: &PropKind, rng: &mut Rng) -> PropKind {
    for _ in 0..8 {
        let k = match kind {
            PropKind::Snowman { balls, .. } => PropKind::Snowman {
                look: SnowmanLook { hat: rng.below(OPTIONS), face: rng.below(OPTIONS), item: rng.below(OPTIONS), outfit: rng.below(OPTIONS) },
                balls: *balls,
            },
            PropKind::Tree { .. } => PropKind::Tree {
                look: TreeLook { lights: rng.below(OPTIONS), ornaments: rng.below(OPTIONS), topper: rng.below(OPTIONS), garland: rng.below(OPTIONS) },
            },
            PropKind::Lights { a, b, .. } => PropKind::Lights {
                look: LightsLook { colors: rng.below(OPTIONS), bulbs: rng.below(OPTIONS), pattern: rng.below(OPTIONS), wire: rng.below(OPTIONS) },
                a: *a,
                b: *b,
            },
        };
        if k != *kind {
            return k;
        }
    }
    *kind
}

pub fn with_balls(kind: &PropKind, n: u8) -> PropKind {
    match *kind {
        PropKind::Snowman { look, .. } => PropKind::Snowman { look, balls: n.clamp(2, 3) },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> PropsFile {
        PropsFile {
            props: vec![
                Prop { id: 1, kind: PropKind::Snowman { look: SnowmanLook::default(), balls: 3 }, x: 0.3, scale: 1.0 },
                Prop { id: 2, kind: PropKind::Tree { look: TreeLook::default() }, x: 0.7, scale: 1.2 },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn lookup() {
        let f = file();
        assert_eq!(find_prop(&f, 2).unwrap().x, 0.7);
        assert!(find_prop(&f, 9).is_none());
    }

    #[test]
    fn merge_keeps_concurrent_position() {
        let mut edited = find_prop(&file(), 1).unwrap();
        edited.kind = PropKind::Snowman { look: SnowmanLook { hat: 3, ..Default::default() }, balls: 2 };
        edited.scale = 9.0;
        edited.x = 0.1; // stale position in the editor must not be written
        let mut fresh = file();
        fresh.props[0].x = 0.55; // main process moved it meanwhile
        assert!(merge_edit(&mut fresh, &edited));
        assert_eq!(fresh.props[0].x, 0.55);
        assert_eq!(fresh.props[0].scale, SCALE_MAX);
        assert_eq!(fresh.props[0].kind, edited.kind);
        assert_eq!(fresh.props[1], file().props[1]);
    }

    #[test]
    fn merge_does_not_resurrect() {
        let edited = find_prop(&file(), 1).unwrap();
        let mut fresh = file();
        remove_prop(&mut fresh, 1);
        assert!(!merge_edit(&mut fresh, &edited));
        assert_eq!(fresh.props.len(), 1);
        assert!(!remove_prop(&mut fresh, 1));
    }

    #[test]
    fn variants_change_only_one_slot() {
        let base = PropKind::Snowman { look: SnowmanLook { hat: 1, face: 2, item: 3, outfit: 4 }, balls: 3 };
        let v = thumb_variants(&base, 1);
        assert_eq!(v.len(), 8);
        for (o, k) in v.iter().enumerate() {
            let PropKind::Snowman { look, balls } = k else { panic!() };
            assert_eq!((look.hat, look.face, look.item, look.outfit, *balls), (1, o as u8, 3, 4, 3));
        }
        let t = thumb_variants(&PropKind::Tree { look: TreeLook::default() }, 2);
        assert_eq!(get_slot(&t[5], 2), 5);
        assert_eq!(get_slot(&t[5], 0), 0);
    }

    #[test]
    fn random_is_valid_and_keeps_balls() {
        let base = PropKind::Snowman { look: SnowmanLook::default(), balls: 2 };
        let mut rng = Rng::new(42);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            let k = random_kind(&base, &mut rng);
            let PropKind::Snowman { look, balls } = k else { panic!() };
            assert_eq!(balls, 2);
            for s in 0..4 {
                assert!(look.get(s) < OPTIONS);
            }
            seen.insert(look.hat);
        }
        assert!(seen.len() > 3);
        assert!(is_tree(&random_kind(&PropKind::Tree { look: TreeLook::default() }, &mut rng)));
    }

    #[test]
    fn lights_variants_and_merge() {
        let kind = PropKind::Lights { look: LightsLook { colors: 1, bulbs: 2, pattern: 3, wire: 4 }, a: [0.3, 0.2], b: [0.7, 0.2] };
        assert_eq!(slot_names(&kind), ["colors", "bulbs", "pattern", "wire"]);
        for slot in 0..4 {
            let v = thumb_variants(&kind, slot);
            assert_eq!(v.len(), 8);
            for (o, k) in v.iter().enumerate() {
                assert_eq!(get_slot(k, slot), o as u8);
                for other in (0..4).filter(|s| *s != slot) {
                    assert_eq!(get_slot(k, other), get_slot(&kind, other));
                }
                assert!(matches!(k, PropKind::Lights { a, b, .. } if *a == [0.3, 0.2] && *b == [0.7, 0.2]));
                assert!(!build_prims(k, [50.0, 50.0], 100.0, 0.0).is_empty());
            }
            let c = thumb_crop(&kind, slot);
            assert!(c[0] < c[2] && c[1] < c[3]);
        }
        let mut rng = Rng::new(7);
        assert!(is_lights(&random_kind(&kind, &mut rng)));
        // an edit never overwrites endpoints the main process moved meanwhile
        let mut fresh = PropsFile { props: vec![Prop { id: 5, kind, x: 0.5, scale: 1.0 }], ..Default::default() };
        if let PropKind::Lights { a, .. } = &mut fresh.props[0].kind {
            *a = [0.1, 0.5];
        }
        let edited = Prop { id: 5, kind: with_slot(&kind, 0, 6), x: 0.5, scale: 1.0 };
        assert!(merge_edit(&mut fresh, &edited));
        assert!(matches!(fresh.props[0].kind, PropKind::Lights { look, a, b } if look.colors == 6 && a == [0.1, 0.5] && b == [0.7, 0.2]));
    }

    #[test]
    fn crops_are_valid_fractions() {
        for tree in [false, true] {
            let kind = if tree {
                PropKind::Tree { look: TreeLook::default() }
            } else {
                PropKind::Snowman { look: SnowmanLook::default(), balls: 3 }
            };
            for s in 0..4 {
                let c = thumb_crop(&kind, s);
                assert!(c[0] < c[2] && c[1] < c[3] && c.iter().all(|v| (0.0..=1.0).contains(v)));
            }
        }
        assert_eq!(crop_bounds([0.0, 10.0, 100.0, 110.0], [0.0, 0.0, 1.0, 0.5]), [0.0, 10.0, 100.0, 60.0]);
    }

    #[test]
    fn balls_toggle() {
        let k = PropKind::Snowman { look: SnowmanLook::default(), balls: 3 };
        assert!(matches!(with_balls(&k, 2), PropKind::Snowman { balls: 2, .. }));
        assert!(matches!(with_balls(&k, 7), PropKind::Snowman { balls: 3, .. }));
    }
}
