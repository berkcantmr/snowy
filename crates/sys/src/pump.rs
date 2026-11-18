use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
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
