//! State of the "check / download / install" flow on the About page. All network work runs on
//! a background thread; the UI only polls a channel, so the window stays responsive.

use eframe::egui;
use snowy_update::Release;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

pub enum Status {
    /// Nothing checked yet in this window.
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading { done: u64, total: u64 },
    /// Installer started; the window closes shortly (the installer replaces the app).
    Installing(Instant),
    Failed(String),
}

enum Msg {
    Checked(Result<Option<Release>, String>),
    Progress(u64, u64),
    Launched(Result<(), String>),
}

pub struct UpdateUi {
    pub status: Status,
    /// The newer release found by the last check (kept after a failed install for a retry).
    pub release: Option<Release>,
    rx: Option<Receiver<Msg>>,
    auto_checked: bool,
}

impl Default for UpdateUi {
    fn default() -> Self {
        Self { status: Status::Idle, release: None, rx: None, auto_checked: false }
    }
}

impl UpdateUi {
    pub fn busy(&self) -> bool {
        matches!(self.status, Status::Checking | Status::Downloading { .. } | Status::Installing(_))
    }

    /// Runs one automatic check the first time the About page is shown.
    pub fn auto_check(&mut self, ctx: &egui::Context, enabled: bool) {
        if enabled && !self.auto_checked && matches!(self.status, Status::Idle) {
            self.check(ctx);
        }
        self.auto_checked = true;
    }

    pub fn check(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        self.status = Status::Checking;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("snowy-update-check".into()).spawn(move || {
            let r = snowy_update::check_latest().map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Checked(r));
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            self.status = Status::Failed(e.to_string());
        }
    }

    /// Downloads, verifies and launches the installer of `self.release`.
    pub fn install(&mut self, ctx: &egui::Context) {
        let Some(release) = self.release.clone() else { return };
        if self.busy() {
            return;
        }
        self.status = Status::Downloading { done: 0, total: release.size };
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("snowy-update-install".into()).spawn(move || {
            let progress_tx = tx.clone();
            let progress_ctx = ctx.clone();
            let result = snowy_update::download_verified(&release, move |done, total| {
                let _ = progress_tx.send(Msg::Progress(done, total));
                progress_ctx.request_repaint();
            })
            .and_then(|path| snowy_update::launch_installer(&path))
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Launched(result));
            ctx.request_repaint();
        });
        if let Err(e) = spawned {
            self.status = Status::Failed(e.to_string());
        }
    }

    /// Applies finished background work. Returns true when the window should close.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Msg::Checked(Ok(Some(r))) => {
                        self.release = Some(r);
                        self.status = Status::Available;
                    }
                    Msg::Checked(Ok(None)) => {
                        self.release = None;
                        self.status = Status::UpToDate;
                    }
                    Msg::Checked(Err(e)) => self.status = Status::Failed(e),
                    Msg::Progress(done, total) => self.status = Status::Downloading { done, total },
                    Msg::Launched(Ok(())) => self.status = Status::Installing(Instant::now()),
                    Msg::Launched(Err(e)) => self.status = Status::Failed(e),
                }
            }
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        matches!(self.status, Status::Installing(t) if t.elapsed() > Duration::from_millis(1500))
    }
}

/// "12.3 MB"
pub fn format_mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mb_format() {
        assert_eq!(format_mb(0), "0.0 MB");
        assert_eq!(format_mb(5 * 1024 * 1024 + 524288), "5.5 MB");
    }

    #[test]
    fn idle_is_not_busy() {
        let u = UpdateUi::default();
        assert!(!u.busy());
        assert!(u.release.is_none());
    }
}
