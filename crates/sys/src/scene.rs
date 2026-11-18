//! Desktop scene sampling for interactive effects: window ledges and pointer state.

use std::cell::Cell;

use snowy_core::{MonitorInfo, Surface};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetCursorPos, GetSystemMetrics, GetWindowLongW,
    GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, WindowFromPoint,
    GA_ROOT, GWL_EXSTYLE, SM_SWAPBUTTON, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

/// Minimum ledge length in px.
const MIN_LEN: f32 = 24.0;
/// Minimum local y (room above the ledge).
const MIN_Y: i32 = 24;
const MAX_PER_MONITOR: usize = 64;

/// Integer rect in virtual-screen pixels (right/bottom exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

/// Subtract `cuts` (x-ranges, any order, may overlap) from `[a, b)`.
/// Returns sorted, disjoint, non-empty remaining ranges.
pub(crate) fn subtract_ranges(a: i32, b: i32, cuts: &[(i32, i32)]) -> Vec<(i32, i32)> {
    if a >= b {
        return Vec::new();
    }
    let mut cuts: Vec<(i32, i32)> = cuts.iter().copied().filter(|c| c.0 < c.1).collect();
    cuts.sort_unstable();
    let mut out = Vec::new();
    let mut cur = a;
    for (cl, cr) in cuts {
        if cr <= cur {
            continue;
        }
        if cl >= b {
            break;
        }
        if cl > cur {
            out.push((cur, cl));
        }
        cur = cur.max(cr);
        if cur >= b {
            break;
        }
    }
    if cur < b {
        out.push((cur, b));
    }
    out
}

/// Pure ledge computation. `windows` are in z-order (topmost first); each is
/// `(id, visible rect, is_taskbar)`. Returns one list per monitor, sorted by y.
///
/// Rule: a window's top edge is `[l, r)` at `y = t`. A previously seen (higher) window
/// hides the part of the edge where the pixel row `t - 1` lies inside that window's rect.
pub(crate) fn ledges(windows: &[(u64, Rect, bool)], monitors: &[MonitorInfo]) -> Vec<Vec<Surface>> {
    let mut out: Vec<Vec<Surface>> = vec![Vec::new(); monitors.len()];
    let mut occluders: Vec<Rect> = Vec::with_capacity(windows.len());
    let mut cuts: Vec<(i32, i32)> = Vec::new();
    for &(id, rc, _taskbar) in windows {
        if rc.r <= rc.l || rc.b <= rc.t {
            continue;
        }
        cuts.clear();
        let row = rc.t - 1;
        cuts.extend(occluders.iter().filter(|o| o.t <= row && row < o.b).map(|o| (o.l, o.r)));
        let segs = subtract_ranges(rc.l, rc.r, &cuts);
        occluders.push(rc);
        for (s0, s1) in segs {
            for (mi, m) in monitors.iter().enumerate() {
                let (mx, my, mw, mh) = m.rect;
                let ly = rc.t - my;
                if ly < MIN_Y || ly > mh || out[mi].len() >= MAX_PER_MONITOR {
                    continue;
                }
                let x0 = s0.max(mx);
                let x1 = s1.min(mx + mw);
                if ((x1 - x0) as f32) < MIN_LEN {
                    continue;
                }
                out[mi].push(Surface {
                    id,
                    x0: (x0 - mx) as f32,
                    x1: (x1 - mx) as f32,
                    y: ly as f32,
                });
            }
        }
    }
    for v in &mut out {
        v.sort_by(|a, b| a.y.total_cmp(&b.y));
    }
    out
}

