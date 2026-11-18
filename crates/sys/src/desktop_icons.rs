//! Screen rectangles of the icons on the Windows desktop.
//!
//! The shell exposes the desktop as an `IFolderView`; item positions come back in client
//! coordinates of the desktop list view and are converted to virtual-screen pixels here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use windows::core::{w, Interface};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IServiceProvider,
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IEnumIDList, IFolderView, IShellBrowser, IShellView, IShellWindows, ShellWindows,
    SID_STopLevelBrowser, SVGIO_ALLVIEW, SWC_DESKTOP, SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, IsWindowVisible};

use crate::ScreenRect;

const REFRESH: Duration = Duration::from_millis(1500);
/// A single query that takes longer than this is abandoned (the shell is unresponsive).
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

/// Screen rects (virtual-screen physical pixels, left, top, right, bottom; right/bottom exclusive)
/// of the desktop icons, refreshed in the background.
pub struct DesktopIcons {
    rects: Arc<Mutex<Vec<ScreenRect>>>,
    stop: Arc<AtomicBool>,
    wake: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl DesktopIcons {
    /// Starts a background thread that refreshes the list about every 1.5 s.
    pub fn start() -> Self {
        let rects = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (wake, rx) = mpsc::channel::<()>();
        let (r2, s2) = (rects.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("desktop-icons".into())
            .spawn(move || {
                while !s2.load(Ordering::Relaxed) {
                    let list = query_desktop_icons().unwrap_or_else(|e| {
                        log::debug!("desktop icon query failed: {e:#}");
                        Vec::new()
                    });
                    if let Ok(mut g) = r2.lock() {
                        *g = list;
                    }
                    match rx.recv_timeout(REFRESH) {
                        Err(RecvTimeoutError::Timeout) => {}
                        _ => break, // woken for shutdown or sender dropped
                    }
                }
            })
            .ok();
        Self { rects, stop, wake, thread }
    }

    /// Latest snapshot (cheap clone). Empty when icons are hidden, when the desktop can't be
    /// queried, or before the first refresh.
    pub fn rects(&self) -> Vec<ScreenRect> {
        self.rects.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl Drop for DesktopIcons {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.wake.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// One synchronous query. Runs the COM work on a helper thread so an unresponsive shell yields
/// an error after a few seconds instead of hanging the caller.
pub fn query_desktop_icons() -> Result<Vec<ScreenRect>> {
    if IN_FLIGHT.swap(true, Ordering::AcqRel) {
        return Err(anyhow!("a previous desktop query is still pending"));
    }
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("desktop-icons-query".into()).spawn(move || {
        let r = unsafe { query_com() };
        IN_FLIGHT.store(false, Ordering::Release);
        let _ = tx.send(r);
    });
    if let Err(e) = spawned {
        IN_FLIGHT.store(false, Ordering::Release);
        return Err(e.into());
    }
    match rx.recv_timeout(QUERY_TIMEOUT) {
        Ok(r) => r,
        Err(_) => Err(anyhow!("desktop query timed out")),
    }
}

/// Item cell (client coordinates, top-left + spacing) to a screen rect given the view origin.
fn cell_to_screen(origin: (i32, i32), pos: (i32, i32), spacing: (i32, i32)) -> Option<ScreenRect> {
    if spacing.0 <= 0 || spacing.1 <= 0 {
        return None;
    }
    let (l, t) = (origin.0 + pos.0, origin.1 + pos.1);
    Some((l, t, l + spacing.0, t + spacing.1))
}

struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

unsafe fn query_com() -> Result<Vec<ScreenRect>> {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            return Err(anyhow!("CoInitializeEx failed: {hr:?}"));
        }
        let _guard = ComGuard;

        let windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER).context("ShellWindows")?;
        let loc = VARIANT::from(0i32); // CSIDL_DESKTOP
        let root = VARIANT::default();
        let mut hwnd = 0i32;
        let disp = windows
            .FindWindowSW(&loc, &root, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)
            .context("FindWindowSW")?;
        let sp: IServiceProvider = disp.cast().context("IServiceProvider")?;
        let browser: IShellBrowser =
            sp.QueryService(&SID_STopLevelBrowser).context("QueryService")?;
        let view: IShellView = browser.QueryActiveShellView().context("QueryActiveShellView")?;
        let defview = view.GetWindow().context("IShellView::GetWindow")?;
        let list = FindWindowExW(Some(defview), None, w!("SysListView32"), None)
            .unwrap_or(HWND::default());
        if list.0.is_null() || !IsWindowVisible(list).as_bool() {
            return Ok(Vec::new());
        }
        let fv: IFolderView = view.cast().context("IFolderView")?;
        let mut cell = POINT::default();
        fv.GetSpacing(&mut cell).context("GetSpacing")?;
        let mut origin = POINT { x: 0, y: 0 };
        if !ClientToScreen(list, &mut origin).as_bool() {
            return Err(anyhow!("ClientToScreen failed"));
        }
        let en: IEnumIDList = fv.Items(SVGIO_ALLVIEW).context("Items")?;
        let mut out = Vec::new();
        loop {
            let mut pidl: [*mut ITEMIDLIST; 1] = [std::ptr::null_mut()];
            let mut got = 0u32;
            let hr = en.Next(&mut pidl, Some(&mut got));
            if hr.is_err() || got == 0 || pidl[0].is_null() {
                break;
            }
            let pos = fv.GetItemPosition(pidl[0]);
            CoTaskMemFree(Some(pidl[0] as *const _));
            if let Ok(p) = pos {
                if let Some(r) =
                    cell_to_screen((origin.x, origin.y), (p.x, p.y), (cell.x, cell.y))
                {
                    out.push(r);
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_cell() {
        assert_eq!(cell_to_screen((10, 20), (5, 6), (100, 90)), Some((15, 26, 115, 116)));
        assert_eq!(cell_to_screen((-1920, 0), (0, 0), (50, 50)), Some((-1920, 0, -1870, 50)));
    }

    #[test]
    fn rejects_empty_spacing() {
        assert_eq!(cell_to_screen((0, 0), (1, 1), (0, 10)), None);
    }
}
