//! Global shortcut for edit mode. Registered on the main thread without a window, so
//! `WM_HOTKEY` arrives in that thread's queue and `pump_messages` counts it.

use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

/// Id of the edit-mode hotkey (thread hotkeys use 0x0000..=0xBFFF).
pub(crate) const EDIT_HOTKEY_ID: i32 = 0x5E01;

static PRESSES: AtomicU32 = AtomicU32::new(0);

/// Called by the message pump for `WM_HOTKEY`.
pub(crate) fn on_hotkey(id: i32) {
    if id == EDIT_HOTKEY_ID {
        PRESSES.fetch_add(1, Ordering::AcqRel);
    }
}

/// A key combination: modifiers plus one letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub win: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: char,
}

impl Combo {
    fn modifiers(&self) -> HOT_KEY_MODIFIERS {
        let mut m = MOD_NOREPEAT;
        for (on, f) in [(self.win, MOD_WIN), (self.ctrl, MOD_CONTROL), (self.alt, MOD_ALT), (self.shift, MOD_SHIFT)] {
            if on {
                m |= f;
            }
        }
        m
    }
}

/// Preferred shortcuts in order. Ctrl+Alt alone is avoided: on many layouts (Turkish Q, German)
/// it is AltGr, which types characters.
pub const EDIT_COMBOS: [Combo; 2] = [
    Combo { win: true, ctrl: false, alt: true, shift: false, key: 'S' },
    Combo { win: false, ctrl: true, alt: true, shift: true, key: 'S' },
];

/// The registered edit-mode shortcut; unregistered on drop.
pub struct EditHotkey {
    pub combo: Combo,
}

impl EditHotkey {
    /// Registers the first free combination of `EDIT_COMBOS`. Must run on the thread that
    /// calls `pump_messages`. None when every combination is taken by another app.
    pub fn register() -> Option<Self> {
        for combo in EDIT_COMBOS {
            let ok = unsafe { RegisterHotKey(None, EDIT_HOTKEY_ID, combo.modifiers(), combo.key as u32) };
            match ok {
                Ok(()) => return Some(Self { combo }),
                Err(e) => log::warn!("edit hotkey {combo:?} unavailable: {e}"),
            }
        }
        None
    }

    /// Number of presses since the last call.
    pub fn take_presses(&self) -> u32 {
        PRESSES.swap(0, Ordering::AcqRel)
    }
}

impl Drop for EditHotkey {
    fn drop(&mut self) {
        unsafe {
            let _ = UnregisterHotKey(None, EDIT_HOTKEY_ID);
        }
    }
}
