//! Opens the "What's new" window and closes it after DEMO_SECS seconds (default 6).
//! SNOWY_THEME=dark|light and SNOWY_LANG=tr|en|... pick the look and language.

fn main() -> anyhow::Result<()> {
    let secs = std::env::var("DEMO_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
    snowy_settings::run_whats_new_demo(Some(std::time::Duration::from_secs(secs)))
}