struct EnumCtx {
    pid: u32,
    list: Vec<(u64, Rect, bool)>,
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..(n.max(0) as usize).min(buf.len())])
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut EnumCtx);
    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return BOOL(1);
    }
    let mut cloaked = 0u32;
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut _,
        std::mem::size_of::<u32>() as u32,
    )
    .is_ok()
        && cloaked != 0
    {
        return BOOL(1);
    }
    if window_pid(hwnd) == ctx.pid {
        return BOOL(1);
    }
    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TRANSPARENT.0 != 0 {
        return BOOL(1);
    }
    let class = class_name(hwnd);
    let taskbar = class == "Shell_TrayWnd" || class == "Shell_SecondaryTrayWnd";
    if (ex & WS_EX_TOOLWINDOW.0 != 0 && !taskbar) || class == "Progman" || class == "WorkerW" {
        return BOOL(1);
    }
    let mut rc = RECT::default();
    let ok = DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut rc as *mut RECT as *mut _,
        std::mem::size_of::<RECT>() as u32,
    )
    .is_ok()
        || GetWindowRect(hwnd, &mut rc).is_ok();
    if ok && rc.right > rc.left && rc.bottom > rc.top {
        ctx.list.push((
            hwnd.0 as usize as u64,
            Rect { l: rc.left, t: rc.top, r: rc.right, b: rc.bottom },
            taskbar,
        ));
    }
    BOOL(1)
}

/// For each monitor (same order as `monitors`), the visible top-edge ledges of top-level
/// windows and the taskbar, in that monitor's overlay-local physical pixels, sorted by y.
pub fn scan_surfaces(monitors: &[MonitorInfo]) -> Vec<Vec<Surface>> {
    let mut ctx = EnumCtx { pid: unsafe { GetCurrentProcessId() }, list: Vec::with_capacity(64) };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut EnumCtx as isize));
    }
    ledges(&ctx.list, monitors)
}

/// Raw cursor state in virtual-screen physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawPointer {
    pub x: i32,
    pub y: i32,
    pub down: bool,
    pub over_desktop: bool,
}

thread_local! {
    static LAST_ROOT: Cell<(isize, bool)> = const { Cell::new((0, false)) };
}

fn root_is_desktop(root: HWND) -> bool {
    let key = root.0 as isize;
    let (k, v) = LAST_ROOT.with(|c| c.get());
    if k == key && key != 0 {
        return v;
    }
    let class = class_name(root);
    let v = class == "Progman"
        || class == "WorkerW"
        || window_pid(root) == unsafe { GetCurrentProcessId() };
    LAST_ROOT.with(|c| c.set((key, v)));
    v
}

