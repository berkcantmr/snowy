//! Mouse grab: a low-level mouse hook on its own thread that swallows primary-button
//! presses inside "grab zones" on the bare desktop, so playing with snow doesn't click the
//! desktop or the apps behind it.
//!
//! Design: a dedicated thread installs `WH_MOUSE_LL` and pumps messages, so hook latency
//! never depends on the app's render loop. State is shared through statics: the zone list
//! (Mutex, `try_lock` in the hook; contention counts as "no zone hit") and atomics.
//!
//! Stuck-capture safety: if a capture lasts longer than `MAX_CAPTURE_MS` (the matching up
//! was lost), the next mouse event releases it and passes through.
//!
//! Limitation: LL hooks don't see input to elevated windows when we aren't elevated.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetClassNameW, GetMessageW, GetSystemMetrics,
    GetWindowThreadProcessId, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WindowFromPoint, GA_ROOT, HHOOK, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT,
    PM_NOREMOVE, SM_SWAPBUTTON, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP,
};

/// Rect in virtual-screen physical pixels: left, top, right, bottom (right/bottom exclusive).
pub type ScreenRect = (i32, i32, i32, i32);

/// A capture older than this is considered stuck and is released.
const MAX_CAPTURE_MS: u64 = 10_000;

static ZONES: Mutex<Vec<ScreenRect>> = Mutex::new(Vec::new());
static CAPTURED: AtomicBool = AtomicBool::new(false);
static CAPTURE_START_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ev {
    PrimaryDown,
    PrimaryUp,
    Other,
}

fn in_zones(pt: (i32, i32), zones: &[ScreenRect]) -> bool {
    zones
        .iter()
        .any(|&(l, t, r, b)| pt.0 >= l && pt.0 < r && pt.1 >= t && pt.1 < b)
}

