use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY, WM_QUIT,
};

/// Process all pending Win32 messages for this thread without blocking.
/// Returns false if WM_QUIT was received.
pub fn pump_messages() -> bool {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            if msg.message == WM_HOTKEY {
                crate::hotkey::on_hotkey(msg.wParam.0 as i32);
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

/// Block until the next desktop composition (vblank). Returns false if DWM is unavailable,
/// in which case the caller should fall back to sleeping.
pub fn wait_vblank() -> bool {
    unsafe { windows::Win32::Graphics::Dwm::DwmFlush().is_ok() }
}

/// Asks every top-level window of process `pid` to close (posts `WM_CLOSE`), so a child
/// window such as the settings or look editor shuts down like the user clicked its X.
pub fn close_process_windows(pid: u32) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE};

    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let mut owner = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
            if owner == lp.0 as u32 {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        true.into()
    }
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(pid as isize));
    }
}
