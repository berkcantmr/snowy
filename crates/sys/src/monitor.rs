use snowy_core::MonitorInfo;
use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    MDT_EFFECTIVE_DPI,
};

/// Enable per-monitor-v2 DPI awareness. Call first thing in main.
pub fn init_dpi_awareness() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn rect_xywh(r: &RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right - r.left, r.bottom - r.top)
}

unsafe extern "system" fn enum_proc(hmon: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam.0 as *mut Vec<MonitorInfo>);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(hmon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
        let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
        let id = String::from_utf16_lossy(&info.szDevice[..len]);
        let (mut dx, mut dy) = (96u32, 96u32);
        let dpi_scale = match GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) {
            Ok(()) if dx > 0 => dx as f32 / 96.0,
            _ => 1.0,
        };
        out.push(MonitorInfo {
            id,
            rect: rect_xywh(&info.monitorInfo.rcMonitor),
            work_rect: rect_xywh(&info.monitorInfo.rcWork),
            primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
            dpi_scale,
        });
    }
    true.into()
}

/// All monitors, primary first, then by (x, y, id). Deterministic.
pub fn monitors() -> Vec<MonitorInfo> {
    let mut out: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(enum_proc), LPARAM(&mut out as *mut _ as isize));
    }
    out.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then(a.rect.0.cmp(&b.rect.0))
            .then(a.rect.1.cmp(&b.rect.1))
            .then(a.id.cmp(&b.id))
    });
    out
}

/// Windows UI language as a lowercase locale name like "tr-tr" or "zh-cn" (region kept so
/// Traditional Chinese can be told apart).
pub fn ui_language() -> String {
    let mut buf = [0u16; 85];
    let n = unsafe {
        let lang = GetUserDefaultUILanguage();
        LCIDToLocaleName(lang as u32, Some(&mut buf), 0)
    };
    if n > 0 {
        let s = String::from_utf16_lossy(&buf[..(n as usize - 1).min(buf.len())]);
        let code = s.to_ascii_lowercase();
        if !code.is_empty() {
            return code;
        }
    }
    "en".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitors_sane() {
        let a = monitors();
        assert!(!a.is_empty());
        assert_eq!(a.iter().filter(|m| m.primary).count(), 1);
        assert!(a[0].primary);
        assert_eq!(a, monitors());
        assert!(a.iter().all(|m| m.dpi_scale >= 1.0 && m.rect.2 > 0));
    }

    #[test]
    fn language_lowercase() {
        let l = ui_language();
        assert!(!l.is_empty());
        assert_eq!(l, l.to_ascii_lowercase());
    }
}