/// Pure decision: returns (swallow, new_captured). `desktop_at_pt` is only evaluated
/// when needed (primary down, inside a zone, not captured). `expired`: capture is stuck.
fn decide(
    ev: Ev,
    pt: (i32, i32),
    zones: &[ScreenRect],
    captured: bool,
    expired: bool,
    injected: bool,
    desktop_at_pt: impl FnOnce() -> bool,
) -> (bool, bool) {
    if injected {
        return (false, captured);
    }
    if captured && expired {
        // Lost the matching up: release and let this event through.
        return (false, false);
    }
    match ev {
        Ev::PrimaryDown => {
            if !captured && in_zones(pt, zones) && desktop_at_pt() {
                (true, true)
            } else {
                (false, captured)
            }
        }
        Ev::PrimaryUp => {
            if captured {
                (true, false)
            } else {
                (false, false)
            }
        }
        Ev::Other => (false, captured),
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn desktop_at(pt: (i32, i32)) -> bool {
    unsafe {
        let hwnd = WindowFromPoint(POINT { x: pt.0, y: pt.1 });
        if hwnd.0.is_null() {
            return false;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let root = if root.0.is_null() { hwnd } else { root };
        let class = class_name(root);
        if class == "Progman" || class == "WorkerW" {
            return true;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(root, Some(&mut pid));
        pid == GetCurrentProcessId()
    }
}

fn handle(wparam: WPARAM, info: &MSLLHOOKSTRUCT) -> bool {
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let (down, up) = if swapped {
        (WM_RBUTTONDOWN, WM_RBUTTONUP)
    } else {
        (WM_LBUTTONDOWN, WM_LBUTTONUP)
    };
    let msg = wparam.0 as u32;
    let ev = if msg == down {
        Ev::PrimaryDown
    } else if msg == up {
        Ev::PrimaryUp
    } else {
        Ev::Other
    };
    let captured = CAPTURED.load(Ordering::Acquire);
    let now = unsafe { GetTickCount64() };
    let expired =
        captured && now.saturating_sub(CAPTURE_START_MS.load(Ordering::Acquire)) > MAX_CAPTURE_MS;
    let pt = (info.pt.x, info.pt.y);
    let injected = info.flags & LLMHF_INJECTED != 0;
    let (swallow, new_cap) = match ZONES.try_lock() {
        Ok(zones) => decide(ev, pt, &zones, captured, expired, injected, || desktop_at(pt)),
        // Contended: treat as no zone hit (but still honour a pending release).
        Err(_) => decide(ev, pt, &[], captured, expired, injected, || false),
    };
    if new_cap != captured {
        if new_cap {
            CAPTURE_START_MS.store(now, Ordering::Release);
        }
        CAPTURED.store(new_cap, Ordering::Release);
    }
    swallow
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam.0 != 0 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let swallow = std::panic::catch_unwind(|| handle(wparam, info)).unwrap_or(false);
        if swallow {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub struct MouseGrab {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl MouseGrab {
    /// Start the hook thread. Errors if the hook can't be installed.
    pub fn start() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("snowy-mouse-grab".into())
            .spawn(move || unsafe {
                let hmod = GetModuleHandleW(None).ok();
                let hook: HHOOK = match SetWindowsHookExW(
                    WH_MOUSE_LL,
                    Some(hook_proc),
                    hmod.map(|m| windows::Win32::Foundation::HINSTANCE(m.0)),
                    0,
                ) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                        return;
                    }
                };
                // Force creation of this thread's message queue before announcing the id.
                let mut msg = MSG::default();
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                let _ = tx.send(Ok(GetCurrentThreadId()));
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
                let _ = UnhookWindowsHookEx(hook);
            })?;
        match rx.recv() {
            Ok(Ok(thread_id)) => Ok(Self { thread_id, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                anyhow::bail!("SetWindowsHookEx(WH_MOUSE_LL) failed: {e}")
            }
            Err(_) => anyhow::bail!("mouse grab thread died during startup"),
        }
    }

    /// Replace the current grab zones (called every frame by the app; cheap).
    pub fn set_zones(&self, zones: &[ScreenRect]) {
        if let Ok(mut z) = ZONES.lock() {
            if z.as_slice() != zones {
                z.clear();
                z.extend_from_slice(zones);
            }
        }
    }

    /// True between a swallowed primary-button down and the matching up.
    pub fn captured(&self) -> bool {
        CAPTURED.load(Ordering::Acquire)
    }
}

impl Drop for MouseGrab {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        CAPTURED.store(false, Ordering::Release);
        if let Ok(mut z) = ZONES.lock() {
            z.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: &[ScreenRect] = &[(100, 100, 200, 200)];

    fn d(ev: Ev, pt: (i32, i32), cap: bool, inj: bool, desk: bool) -> (bool, bool) {
        decide(ev, pt, Z, cap, false, inj, || desk)
    }

    #[test]
    fn down_in_zone_on_desktop_captures() {
        assert_eq!(d(Ev::PrimaryDown, (150, 150), false, false, true), (true, true));
    }

    #[test]
    fn down_in_zone_over_app_passes() {
        assert_eq!(d(Ev::PrimaryDown, (150, 150), false, false, false), (false, false));
    }

    #[test]
    fn down_outside_zone_passes() {
        assert_eq!(d(Ev::PrimaryDown, (200, 150), false, false, true), (false, false));
        assert_eq!(d(Ev::PrimaryDown, (99, 150), false, false, true), (false, false));
    }

    #[test]
    fn up_while_captured_swallows_and_releases() {
        assert_eq!(d(Ev::PrimaryUp, (0, 0), true, false, false), (true, false));
    }

    #[test]
    fn up_while_not_captured_passes() {
        assert_eq!(d(Ev::PrimaryUp, (150, 150), false, false, true), (false, false));
    }

    #[test]
    fn injected_passes() {
        assert_eq!(d(Ev::PrimaryDown, (150, 150), false, true, true), (false, false));
        assert_eq!(d(Ev::PrimaryUp, (150, 150), true, true, true), (false, true));
    }

    #[test]
    fn other_events_always_pass() {
        assert_eq!(d(Ev::Other, (150, 150), false, false, true), (false, false));
        assert_eq!(d(Ev::Other, (150, 150), true, false, true), (false, true));
    }

    #[test]
    fn expired_capture_releases_and_passes() {
        assert_eq!(
            decide(Ev::Other, (0, 0), Z, true, true, false, || true),
            (false, false)
        );
        assert_eq!(
            decide(Ev::PrimaryUp, (0, 0), Z, true, true, false, || true),
            (false, false)
        );
    }

    #[test]
    fn desktop_check_is_lazy() {
        decide(Ev::PrimaryDown, (0, 0), Z, false, false, false, || {
            panic!("should not be evaluated outside zones")
        });
    }

    #[test]
    fn live_start_zones_drop() {
        let g = MouseGrab::start().expect("hook installs");
        g.set_zones(&[(-30000, -30000, -29990, -29990)]);
        assert!(!g.captured());
        g.set_zones(&[]);
        drop(g);
    }
}
