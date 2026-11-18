//! Manual check: apply one generated wallpaper for a few seconds, then restore the original.
//! Usage: apply_check <screenshot.png-path-ignored>  (screenshot is taken by the caller during the pause)

use snowy_wallpaper::{apply, current_wallpaper, ensure_cached, Theme, TimeOfDay};
use std::time::Duration;

fn main() {
    let original = current_wallpaper();
    println!("original: {original:?}");
    let path = ensure_cached(Theme::StarryVillage, TimeOfDay::Dusk, 1920, 1080).expect("cache");
    apply(&path).expect("apply");
    println!("applied: {path:?}; now: {:?}", current_wallpaper());
    println!("READY");
    for i in 0..4 {
        std::thread::sleep(Duration::from_millis(500));
        println!("t={:.1}s current={:?}", (i + 1) as f32 * 0.5, current_wallpaper());
    }
    if let Some(o) = &original {
        apply(o).expect("restore");
    }
    // The shell applies wallpapers asynchronously; give it a moment before reading back.
    std::thread::sleep(Duration::from_millis(1500));
    let after = current_wallpaper();
    println!("restored: {after:?}; equal = {}", after == original);
}
