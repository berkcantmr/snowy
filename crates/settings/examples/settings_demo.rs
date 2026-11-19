use snowy_core::MonitorInfo;

fn main() -> anyhow::Result<()> {
    let monitors = vec![
        MonitorInfo {
            id: r"\\.\DISPLAY1".into(),
            rect: (0, 0, 2560, 1440),
            work_rect: (0, 0, 2560, 1400),
            primary: true,
            dpi_scale: 1.0,
        },
        MonitorInfo {
            id: r"\\.\DISPLAY2".into(),
            rect: (2560, 0, 1920, 1080),
            work_rect: (2560, 0, 1920, 1040),
            primary: false,
            dpi_scale: 1.0,
        },
    ];
    // Cycles through all pages and closes afterwards. DEMO_PAGE_SECS (default 1) sets the
    // time per page, e.g. for screenshots.
    let per: u64 = std::env::var("DEMO_PAGE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    snowy_settings::run_demo(
        monitors,
        Some(std::time::Duration::from_secs(per * 7)),
        Some(std::time::Duration::from_secs(per)),
    )
}
