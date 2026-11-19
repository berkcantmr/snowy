//! Transparent, click-through overlay windows rendered with D3D11 + DirectComposition.

mod d3d;
mod desktop;
mod window;

use anyhow::Result;
use snowy_core::{Effect, MonitorInfo};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::*;

/// One overlay window covering one monitor.
pub struct OverlayWindow {
    hwnd: HWND,
    rect: (i32, i32, i32, i32),
    /// Parent window while parented into the desktop (behind-windows mode).
    parent: Option<HWND>,
    behind: bool,
    renderer: Option<d3d::Renderer>,
    /// The shell pop-up this (topmost) overlay is currently tucked under, if any.
    under: Option<isize>,
}

impl OverlayWindow {
    /// Create a borderless, click-through, non-activating, topmost (or behind-windows)
    /// overlay covering `monitor.rect`. Must be called on the thread that pumps messages.
    pub fn new(monitor: &MonitorInfo, behind_windows: bool) -> Result<Self> {
        let rect = monitor.rect;
        let hwnd = window::create(rect, true)?;
        let mut w = OverlayWindow { hwnd, rect, parent: None, behind: false, renderer: None, under: None };
        // On error `w` drops and destroys the window.
        w.renderer = Some(d3d::Renderer::new(hwnd, rect.2.max(1) as u32, rect.3.max(1) as u32)?);
        if behind_windows {
            w.set_behind_windows(true)?;
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(w)
    }

    /// Overlay size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.rect.2.max(1) as u32, self.rect.3.max(1) as u32)
    }

    /// Move/resize to a new monitor rect (resolution or DPI change).
    pub fn reposition(&mut self, monitor: &MonitorInfo) -> Result<()> {
        self.rect = monitor.rect;
        window::move_to(self.hwnd, self.rect, self.parent)?;
        let (w, h) = self.size();
        match self.renderer.as_mut() {
            Some(r) => r.resize(w, h),
            None => Ok(()),
        }
    }

    /// Switch between topmost overlay and behind-windows (wallpaper layer) mode.
    pub fn set_behind_windows(&mut self, behind: bool) -> Result<()> {
        if behind == self.behind {
            return Ok(());
        }
        if behind {
            match desktop::attach(self.hwnd, self.rect) {
                Ok(parent) => self.parent = Some(parent),
                Err(e) => {
                    log::warn!("could not attach to desktop ({e}); falling back to bottom of z-order");
                    desktop::detach(self.hwnd, self.rect, false)?;
                    self.parent = None;
                }
            }
        } else {
            desktop::detach(self.hwnd, self.rect, true)?;
            self.parent = None;
        }
        self.behind = behind;
        Ok(())
    }

    /// Draw one frame of `effect`. `time` is seconds since app start.
    pub fn render(&mut self, effect: &dyn Effect, time: f32) -> Result<()> {
        match self.renderer.as_mut() {
            Some(r) => r.render(effect, time),
            None => Ok(()),
        }
    }

    /// Present a fully transparent frame (used when paused / effect off).
    pub fn clear(&mut self) -> Result<()> {
        match self.renderer.as_mut() {
            Some(r) => r.clear(),
            None => Ok(()),
        }
    }

    /// While a shell pop-up (e.g. the taskbar's right-click menu) is open, keep this overlay
    /// just below it so snow never covers it; `None` puts the overlay back on top. Does
    /// nothing in behind-windows mode. Only touches the z-order when the answer changes.
    pub fn stay_under(&mut self, popup: Option<isize>) {
        if self.behind || self.under == popup {
            return;
        }
        self.under = popup;
        let after = match popup {
            Some(h) => HWND(h as *mut _),
            None => HWND_TOPMOST,
        };
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        unsafe {
            let _ = ShowWindow(self.hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        self.renderer = None;
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
