//! Windows integration: monitors, power, fullscreen detection, autostart, single instance,
//! message pump and the system tray menu.

mod autostart;
mod icon;
mod instance;
mod monitor;
mod power;
mod pump;
mod toast;
mod tray;

pub use autostart::set_autostart;
pub use instance::{single_instance, InstanceGuard};
pub use monitor::{init_dpi_awareness, monitors, ui_language};
pub use power::{foreground_is_fullscreen, on_battery};
pub use pump::{close_process_windows, pump_messages, wait_vblank};
pub use toast::{Key, Toast, ToastIcon};
pub use tray::{Tray, TrayCommand, TrayState};

mod scene;
pub use scene::{pointer_sample, scan_surfaces, RawPointer};

mod hotkey;
pub use hotkey::{Combo, EditHotkey};

mod grab;
pub use grab::{MouseGrab, ScreenRect};

mod desktop_icons;
pub use desktop_icons::{query_desktop_icons, DesktopIcons};

mod popup;
pub use popup::open_shell_popup;

mod icons_visibility;
pub use icons_visibility::{desktop_icons_visible, set_desktop_icons_visible};
