//! Desktop props (snowmen, Christmas trees): data model and persistence.
//! One JSON file per monitor: %APPDATA%\Snowy\props-<monitor>.json. The main process owns
//! the props and saves on change; the editor window (`snowy.exe --edit <monitor> <id>`)
//! rewrites the prop's look in the same file; the main process reloads on mtime change.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Number of choices per look slot (index 0 = "none"/default for each slot).
pub const OPTIONS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SnowmanLook {
    /// 0 none, 1 top hat, 2 beanie, 3 santa hat, 4 bucket, 5 cowboy hat, 6 crown, 7 earmuffs
    pub hat: u8,
    /// 0 classic coal smile, 1 big grin, 2 surprised, 3 wink, 4 sleepy, 5 tongue out,
    /// 6 sunglasses, 7 rosy cheeks
    pub face: u8,
    /// Held in a stick arm: 0 nothing (both arms up waving), 1 broom, 2 candy cane, 3 shovel,
    /// 4 flag, 5 lantern, 6 gift box, 7 mug
    pub item: u8,
    /// 0 coal buttons only, 1 red scarf, 2 striped scarf, 3 bow tie, 4 vest, 5 cape,
    /// 6 sweater, 7 mittens + scarf
    pub outfit: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeLook {
    /// 0 none, 1 warm white, 2 multicolor, 3 blue, 4 red & green, 5 rainbow twinkle, 6 gold,
    /// 7 purple
    pub lights: u8,
    /// 0 none, 1 red balls, 2 gold balls, 3 mixed balls, 4 candy canes, 5 stars,
    /// 6 snowflakes, 7 pinecones
    pub ornaments: u8,
    /// 0 none, 1 gold star, 2 silver star, 3 bow, 4 snowflake, 5 heart, 6 moon, 7 angel
    pub topper: u8,
    /// 0 none, 1 gold tinsel, 2 silver tinsel, 3 red ribbon, 4 popcorn string, 5 beads,
    /// 6 snow-dusted branches, 7 paper chain
    pub garland: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LightsLook {
    /// 0 warm white, 1 multicolor, 2 blue, 3 red & green, 4 rainbow chase, 5 gold, 6 purple,
    /// 7 candy (red / white)
    pub colors: u8,
    /// 0 classic round, 1 C9 cone, 2 tiny fairy dots, 3 stars, 4 snowflakes, 5 hearts,
    /// 6 icicle drops, 7 mini lanterns
    pub bulbs: u8,
    /// 0 steady, 1 twinkle, 2 slow fade, 3 chase, 4 alternate blink, 5 sparkle, 6 wave,
    /// 7 heartbeat
    pub pattern: u8,
    /// 0 green, 1 white, 2 black, 3 gold, 4 copper, 5 silver, 6 red, 7 invisible
    pub wire: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PropKind {
    /// `balls`: 2 or 3 stacked snowballs.
    Snowman { look: SnowmanLook, balls: u8 },
    Tree { look: TreeLook },
    /// A string of lights hanging between two endpoints, each as a fraction of the monitor
    /// width / height (`Prop::x` and `scale` are unused for it).
    Lights { look: LightsLook, a: [f32; 2], b: [f32; 2] },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Prop {
    /// Unique within the file (random u64).
    pub id: u64,
    pub kind: PropKind,
    /// Horizontal center as a fraction of the monitor width (0..1), so props survive
    /// resolution changes. They always stand on the ground (work-area bottom).
    pub x: f32,
    /// Size multiplier around 1.0 (0.6..1.6). Base height: snowman ~150 px, tree ~220 px at
    /// 1080p (scaled by monitor height / 1080).
    pub scale: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PropsFile {
    pub props: Vec<Prop>,
    /// Local calendar day (days since the Unix epoch) of the last daily gift; 0 = never.
    pub last_gift_day: u32,
}

/// Look slot names for UI / i18n: "look.<slot>.<index>" keys.
pub const SNOWMAN_SLOTS: [&str; 4] = ["hat", "face", "item", "outfit"];
pub const TREE_SLOTS: [&str; 4] = ["lights", "ornaments", "topper", "garland"];
pub const LIGHTS_SLOTS: [&str; 4] = ["colors", "bulbs", "pattern", "wire"];

impl LightsLook {
    pub fn get(&self, slot: usize) -> u8 {
        [self.colors, self.bulbs, self.pattern, self.wire][slot.min(3)]
    }
    pub fn set(&mut self, slot: usize, v: u8) {
        let v = v % OPTIONS;
        match slot {
            0 => self.colors = v,
            1 => self.bulbs = v,
            2 => self.pattern = v,
            _ => self.wire = v,
        }
    }
}

/// Default endpoints of a freshly added string (fractions of the monitor).
pub const DEFAULT_LIGHTS: ([f32; 2], [f32; 2]) = ([0.3, 0.25], [0.7, 0.25]);

fn clamp_point(p: [f32; 2], fallback: [f32; 2]) -> [f32; 2] {
    let f = |v: f32, d: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
    [f(p[0], fallback[0]), f(p[1], fallback[1])]
}

/// Clamps one prop's values into their valid ranges (used on load).
pub fn sanitize_prop(p: &mut Prop) {
    p.x = if p.x.is_finite() { p.x.clamp(0.0, 1.0) } else { 0.5 };
    p.scale = if p.scale.is_finite() { p.scale.clamp(0.6, 1.6) } else { 1.0 };
    match &mut p.kind {
        PropKind::Snowman { look, balls } => {
            *balls = (*balls).clamp(2, 3);
            for s in 0..4 {
                let v = look.get(s);
                look.set(s, v);
            }
        }
        PropKind::Tree { look } => {
            for s in 0..4 {
                let v = look.get(s);
                look.set(s, v);
            }
        }
        PropKind::Lights { look, a, b } => {
            for s in 0..4 {
                let v = look.get(s);
                look.set(s, v);
            }
            *a = clamp_point(*a, DEFAULT_LIGHTS.0);
            *b = clamp_point(*b, DEFAULT_LIGHTS.1);
            if (a[0] - b[0]).hypot(a[1] - b[1]) < 0.02 {
                *a = DEFAULT_LIGHTS.0;
                *b = DEFAULT_LIGHTS.1;
            }
        }
    }
}

impl SnowmanLook {
    pub fn get(&self, slot: usize) -> u8 {
        [self.hat, self.face, self.item, self.outfit][slot.min(3)]
    }
    pub fn set(&mut self, slot: usize, v: u8) {
        let v = v % OPTIONS;
        match slot {
            0 => self.hat = v,
            1 => self.face = v,
            2 => self.item = v,
            _ => self.outfit = v,
        }
    }
}

impl TreeLook {
    pub fn get(&self, slot: usize) -> u8 {
        [self.lights, self.ornaments, self.topper, self.garland][slot.min(3)]
    }
    pub fn set(&mut self, slot: usize, v: u8) {
        let v = v % OPTIONS;
        match slot {
            0 => self.lights = v,
            1 => self.ornaments = v,
            2 => self.topper = v,
            _ => self.garland = v,
        }
    }
}

impl PropsFile {
    /// File for a monitor id like "\\\\.\\DISPLAY1" → "props-DISPLAY1.json".
    pub fn path(monitor_id: &str) -> PathBuf {
        let clean: String = monitor_id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        crate::config::Settings::dir().join(format!("props-{clean}.json"))
    }

    /// Missing/corrupt file → empty. Clamps values into valid ranges.
    pub fn load(monitor_id: &str) -> Self {
        let mut f: PropsFile = std::fs::read_to_string(Self::path(monitor_id))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        for p in &mut f.props {
            sanitize_prop(p);
        }
        f
    }

    /// Atomic save (temp file + rename).
    pub fn save(&self, monitor_id: &str) -> anyhow::Result<()> {
        let path = Self::path(monitor_id);
        std::fs::create_dir_all(crate::config::Settings::dir())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_sanitized() {
        let p = PropsFile::path("\\\\.\\DISPLAY1");
        assert!(p.ends_with("props-DISPLAY1.json"), "{p:?}");
    }

    #[test]
    fn look_set_wraps() {
        let mut l = SnowmanLook::default();
        l.set(0, 9);
        assert_eq!(l.hat, 1);
        let mut t = TreeLook::default();
        t.set(3, 7);
        assert_eq!(t.get(3), 7);
    }

    #[test]
    fn json_roundtrip() {
        let f = PropsFile {
            props: vec![
                Prop { id: 1, kind: PropKind::Snowman { look: SnowmanLook { hat: 3, ..Default::default() }, balls: 3 }, x: 0.3, scale: 1.0 },
                Prop { id: 2, kind: PropKind::Tree { look: TreeLook::default() }, x: 0.7, scale: 1.2 },
            ],
            ..Default::default()
        };
        let s = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<PropsFile>(&s).unwrap(), f);
    }

    #[test]
    fn lights_roundtrip_and_old_files() {
        let f = PropsFile {
            props: vec![Prop {
                id: 3,
                kind: PropKind::Lights { look: LightsLook { colors: 4, bulbs: 5, pattern: 6, wire: 7 }, a: [0.1, 0.2], b: [0.9, 0.3] },
                x: 0.5,
                scale: 1.0,
            }],
            ..Default::default()
        };
        let s = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<PropsFile>(&s).unwrap(), f);
        // a file written before lights existed still parses
        let old = r#"{"props":[{"id":1,"kind":{"Tree":{"look":{"lights":2}}},"x":0.5,"scale":1.0}]}"#;
        let o: PropsFile = serde_json::from_str(old).unwrap();
        assert_eq!(o.props.len(), 1);
        // partial look defaults missing slots
        let part = r#"{"id":2,"kind":{"Lights":{"look":{"colors":3},"a":[0.2,0.2],"b":[0.8,0.2]}},"x":0.5,"scale":1.0}"#;
        let p: Prop = serde_json::from_str(part).unwrap();
        assert!(matches!(p.kind, PropKind::Lights { look, .. } if look.colors == 3 && look.wire == 0));
    }

    #[test]
    fn lights_are_clamped() {
        let mut p = Prop {
            id: 1,
            kind: PropKind::Lights { look: LightsLook { colors: 9, bulbs: 0, pattern: 0, wire: 0 }, a: [-3.0, f32::NAN], b: [4.0, 0.5] },
            x: 0.5,
            scale: 1.0,
        };
        sanitize_prop(&mut p);
        let PropKind::Lights { look, a, b } = p.kind else { panic!() };
        assert_eq!(look.colors, 1);
        assert!(a.iter().chain(&b).all(|v| (0.0..=1.0).contains(v)));
    }
}
