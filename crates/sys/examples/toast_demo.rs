use snowy_sys::{pump_messages, Key, Toast, ToastIcon};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    snowy_sys::init_dpi_awareness();
    let mut toast = Toast::new()?;
    let text = "Bu nesne masaüstü simgelerine yakın. Taşımak için düzenleme modunu aç.";
    let keys = [Key::Win, Key::Alt, Key::Char('S')];
    for icon in [ToastIcon::Lock, ToastIcon::EditOn, ToastIcon::EditOff] {
        toast.show(icon, text, &keys, None);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(4) {
            if !pump_messages() {
                return Ok(());
            }
            toast.tick();
            std::thread::sleep(Duration::from_millis(16));
        }
    }
    Ok(())
}
