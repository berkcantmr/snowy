//! A process-wide "please quit" flag. Window procedures set it when Windows (or an installer
//! through the Restart Manager) asks Snowy to close; the main loop checks it every frame and
//! exits cleanly, so no window is destroyed while the app keeps running without it.

use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Ask the app to quit.
pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

/// True once something asked the app to quit.
pub fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}
