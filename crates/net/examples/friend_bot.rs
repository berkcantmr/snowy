//! A test friend for trying the friends feature on one computer.
//!
//!     cargo run -p snowy-net --example friend_bot
//!
//! It prints its friend code, accepts every friend request, and once someone is its friend
//! and online, throws a snowball every few seconds and sends one gift. Its key lives in
//! %TEMP%\snowy-friend-bot.key, so it keeps the same code between runs. Ctrl+C to stop.

use snowy_net::state::{Action, Command};
use snowy_net::{Hub, Status};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let key = snowy_net::identity::load_or_create(&std::env::temp_dir().join("snowy-friend-bot.key"))?;
    let hub = Hub::start(key, snowy_net::hub_url(), "bot".into());
    let mut announced = false;
    let mut gifted = false;
    let mut last_throw = Instant::now() - Duration::from_secs(60);
    loop {
        let st = hub.state();
        if st.status == Status::Online && !announced {
            println!("friend bot online. Its code: {}", st.code);
            println!("Add this code in Snowy > Settings > Friends.");
            announced = true;
        }
        for code in &st.requests {
            println!("accepting {code}");
            hub.send(Command::Accept { code: code.clone() });
        }
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
        for (from, a) in hub.take_incoming() {
            println!("{from} did {a:?} to the bot");
        }
        if let Some(e) = &st.last_error {
            if e != "slow_down" {
                println!("error: {e}");
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
