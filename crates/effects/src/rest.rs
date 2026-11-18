//! Shared landing test for particles that fall onto window ledges or the ground.

use snowy_core::Scene;

/// Finds where a point moving down from `prev` to `new` (y) at column `x` lands.
/// Returns the surface id (None = ground) and the y of the resting line.
/// `scene.surfaces` is sorted by y, so only segments with y in (prev, new] are examined.
pub fn find_land(scene: &Scene, x: f32, prev: f32, new: f32) -> Option<(Option<u64>, f32)> {
    if !prev.is_finite() || !new.is_finite() || new <= prev {
        return None;
    }
    let mut best = None;
    for s in scene.surfaces {
        if s.y <= prev {
            continue;
        }
        if s.y > new {
            break;
        }
        if x >= s.x0 && x <= s.x1 {
            best = Some((Some(s.id), s.y));
            break;
        }
    }
    if let Some(g) = scene.ground {
        if g > prev && g <= new && best.is_none_or(|b| g < b.1) {
            best = Some((None, g));
        }
    }
    best
}