/// Current cursor position/button. None if unavailable (e.g. secure desktop).
pub fn pointer_sample() -> Option<RawPointer> {
    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        let vk = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON };
        let down = (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0;
        let hwnd = WindowFromPoint(pt);
        let over_desktop = if hwnd.0.is_null() {
            false
        } else {
            let root = GetAncestor(hwnd, GA_ROOT);
            let root = if root.0.is_null() { hwnd } else { root };
            root_is_desktop(root)
        };
        Some(RawPointer { x: pt.x, y: pt.y, down, over_desktop })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, y: i32, w: i32, h: i32) -> MonitorInfo {
        MonitorInfo {
            id: "m".into(),
            rect: (x, y, w, h),
            work_rect: (x, y, w, h),
            primary: x == 0,
            dpi_scale: 1.0,
        }
    }
    fn r(l: i32, t: i32, rr: i32, b: i32) -> Rect {
        Rect { l, t, r: rr, b }
    }

    #[test]
    fn sub_no_overlap() {
        assert_eq!(subtract_ranges(0, 100, &[(200, 300), (-50, -10)]), vec![(0, 100)]);
        assert_eq!(subtract_ranges(0, 100, &[]), vec![(0, 100)]);
    }
    #[test]
    fn sub_partial() {
        assert_eq!(subtract_ranges(0, 100, &[(50, 150)]), vec![(0, 50)]);
        assert_eq!(subtract_ranges(0, 100, &[(-10, 30)]), vec![(30, 100)]);
        assert_eq!(subtract_ranges(0, 100, &[(40, 60)]), vec![(0, 40), (60, 100)]);
    }
    #[test]
    fn sub_full() {
        assert!(subtract_ranges(0, 100, &[(-5, 105)]).is_empty());
        assert!(subtract_ranges(0, 100, &[(0, 50), (50, 100)]).is_empty());
    }
    #[test]
    fn sub_multiple_unsorted_overlapping() {
        let got = subtract_ranges(0, 100, &[(70, 80), (10, 30), (20, 40), (75, 90)]);
        assert_eq!(got, vec![(0, 10), (40, 70), (90, 100)]);
    }
    #[test]
    fn sub_touching() {
        assert_eq!(subtract_ranges(0, 100, &[(100, 200), (-100, 0)]), vec![(0, 100)]);
        assert_eq!(subtract_ranges(0, 100, &[(0, 40), (60, 100)]), vec![(40, 60)]);
    }

    #[test]
    fn window_under_another_is_split() {
        let w = [(1, r(100, 100, 200, 400), false), (2, r(0, 300, 500, 600), false)];
        let s = &ledges(&w, &[mon(0, 0, 1000, 800)])[0];
        let back: Vec<_> = s.iter().filter(|s| s.id == 2).map(|s| (s.x0, s.x1)).collect();
        assert_eq!(back, vec![(0.0, 100.0), (200.0, 500.0)]);
        assert!(s.iter().any(|s| s.id == 1 && s.y == 100.0));
    }

    #[test]
    fn occluder_starting_at_edge_does_not_hide() {
        // Front window starts exactly at y=300 (row 299 is not covered): no cut.
        let w = [(1, r(100, 300, 200, 400), false), (2, r(0, 300, 500, 600), false)];
        let s = &ledges(&w, &[mon(0, 0, 1000, 800)])[0];
        assert!(s.iter().any(|s| s.id == 2 && s.x0 == 0.0 && s.x1 == 500.0));
    }

    #[test]
    fn taskbar_ledge_present() {
        let w = [(9, r(0, 1040, 1920, 1080), true)];
        let s = &ledges(&w, &[mon(0, 0, 1920, 1080)])[0];
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].id, s[0].y, s[0].x0, s[0].x1), (9, 1040.0, 0.0, 1920.0));
    }

    #[test]
    fn maximized_yields_nothing() {
        let w = [(1, r(0, 0, 1920, 1040), false)];
        assert!(ledges(&w, &[mon(0, 0, 1920, 1080)])[0].is_empty());
    }

    #[test]
    fn short_segments_dropped() {
        let w = [(1, r(100, 100, 110, 400), false), (2, r(0, 300, 120, 600), false)];
        let s = &ledges(&w, &[mon(0, 0, 1000, 800)])[0];
        assert_eq!(s.iter().filter(|s| s.id == 2).count(), 1);
        assert!(s.iter().all(|s| s.id != 1));
    }

    #[test]
    fn multi_monitor_negative_x() {
        let mons = [mon(0, 0, 1920, 1080), mon(-1280, 100, 1280, 1024)];
        let w = [(5, r(-600, 500, 600, 900), false)];
        let s = ledges(&w, &mons);
        assert_eq!(s[0].len(), 1);
        assert_eq!((s[0][0].x0, s[0][0].x1, s[0][0].y), (0.0, 600.0, 500.0));
        assert_eq!(s[1].len(), 1);
        assert_eq!((s[1][0].x0, s[1][0].x1, s[1][0].y), (680.0, 1280.0, 400.0));
    }

    #[test]
    fn offscreen_y_dropped() {
        let w = [(1, r(0, 2000, 500, 2100), false)];
        assert!(ledges(&w, &[mon(0, 0, 1920, 1080)])[0].is_empty());
    }

    #[test]
    fn cap_64_keeps_topmost_and_sorts() {
        // 80 disjoint windows, front to back; tops descend so z-order != y-order.
        let w: Vec<_> = (0..80u64)
            .map(|i| {
                let l = i as i32 * 30;
                (i, r(l, 900 - i as i32 * 10, l + 25, 1000), false)
            })
            .collect();
        let s = &ledges(&w, &[mon(0, 0, 4000, 1080)])[0];
        assert_eq!(s.len(), 64);
        assert!(s.iter().all(|s| s.id < 64));
        assert!(s.windows(2).all(|p| p[0].y <= p[1].y));
    }

    #[test]
    fn smoke_real_scan_and_pointer() {
        let mons = crate::monitors();
        let t = std::time::Instant::now();
        let s = scan_surfaces(&mons);
        println!("scan_surfaces took {:?}", t.elapsed());
        assert_eq!(s.len(), mons.len());
        for v in &s {
            assert!(v.len() <= 64);
            assert!(v.windows(2).all(|p| p[0].y <= p[1].y));
        }
        let t = std::time::Instant::now();
        let p = pointer_sample();
        println!("pointer_sample took {:?}: {:?}", t.elapsed(), p);
    }
}
