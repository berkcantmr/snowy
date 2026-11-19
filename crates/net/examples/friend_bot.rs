//! A test friend for trying the friends feature on one computer.
//!
//!     cargo run -p snowy-net --example friend_bot            (snowballs and a gift)
//!     cargo run -p snowy-net --example friend_bot -- --share (shared desktop)
//!
//! It prints its friend code and accepts every friend request. By default, once someone is
//! its friend and online, it throws a snowball every few seconds and sends one gift. With
//! `--share` it instead accepts a shared desktop invite, puts its own snowman and tree in it
//! and walks the snowman back and forth. Its key lives in %TEMP%\snowy-friend-bot.key, so
//! it keeps the same code between runs. Ctrl+C to stop.

use snowy_core::props::{Prop, PropKind, SnowmanLook, TreeLook};
use snowy_net::state::{Action, Command, SceneMsg};
use snowy_net::{Hub, Status};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let share_mode = std::env::args().any(|a| a == "--share");
    let key = snowy_net::identity::load_or_create(&std::env::temp_dir().join("snowy-friend-bot.key"))?;
    let hub = Hub::start(key, snowy_net::hub_url(), "bot".into());
    let mut announced = false;
    let mut gifted = false;
    let mut last_throw = Instant::now() - Duration::from_secs(60);
    // Shared desktop: who with, and the bot's own snowman.
    let mut partner: Option<String> = None;
    let mut snowman = Prop { id: 0xB07_5A0E, kind: PropKind::Snowman { look: SnowmanLook { hat: 5, face: 2, item: 3, outfit: 4 }, balls: 3 }, x: 0.35, scale: 1.1 };
    let tree = Prop { id: 0xB07_72EE, kind: PropKind::Tree { look: TreeLook { lights: 5, ornaments: 6, topper: 4, garland: 2 } }, x: 0.6, scale: 1.0 };
    let mut last_walk = Instant::now();
    let mut dir = 1.0f32;
    let mut last_error: Option<String> = None;
    // Each request is accepted once.
    let mut accepted = std::collections::HashSet::new();
    loop {
        let st = hub.state();
        if st.status == Status::Online && !announced {
            println!("friend bot online ({}). Its code: {}", if share_mode { "shared desktop mode" } else { "snowball mode" }, st.code);
            println!("Add this code in Snowy > Settings > Friends.");
            announced = true;
        }
        for code in st.requests.iter().filter(|c| accepted.insert((*c).clone())) {
            println!("accepting {code}");
            hub.send(Command::Accept { code: code.clone() });
        }
        if !share_mode {
            for f in st.friends.iter().filter(|f| f.online) {
                if last_throw.elapsed() >= Duration::from_secs(4) {
                    let x = 0.15 + 0.7 * ((last_throw.elapsed().as_millis() % 1000) as f32 / 1000.0);
                    println!("snowball -> {}", f.code);
                    hub.send(Command::Send { to: f.code.clone(), action: Action::Snowball { x } });
                    last_throw = Instant::now();
                }
                if !gifted {
                    println!("gift -> {}", f.code);
                    hub.send(Command::Send { to: f.code.clone(), action: Action::Gift });
                    gifted = true;
                }
            }
        }
        for (from, a) in hub.take_incoming() {
            println!("{from} did {a:?} to the bot");
        }
        for (from, msg) in hub.take_scene() {
            match msg {
                SceneMsg::Invite if share_mode => {
                    println!("{from} invited the bot to share desktops: accepting");
                    hub.send(Command::Scene { to: from.clone(), msg: SceneMsg::Accept });
                    for p in [snowman, tree] {
                        hub.send(Command::Scene { to: from.clone(), msg: SceneMsg::Upsert { prop: p } });
                    }
                    partner = Some(from);
                }
                SceneMsg::Invite => {
                    hub.send(Command::Scene { to: from, msg: SceneMsg::Decline });
                }
                SceneMsg::Stop => {
                    println!("{from} stopped sharing");
                    partner = None;
                }
                SceneMsg::Upsert { prop } => {
                    if prop.id == snowman.id {
                        snowman = prop;
                    }
                    println!("{from} changed a decoration: id {:x} at x={:.2}", prop.id, prop.x);
                }
                other => println!("{from}: {other:?}"),
            }
        }
        // Walk the bot's snowman back and forth on the shared desktop.
        if let Some(p) = &partner {
            if last_walk.elapsed() >= Duration::from_secs(3) {
                snowman.x += dir * 0.05;
                if snowman.x > 0.8 || snowman.x < 0.2 {
                    dir = -dir;
                }
                hub.send(Command::Scene { to: p.clone(), msg: SceneMsg::Upsert { prop: snowman } });
                last_walk = Instant::now();
            }
        }
        if st.last_error != last_error {
            if let Some(e) = st.last_error.as_deref().filter(|e| *e != "slow_down") {
                println!("error: {e}");
            }
            last_error = st.last_error.clone();
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
