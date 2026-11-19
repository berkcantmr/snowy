//! Turning the desktop icons on and off, like right-click > View > Show desktop icons.
//!
//! Edit mode hides them so decorations can be arranged freely, and shows them again
//! afterwards (only if they were showing before).

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, FindWindowExW, FindWindowW, IsWindowVisible, SendMessageW, WM_COMMAND};
use windows::core::BOOL;

/// The shell's "toggle desktop icons" command (the same one the View menu sends).
const TOGGLE_ICONS: usize = 0x7402;

/// The desktop's shell view window: under Progman, or under a WorkerW when a wallpaper
/// animation / slideshow has moved it there.
fn defview() -> Option<HWND> {
    unsafe {
        if let Ok(progman) = FindWindowW(w!("Progman"), None) {
            if let Ok(v) = FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), None) {
                return Some(v);
            }
        }
        unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
            if let Ok(v) = FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), None) {
                *(lp.0 as *mut Option<HWND>) = Some(v);
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut found: Option<HWND> = None;
        let _ = EnumWindows(Some(cb), LPARAM(&mut found as *mut _ as isize));
        found
    }
}

/// Whether the desktop icons are showing; None if the desktop can't be found.
pub fn desktop_icons_visible() -> Option<bool> {
    let view = defview()?;
    unsafe {
        let list = FindWindowExW(Some(view), None, w!("SysListView32"), None).ok()?;
        Some(IsWindowVisible(list).as_bool())
    }
}

/// Shows or hides the desktop icons. Returns true when they are now as asked.
pub fn set_desktop_icons_visible(visible: bool) -> bool {
    match desktop_icons_visible() {
        Some(v) if v == visible => true,
        Some(_) => {
            let Some(view) = defview() else { return false };
            unsafe {
                SendMessageW(view, WM_COMMAND, Some(WPARAM(TOGGLE_ICONS)), Some(LPARAM(0)));
            }
            desktop_icons_visible() == Some(visible)
        }
        None => false,
    }
}
