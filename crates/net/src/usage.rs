//! The anonymous daily usage ping: `POST {"ver":"0.1.6"}`, at most once per day. The only
//! thing kept locally is the day of the last ping (`usage.json`); nothing identifies the
//! computer, and the server only adds one to that day's total.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Usage {
    /// Days since 1970 (UTC) of the last successful ping.
    last_day: u64,
}

fn today() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}

/// True when today's ping hasn't been sent yet.
pub fn due(dir: &Path) -> bool {
    let u: Usage = std::fs::read(dir.join("usage.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    u.last_day != today()
}

/// Sends today's ping (blocking; call from a background thread) and remembers the day.
pub fn send(dir: &Path, url: &str, version: &str) -> anyhow::Result<()> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(&format!("Snowy/{version}"))
        .https_only(url.starts_with("https://"))
        .build();
    agent.post(url).send_json(serde_json::json!({ "ver": version }))?;
    let _ = std::fs::create_dir_all(dir);
    std::fs::write(dir.join("usage.json"), serde_json::to_vec(&Usage { last_day: today() })?)?;
    Ok(())
}
