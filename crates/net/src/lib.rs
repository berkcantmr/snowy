//! Snowy's network features besides updates:
//!
//! - **Friends** (off until turned on): an identity key, a connection to the friends server,
//!   and the files the app and the settings window share (`state`).
//! - **Anonymous usage count** (on by default, can be turned off): at most once a day, the
//!   app version is sent to the server, which only adds it to that day's totals.

pub mod client;
pub mod identity;
pub mod state;
pub mod usage;

pub use client::Hub;
pub use state::{Command, Friend, FriendsState, Status};

/// The friends server. `SNOWY_HUB` overrides it (e.g. `ws://127.0.0.1:8790/ws` for testing).
pub const HUB_URL: &str = "wss://snowy.berkcantumer.dev/ws";
/// Where the daily usage ping goes.
pub const PING_URL: &str = "https://snowy.berkcantumer.dev/ping";

pub fn hub_url() -> String {
    std::env::var("SNOWY_HUB").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| HUB_URL.to_string())
}
