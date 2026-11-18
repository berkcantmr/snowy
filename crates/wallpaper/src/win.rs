//! Setting and reading the desktop wallpaper (IDesktopWallpaper, with a SystemParametersInfo fallback).

use std::path::{Path, PathBuf};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper, DWPOS_FILL};
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_GETDESKWALLPAPER, SPI_SETDESKWALLPAPER,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

fn desktop_wallpaper() -> Option<IDesktopWallpaper> {
    unsafe {
        // Fine if COM is already initialized on this thread (S_FALSE / RPC_E_CHANGED_MODE).
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        CoCreateInstance::<_, IDesktopWallpaper>(&DesktopWallpaper, None, CLSCTX_ALL).ok()
    }
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// Sets `path` as the wallpaper on all monitors, scaled to fill.
pub fn apply(path: &Path) -> std::io::Result<()> {
    if let Some(dw) = desktop_wallpaper() {
        let w = wide(path);
        let ok = unsafe { dw.SetPosition(DWPOS_FILL).is_ok() && dw.SetWallpaper(PCWSTR::null(), PCWSTR(w.as_ptr())).is_ok() };
        if ok {
            return Ok(());
        }
    }
    let mut w = wide(path);
    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(w.as_mut_ptr() as *mut _),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

/// The wallpaper path currently shown on the primary monitor (None if a solid color / unknown).
pub fn current_wallpaper() -> Option<PathBuf> {
    if let Some(dw) = desktop_wallpaper() {
        unsafe {
            // First monitor first, then the "all monitors" query.
            let mut candidates: Vec<PCWSTR> = Vec::new();
            let mut keep: Vec<PWSTR> = Vec::new();
            if let Ok(id) = dw.GetMonitorDevicePathAt(0) {
                candidates.push(PCWSTR(id.0));
                keep.push(id);
            }
            candidates.push(PCWSTR::null());
            for mon in candidates {
                if let Ok(p) = dw.GetWallpaper(mon) {
                    let s = p.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(p.0 as *const _));
                    if !s.is_empty() {
                        for k in keep {
                            CoTaskMemFree(Some(k.0 as *const _));
                        }
                        return Some(PathBuf::from(s));
                    }
                }
            }
            for k in keep {
                CoTaskMemFree(Some(k.0 as *const _));
            }
        }
    }
    let mut buf = [0u16; 520];
    unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buf.len() as u32,
            Some(buf.as_mut_ptr() as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    if n == 0 {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// Local wall-clock time as fractional hours (0..24).
pub fn local_hour() -> f32 {
    let t = unsafe { GetLocalTime() };
    t.wHour as f32 + t.wMinute as f32 / 60.0 + t.wSecond as f32 / 3600.0
}
