//! Shell pop-ups (the taskbar's right-click menu, context menus, the tray overflow, taskbar
//! thumbnails, Start and search) that must stay above Snowy's always-on-top overlay. Many of
//! them aren't topmost windows themselves, so while one is open the overlay steps down below
//! it, and goes back on top when it closes.

use windows::core::BOOL;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetTopWindow, GetWindow, IsWindowVisible, GW_HWNDNEXT};

/// Pop-up window classes that are always worth staying under while visible.
const POPUPS: &[&str] = &[
    // Windows 11 taskbar and shell context menus.
    "Xaml_WindowedPopupClass",
    // Classic context menus.
    "#32768",
    // Tray overflow (Windows 11 / Windows 10).
    "TopLevelWindowForOverflowXamlIsland",
    "NotifyIconOverflowWindow",
    // Taskbar thumbnail previews.
    "TaskListThumbnailWnd",
    // Input language switcher.
    "Shell_InputSwitchTopLevelWindow",
];

/// Shell surfaces that only count while they have the focus (Start, search, notification
/// center, quick settings: their windows exist all the time, hidden).
const FOCUSED_SHELL: &[&str] = &["Windows.UI.Core.CoreWindow"];

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn cloaked(hwnd: HWND) -> bool {
    let mut v: u32 = 0;
    let ok = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut v as *mut u32 as *mut _, std::mem::size_of::<u32>() as u32) };
    ok.is_ok() && v != 0
}

/// The highest shell pop-up that is open right now (as a raw window handle), if any.
pub fn open_shell_popup() -> Option<isize> {
    unsafe {
        let fg = GetForegroundWindow();
        let mut h = GetTopWindow(None).ok()?;
        for _ in 0..4000 {
            if IsWindowVisible(h) != BOOL(0) {
                let class = class_of(h);
                let wanted = POPUPS.contains(&class.as_str()) || (FOCUSED_SHELL.contains(&class.as_str()) && h == fg);
                if wanted && !cloaked(h) {
                    return Some(h.0 as isize);
                }
            }
            match GetWindow(h, GW_HWNDNEXT) {
                Ok(next) if !next.is_invalid() => h = next,
                _ => break,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn runs_without_crashing() {
        // Whatever is open on the machine running the tests; just must not panic.
        let _ = super::open_shell_popup();
    }
}
