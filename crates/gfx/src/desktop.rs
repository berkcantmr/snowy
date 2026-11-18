//! Parenting the overlay into the desktop (between wallpaper and icons).

use anyhow::{anyhow, Result};
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::window::move_to;

struct Search {
    container: HWND,
    defview: HWND,
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
    let s = &mut *(lp.0 as *mut Search);
    if let Ok(dv) = FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), None) {
        s.container = hwnd;
        s.defview = dv;
        return false.into();
    }
    true.into()
}

/// Where to parent: the parent window and the z-order anchor inside it.
struct Target {
    parent: HWND,
    insert_after: HWND,
}

fn find_target() -> Result<Target> {
    unsafe {
        let progman = FindWindowW(w!("Progman"), None)?;
        let mut res = 0usize;
        let _ = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0xD),
            LPARAM(0x1),
            SMTO_NORMAL,
            1000,
            Some(&mut res),
        );

        let mut s = Search { container: HWND::default(), defview: HWND::default() };
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut s as *mut Search as isize));
        if s.container.0.is_null() {
            return Err(anyhow!("SHELLDLL_DefView not found"));
        }

        if s.container != progman {
            // Classic layout: WorkerW (behind icons) follows the one holding the DefView.
            let worker = FindWindowExW(None, Some(s.container), w!("WorkerW"), None)?;
            return Ok(Target { parent: worker, insert_after: HWND_TOP });
        }
        // Windows 11 24H2+: WorkerW is a child of Progman, below the DefView.
        FindWindowExW(Some(progman), None, w!("WorkerW"), None)?;
        Ok(Target { parent: progman, insert_after: s.defview })
    }
}

/// Parent `hwnd` behind the desktop icons. Returns the new parent window.
pub fn attach(hwnd: HWND, rect: (i32, i32, i32, i32)) -> Result<HWND> {
    let t = find_target()?;
    unsafe {
        SetParent(hwnd, Some(t.parent))?;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let style = (style & !WS_POPUP.0) | WS_CHILD.0;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        move_to(hwnd, rect, Some(t.parent))?;
        SetWindowPos(
            hwnd,
            Some(t.insert_after),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )?;
    }
    Ok(t.parent)
}

/// Undo `attach`: back to a top-level popup in screen coordinates.
pub fn detach(hwnd: HWND, rect: (i32, i32, i32, i32), topmost: bool) -> Result<()> {
    unsafe {
        let _ = SetParent(hwnd, None);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let style = (style & !WS_CHILD.0) | WS_POPUP.0;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        SetWindowPos(
            hwnd,
            Some(if topmost { HWND_TOPMOST } else { HWND_BOTTOM }),
            rect.0,
            rect.1,
            rect.2.max(1),
            rect.3.max(1),
            SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )?;
    }
    Ok(())
}
