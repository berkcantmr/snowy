//! Win32 window class and overlay window creation.

use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS_NAME: PCWSTR = w!("SnowyOverlayWindow");
const HTTRANSPARENT_RESULT: isize = -1;

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT_RESULT),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn register_class() -> Result<()> {
    static REGISTERED: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| unsafe {
            let hinst = GetModuleHandleW(None).map_err(|e| e.to_string())?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinst.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                return Err(windows::core::Error::from_win32().to_string());
            }
            Ok(())
        })
        .clone()
        .map_err(|e| anyhow!("RegisterClassW failed: {e}"))
}

/// Create the (initially hidden) overlay window at `rect` (x, y, w, h).
pub fn create(rect: (i32, i32, i32, i32), topmost: bool) -> Result<HWND> {
    register_class()?;
    let mut ex = WS_EX_NOREDIRECTIONBITMAP
        | WS_EX_TRANSPARENT
        | WS_EX_LAYERED
        | WS_EX_NOACTIVATE
        | WS_EX_TOOLWINDOW;
    if topmost {
        ex |= WS_EX_TOPMOST;
    }
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let hwnd = CreateWindowExW(
            ex,
            CLASS_NAME,
            w!("Snowy Overlay"),
            WS_POPUP,
            rect.0,
            rect.1,
            rect.2.max(1),
            rect.3.max(1),
            None,
            None,
            Some(hinst.into()),
            None,
        )?;
        if let Err(e) = SetLayeredWindowAttributes(hwnd, Default::default(), 255, LWA_ALPHA) {
            let _ = DestroyWindow(hwnd);
            return Err(e.into());
        }
        Ok(hwnd)
    }
}

/// Move/resize to a screen-coordinate `rect`; if `parent` is set the position is
/// converted to the parent's client coordinates.
pub fn move_to(hwnd: HWND, rect: (i32, i32, i32, i32), parent: Option<HWND>) -> Result<()> {
    let mut p = POINT { x: rect.0, y: rect.1 };
    if let Some(parent) = parent {
        unsafe {
            let _ = ScreenToClient(parent, &mut p);
        }
    }
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            p.x,
            p.y,
            rect.2.max(1),
            rect.3.max(1),
            SWP_NOACTIVATE | SWP_NOZORDER,
        )?;
    }
    Ok(())
}
