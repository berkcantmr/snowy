use snowy_core::EffectChoice;
use snowy_sys::{pump_messages, Tray, TrayCommand, TrayState};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    snowy_sys::init_dpi_awareness();
    let mut state = TrayState { effect: EffectChoice::Season, paused: false, lang: "en".into() };
    let mut tray = Tray::new(&state)?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if !pump_messages() {
            break;
        }
        while let Some(cmd) = tray.poll() {
            println!("{cmd:?}");
            match cmd {
                TrayCommand::SetEffect(c) => state.effect = c,
                TrayCommand::TogglePause => state.paused = !state.paused,
                TrayCommand::Quit => return Ok(()),
                _ => {}
            }
            tray.update(&state);
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}
