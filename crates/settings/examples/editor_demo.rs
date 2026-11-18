//! Opens the editor for a demo snowman, a demo tree, then demo string lights (3 s each) using
//! a temporary props file for the fake monitor id "DEMO"; an existing props-DEMO.json is
//! backed up and restored. EDITOR_DEMO_IDS="3" opens only the listed props; EDITOR_DEMO_SECS
//! sets the time each stays open.

use snowy_core::props::{LightsLook, Prop, PropKind, PropsFile, SnowmanLook, TreeLook};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let path = PropsFile::path("DEMO");
    let backup = std::fs::read(&path).ok();

    let demo = PropsFile {
        props: vec![
            Prop {
                id: 1,
                kind: PropKind::Snowman { look: SnowmanLook { hat: 3, face: 1, item: 2, outfit: 1 }, balls: 3 },
                x: 0.3,
                scale: 1.0,
            },
            Prop { id: 2, kind: PropKind::Tree { look: TreeLook { lights: 2, ornaments: 3, topper: 1, garland: 1 } }, x: 0.7, scale: 1.0 },
            Prop {
                id: 3,
                kind: PropKind::Lights { look: LightsLook { colors: 1, bulbs: 3, pattern: 1, wire: 0 }, a: [0.3, 0.25], b: [0.7, 0.25] },
                x: 0.5,
                scale: 1.0,
            },
        ],
        ..Default::default()
    };
    demo.save("DEMO")?;

    let secs = std::env::var("EDITOR_DEMO_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let ids: Vec<u64> = std::env::var("EDITOR_DEMO_IDS")
        .ok()
        .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![1, 2, 3]);
    let mut result = Ok(());
    for id in ids {
        result = result.and_then(|()| snowy_settings::run_editor_demo("DEMO".into(), id, Some(Duration::from_secs(secs))));
    }

    match backup {
        Some(bytes) => std::fs::write(&path, bytes)?,
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
    result
}
