//! Opens one settings page for screenshots, even while a real settings window is open.
//!
//!     cargo run -p snowy-settings --example page_demo -- friends
//!
//! DEMO_SECS (default 8) closes it. Point APPDATA at a folder with fake data to show it.

use snowy_core::MonitorInfo;

fn main() -> anyhow::Result<()> {
    let page = std::env::args().nth(1).unwrap_or_else(|| "friends".into());
    let secs = std::env::var("DEMO_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(8);
    let monitors = vec![MonitorInfo { id: r"\.\DISPLAY1".into(), rect: (0, 0, 1920, 1080), work_rect: (0, 0, 1920, 1040), primary: true, dpi_scale: 1.0 }];
    snowy_settings::run_page_demo(monitors, &page, Some(std::time::Duration::from_secs(secs)))
}
