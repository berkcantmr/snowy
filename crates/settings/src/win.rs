//! Small Win32 helpers: single-instance guard and UI language detection.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE};

/// Holds the named mutex for the lifetime of the settings window.
pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Returns None if another settings window already exists (and tries to focus it).
pub fn acquire_single_instance() -> Option<InstanceGuard> {
    unsafe {
        let h = CreateMutexW(None, false, w!("Local\\SnowySettingsWindow")).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(h);
            focus_existing();
            return None;
        }
        Some(InstanceGuard(h))
    }
}

unsafe fn focus_existing() {
    for title in [w!("Snowy Settings"), w!("Snowy Ayarları")] {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), title) {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
            return;
        }
    }
}

/// Windows user UI locale name, e.g. "tr-TR".
pub fn windows_locale() -> Option<String> {
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n <= 1 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..(n as usize - 1)]))
}

/// True when Windows apps use the dark theme (HKCU ...\Personalize\AppsUseLightTheme == 0).
pub fn system_prefers_dark() -> bool {
    // Dev/test override: SNOWY_THEME=dark|light.
    match std::env::var("SNOWY_THEME").ok().as_deref() {
        Some("dark") => return true,
        Some("light") => return false,
        _ => {}
    }
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut std::ffi::c_void),
            Some(&mut size),
        )
    };
    rc.is_ok() && data == 0
}

/// Opens a folder in Explorer (errors are ignored; there is nothing useful to do about them).
pub fn open_folder(path: &std::path::Path) {
    let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
}

/// Single-instance guard for one prop editor ("Local\SnowyEditor-<prop_id>").
/// Returns None if that editor already exists; then the window with one of `titles` is focused.
pub fn acquire_editor_instance(prop_id: u64, titles: &[String]) -> Option<InstanceGuard> {
    use windows::core::HSTRING;
    unsafe {
        let name = HSTRING::from(format!(r"Local\SnowyEditor-{prop_id}"));
        let h = CreateMutexW(None, false, &name).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(h);
            for title in titles {
                let t = HSTRING::from(title.as_str());
                if let Ok(hwnd) = FindWindowW(PCWSTR::null(), &t) {
                    if IsIconic(hwnd).as_bool() {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    }
                    let _ = SetForegroundWindow(hwnd);
                    break;
                }
            }
            return None;
        }
        Some(InstanceGuard(h))
    }
}

/// Work area (screen minus taskbar) of the primary monitor in logical pixels (DIPs):
/// (left, top, width, height). New windows open on that monitor. None if unreadable.
pub fn primary_work_area_logical() -> Option<[f32; 4]> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let hmon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return None;
        }
        let (mut dx, mut dy) = (96u32, 96u32);
        GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).ok()?;
        let scale = dx.max(96) as f32 / 96.0;
        let r = mi.rcWork;
        Some([
            r.left as f32 / scale,
            r.top as f32 / scale,
            (r.right - r.left) as f32 / scale,
            (r.bottom - r.top) as f32 / scale,
        ])
    }
}

/// Height of the window title bar plus frame, in DIPs (Windows 10/11 default theme).
const FRAME_H: f32 = 32.0;

/// Where and how big a window opens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub size: [f32; 2],
    pub min: [f32; 2],
    /// Outer top-left in DIPs, centred in the work area. None = let the system place it.
    pub pos: Option<[f32; 2]>,
}

/// Shrinks a window's `desired` (and `min`) inner size so the whole window, title bar
/// included, fits the work area `work` (DIPs: left, top, width, height), and centres it
/// there rather than on the whole monitor, so it never slides under the taskbar. Content
/// scrolls when the window is shorter than desired.
pub fn fit_window(desired: [f32; 2], min: [f32; 2], work: Option<[f32; 4]>) -> Placement {
    let Some([x, y, w, h]) = work else { return Placement { size: desired, min, pos: None } };
    let (max_w, max_h) = ((w - 16.0).max(320.0), (h - FRAME_H - 16.0).max(300.0));
    let size = [desired[0].min(max_w), desired[1].min(max_h)];
    let pos = [x + ((w - size[0]) / 2.0).max(0.0), y + ((h - size[1] - FRAME_H) / 2.0).max(0.0)];
    Placement { size, min: [min[0].min(size[0]), min[1].min(size[1])], pos: Some(pos) }
}

