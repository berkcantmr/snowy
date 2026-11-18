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
