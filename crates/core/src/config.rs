//! Persistent settings, stored as JSON at %APPDATA%\Snowy\settings.json.
//! The settings window (separate process) writes this file; the main app polls its
//! modification time and reloads.

use crate::{EffectChoice, EffectKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub effect: EffectChoice,
    /// 0.25..=2.0
    pub intensity: f32,
    /// 0.5..=2.0
    pub speed: f32,
    /// Wind slowly changes direction and strength.
    pub changing_wind: bool,
    /// Draw under all windows (on the wallpaper) instead of on top of them.
    pub behind_windows: bool,
    /// Don't let snow/petals pile up on windows and the taskbar.
    pub keep_windows_clear: bool,
    /// Allow rolling and throwing snowballs with the mouse on the desktop.
    pub snowballs: bool,
    /// Stop rendering while on battery power.
    pub pause_on_battery: bool,
    /// Stop rendering while a fullscreen app/game is in the foreground.
    pub pause_on_fullscreen: bool,
    /// Launch with Windows (HKCU Run key).
    pub start_with_windows: bool,
    /// Monitor ids (MonitorInfo::id) that should NOT show weather. Empty = all monitors.
    pub disabled_monitors: Vec<String>,
    /// Minutes between switches when effect == Random.
    pub random_minutes: u32,
    /// Frame cap.
    pub max_fps: u32,
    /// UI language code: a code from `i18n::LANGS` (en, tr, de, ...). Empty = follow Windows.
    pub language: String,
    /// Painted wallpaper theme id (e.g. "snowy_hills"). None = keep the user's own wallpaper.
    pub wallpaper: Option<String>,
    /// true = follow the time of day; false = always use `wallpaper_time`.
    pub wallpaper_day_night: bool,
    /// Fixed time-of-day id (dawn, morning, midday, golden, sunset, dusk, night).
    pub wallpaper_time: String,
    /// The user's own wallpaper path, saved the first time Snowy changes it (restored on "None").
    pub original_wallpaper: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            effect: EffectChoice::Fixed(EffectKind::Snow),
            intensity: 1.0,
            speed: 1.0,
            changing_wind: true,
            behind_windows: false,
            keep_windows_clear: false,
            snowballs: true,
            pause_on_battery: true,
            pause_on_fullscreen: true,
            start_with_windows: true,
            disabled_monitors: Vec::new(),
            random_minutes: 30,
            max_fps: 60,
            language: String::new(),
            wallpaper: None,
            wallpaper_day_night: true,
            wallpaper_time: "golden".to_string(),
            original_wallpaper: None,
        }
    }
}

impl Settings {
    pub fn dir() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("Snowy")
    }

    pub fn path() -> PathBuf {
        Self::dir().join("settings.json")
    }

    /// Loads settings; returns defaults if missing or corrupt. Clamps ranges.
    pub fn load() -> Self {
        let mut s: Settings = std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        s.clamp();
        s
    }

    /// Atomic save (write temp file then rename).
    pub fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(Self::dir())?;
        let tmp = Self::dir().join("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, Self::path())?;
        Ok(())
    }

    pub fn clamp(&mut self) {
        self.intensity = self.intensity.clamp(0.25, 2.0);
        self.speed = self.speed.clamp(0.5, 2.0);
        self.max_fps = self.max_fps.clamp(15, 144);
        self.random_minutes = self.random_minutes.clamp(1, 24 * 60);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_files_still_load() {
        let s: Settings = serde_json::from_str(r#"{"intensity": 1.5, "language": "tr"}"#).unwrap();
        assert_eq!(s.intensity, 1.5);
        assert_eq!(s.wallpaper, None);
        assert!(s.wallpaper_day_night);
        assert_eq!(s.wallpaper_time, "golden");
        assert_eq!(s.original_wallpaper, None);
    }

    #[test]
    fn wallpaper_fields_round_trip() {
        let s = Settings {
            wallpaper: Some("aurora".into()),
            wallpaper_day_night: false,
            wallpaper_time: "night".into(),
            original_wallpaper: Some("a.jpg".into()),
            ..Settings::default()
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