#[cfg(test)]
mod fit_tests {
    use super::{fit_window, FRAME_H};

    #[test]
    fn fits_common_laptop_scales() {
        // 1920x1080, taskbar 48 px at 100 % (it grows with the scale).
        for (scale, fits_unchanged) in [(1.0, true), (1.25, true), (1.5, false), (1.75, false)] {
            let wh = (1080.0 - 48.0 * scale) / scale;
            let work = [0.0, 0.0, 1920.0 / scale, wh];
            let p = fit_window([520.0, 720.0], [480.0, 560.0], Some(work));
            let pos = p.pos.unwrap();
            assert!(pos[1] >= 0.0 && pos[1] + FRAME_H + p.size[1] <= wh + 0.01, "{scale}: {p:?}");
            assert!(pos[0] >= 0.0 && pos[0] + p.size[0] <= work[2] + 0.01);
            assert!(p.min[0] <= p.size[0] && p.min[1] <= p.size[1]);
            assert_eq!(p.size == [520.0, 720.0], fits_unchanged, "{scale}");
        }
    }

    #[test]
    fn work_area_offset_is_respected() {
        // Taskbar at the top: work area starts lower.
        let p = fit_window([400.0, 300.0], [400.0, 300.0], Some([0.0, 40.0, 1000.0, 700.0]));
        assert_eq!(p.pos, Some([300.0, 40.0 + (700.0 - 300.0 - FRAME_H) / 2.0]));
    }

    #[test]
    fn unknown_work_area_keeps_size() {
        let p = fit_window([560.0, 640.0], [480.0, 520.0], None);
        assert_eq!((p.size, p.min, p.pos), ([560.0, 640.0], [480.0, 520.0], None));
    }
}

/// True while the tray app (the main Snowy process) is running.
pub fn main_instance_running() -> bool {
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    // Same name as snowy_sys::single_instance.
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, w!(r"Local\SnowyMainInstance")) } {
        Ok(h) => {
            unsafe {
                let _ = CloseHandle(h);
            }
            true
        }
        Err(_) => false,
    }
}

struct Guard {
    /// The tray app was running when this window opened.
    had_main: bool,
    last_check: std::time::Instant,
    /// Minimum inner size (DIPs) restored if the window gets squashed.
    min: [f32; 2],
}

thread_local! {
    static GUARD: std::cell::RefCell<Option<Guard>> = const { std::cell::RefCell::new(None) };
}

/// Starts watching for `tick_window_guard`; call before running the window.
pub fn start_window_guard(min: [f32; 2]) {
    let g = Guard { had_main: main_instance_running(), last_check: std::time::Instant::now(), min };
    GUARD.with(|c| *c.borrow_mut() = Some(g));
}

/// Call every frame. About once a second:
/// - closes this window when the tray app it belongs to has quit (so windows opened by a
///   second launch don't outlive the app);
/// - restores a usable size if a display-scale change squashed the window.
pub fn tick_window_guard(ctx: &eframe::egui::Context) {
    use eframe::egui::{vec2, ViewportCommand};
    ctx.request_repaint_after(std::time::Duration::from_secs(1));
    GUARD.with(|c| {
        let mut c = c.borrow_mut();
        let Some(g) = c.as_mut() else { return };
        if g.last_check.elapsed() < std::time::Duration::from_secs(1) {
            return;
        }
        g.last_check = std::time::Instant::now();
        if g.had_main && !main_instance_running() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        let inner = ctx.input(|i| i.viewport().inner_rect);
        if let Some(r) = inner {
            if r.width() < g.min[0] * 0.6 || r.height() < g.min[1] * 0.6 {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(g.min[0], g.min[1])));
            }
        }
    });
}
