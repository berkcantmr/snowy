//! Pure logic for the settings window (no egui types, unit-testable).

use snowy_core::{i18n, EffectChoice, EffectKind, MonitorInfo};
use std::time::Duration;

/// Minimum time between two saves while the user keeps dragging a slider.
pub const SAVE_INTERVAL: Duration = Duration::from_millis(250);

pub const FPS_CHOICES: [u32; 3] = [30, 60, 120];

/// Display name of a language code (native names are not translated); "" is "Automatic".
pub fn language_name(code: &str, lang: &str) -> &'static str {
    i18n::LANGS
        .iter()
        .find(|(c, _)| *c == code)
        .map_or_else(|| i18n::t(lang, "settings.language_auto"), |(_, name)| name)
}

/// Normalizes any stored language string to a code from `i18n::LANGS`, or "" (automatic).
pub fn normalize_language(stored: &str) -> &'static str {
    let s = stored.to_ascii_lowercase();
    let primary = s.split(['-', '_']).next().unwrap_or("");
    i18n::LANGS.iter().find(|(c, _)| *c == primary).map_or("", |(c, _)| *c)
}

/// Effective UI language. `windows_locale` is the Windows UI locale name (e.g. "tr-TR"),
/// used when the setting is automatic. Dev override: env `SNOWY_LANG=ja` replaces the
/// Windows locale (only while the setting is automatic).
pub fn effective_lang(setting: &str, windows_locale: Option<&str>) -> &'static str {
    if setting.is_empty() {
        if let Ok(forced) = std::env::var("SNOWY_LANG") {
            if !forced.is_empty() {
                return i18n::resolve_lang(&forced);
            }
        }
    }
    match (setting.is_empty(), windows_locale) {
        (true, Some(loc)) if !loc.is_empty() => i18n::resolve_lang(loc),
        _ => i18n::resolve_lang(setting),
    }
}

/// All selectable effect choices in display order.
pub fn effect_choices() -> Vec<EffectChoice> {
    let mut v: Vec<EffectChoice> = EffectKind::ALL.iter().map(|k| EffectChoice::Fixed(*k)).collect();
    v.push(EffectChoice::Weather);
    v.push(EffectChoice::Season);
    v.push(EffectChoice::Random);
    v.push(EffectChoice::Off);
    v
}

pub fn effect_label_key(c: EffectChoice) -> String {
    match c {
        EffectChoice::Off => "effect.off".to_string(),
        EffectChoice::Season => "effect.season".to_string(),
        EffectChoice::Random => "effect.random".to_string(),
        EffectChoice::Weather => "effect.weather".to_string(),
        EffectChoice::Fixed(k) => i18n::effect_key(k),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

fn screen_word(lang: &str) -> &'static str {
    snowy_core::i18n::t(lang, "settings.screen")
}

/// "Screen 1 (main) 2560×1440"
pub fn monitor_label(index: usize, m: &MonitorInfo, lang: &str) -> String {
    let primary = if m.primary {
        format!(" ({})", i18n::t(lang, "settings.primary"))
    } else {
        String::new()
    };
    format!("{} {}{} {}\u{d7}{}", screen_word(lang), index + 1, primary, m.rect.2, m.rect.3)
}

pub fn monitor_rows(monitors: &[MonitorInfo], disabled: &[String], lang: &str) -> Vec<MonitorRow> {
    monitors
        .iter()
        .enumerate()
        .map(|(i, m)| MonitorRow {
            id: m.id.clone(),
            label: monitor_label(i, m, lang),
            enabled: !disabled.contains(&m.id),
        })
        .collect()
}

/// Enables/disables a monitor. Refuses to disable the last enabled monitor.
/// Returns true if `disabled` changed.
pub fn set_monitor_enabled(disabled: &mut Vec<String>, monitors: &[MonitorInfo], id: &str, enable: bool) -> bool {
    let is_disabled = disabled.iter().any(|d| d == id);
    if enable {
        if is_disabled {
            disabled.retain(|d| d != id);
            return true;
        }
        return false;
    }
    if is_disabled {
        return false;
    }
    let enabled_left = monitors.iter().filter(|m| m.id != id && !disabled.contains(&m.id)).count();
    if enabled_left == 0 {
        return false;
    }
    disabled.push(id.to_string());
    true
}

/// Save debouncing with an injected clock (`now` = time since an arbitrary origin).
#[derive(Debug, Default)]
pub struct Debouncer {
    dirty: bool,
    last_save: Option<Duration>,
}

impl Debouncer {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns true if a save should happen now; records it as saved.
    pub fn should_save(&mut self, now: Duration) -> bool {
        if !self.dirty {
            return false;
        }
        match self.last_save {
            Some(t) if now.saturating_sub(t) < SAVE_INTERVAL => false,
            _ => {
                self.dirty = false;
                self.last_save = Some(now);
                true
            }
        }
    }

    /// Time until a pending save becomes allowed (None if nothing pending).
    pub fn pending_in(&self, now: Duration) -> Option<Duration> {
        if !self.dirty {
            return None;
        }
        Some(match self.last_save {
            Some(t) => SAVE_INTERVAL.saturating_sub(now.saturating_sub(t)),
            None => Duration::ZERO,
        })
    }

    /// Forces the dirty flag off (e.g. after a final flush).
    pub fn clear(&mut self) {
        self.dirty = false;
    }
}

/// Pages of the sidebar navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Effects,
    Appearance,
    Desktop,
    Wallpapers,
    System,
    About,
}

impl Page {
    pub const ALL: [Page; 6] = [Page::Effects, Page::Appearance, Page::Desktop, Page::Wallpapers, Page::System, Page::About];

    /// Page for a command-line id such as `--page about`.
    pub fn from_id(id: &str) -> Option<Page> {
        Some(match id.to_ascii_lowercase().as_str() {
            "effects" => Page::Effects,
            "appearance" => Page::Appearance,
            "desktop" => Page::Desktop,
            "wallpapers" => Page::Wallpapers,
            "system" => Page::System,
            "about" => Page::About,
            _ => return None,
        })
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Page::Effects => "page.effects",
            Page::Appearance => "page.appearance",
            Page::Desktop => "page.desktop",
            Page::Wallpapers => "page.wallpapers",
            Page::System => "page.system",
            Page::About => "page.about",
        }
    }
}

/// Applies a click on a wallpaper choice (`None` = keep my own). Returns true if it changed.
pub fn choose_wallpaper(current: &mut Option<String>, choice: Option<&str>) -> bool {
    let new = choice.map(str::to_string);
    if *current == new {
        return false;
    }
    *current = new;
    true
}

/// Percentage text for a 1.0-based multiplier: "%100" in Turkish, "100 %" (non-breaking
/// space) in French and German, "100%" elsewhere.
pub fn format_percent(lang: &str, value: f32) -> String {
    let n = (value * 100.0).round() as i32;
    match lang {
        "tr" => format!("%{n}"),
        "fr" | "de" => format!("{n}\u{a0}%"),
        _ => format!("{n}%"),
    }
}

/// Month (1..=12) for a Unix timestamp in seconds (UTC), via the civil-from-days algorithm.
pub fn month_from_unix_secs(secs: i64) -> u32 {
    let z = secs.div_euclid(86_400) + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    m as u32
}

/// Current month from the system clock (UTC; good enough for a season hint).
pub fn current_month() -> u32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    month_from_unix_secs(secs)
}

/// Effect used by "match the season" for a month: spring sakura, summer fireflies,
/// autumn leaves, winter snow.
pub fn season_effect(month: u32) -> EffectKind {
    match month {
        3..=5 => EffectKind::Sakura,
        6..=8 => EffectKind::Fireflies,
        9..=11 => EffectKind::Leaves,
        _ => EffectKind::Snow,
    }
}

/// Two-step "reset to defaults" confirmation: the first click arms it, a second click
/// within `CONFIRM_WINDOW` confirms; otherwise it disarms by itself.
#[derive(Debug, Default)]
pub struct ResetConfirm {
    armed_at: Option<Duration>,
}

pub const CONFIRM_WINDOW: Duration = Duration::from_secs(4);

impl ResetConfirm {
    /// Handles a click; returns true when the reset must be performed now.
    pub fn click(&mut self, now: Duration) -> bool {
        if self.is_armed(now) {
            self.armed_at = None;
            true
        } else {
            self.armed_at = Some(now);
            false
        }
    }

    pub fn is_armed(&self, now: Duration) -> bool {
        matches!(self.armed_at, Some(t) if now.saturating_sub(t) < CONFIRM_WINDOW)
    }

    /// Time until the armed state expires (None if not armed).
    pub fn expires_in(&self, now: Duration) -> Option<Duration> {
        let t = self.armed_at?;
        let left = CONFIRM_WINDOW.saturating_sub(now.saturating_sub(t));
        (!left.is_zero()).then_some(left)
    }

    pub fn cancel(&mut self) {
        self.armed_at = None;
    }
}

/// Fading "Saved" indicator timeline.
#[derive(Debug, Default)]
pub struct SavedFlash {
    at: Option<Duration>,
}

pub const SAVED_VISIBLE: Duration = Duration::from_millis(1800);

impl SavedFlash {
    pub fn mark(&mut self, now: Duration) {
        self.at = Some(now);
    }

    /// Opacity 0..=1: fully visible for a moment, then fading out.
    pub fn alpha(&self, now: Duration) -> f32 {
        let Some(t) = self.at else { return 0.0 };
        let el = now.saturating_sub(t).as_secs_f32();
        let total = SAVED_VISIBLE.as_secs_f32();
        if el >= total {
            0.0
        } else if el < total * 0.6 {
            1.0
        } else {
            1.0 - (el - total * 0.6) / (total * 0.4)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(id: &str, w: i32, h: i32, primary: bool) -> MonitorInfo {
        MonitorInfo { id: id.into(), rect: (0, 0, w, h), work_rect: (0, 0, w, h), primary, dpi_scale: 1.0 }
    }

    #[test]
    fn labels() {
        let m = mon("A", 2560, 1440, true);
        assert_eq!(monitor_label(0, &m, "en"), "Screen 1 (main) 2560\u{d7}1440");
        assert_eq!(monitor_label(0, &m, "tr"), "Ekran 1 (ana ekran) 2560\u{d7}1440");
        let m2 = mon("B", 1920, 1080, false);
        assert_eq!(monitor_label(1, &m2, "en"), "Screen 2 1920\u{d7}1080");
    }

    #[test]
    fn rows_reflect_disabled() {
        let ms = [mon("A", 10, 10, true), mon("B", 10, 10, false)];
        let rows = monitor_rows(&ms, &["B".to_string()], "en");
        assert!(rows[0].enabled && !rows[1].enabled);
    }

    #[test]
    fn keep_one_rule() {
        let ms = [mon("A", 10, 10, true), mon("B", 10, 10, false)];
        let mut d = vec![];
        assert!(set_monitor_enabled(&mut d, &ms, "A", false));
        assert_eq!(d, vec!["A"]);
        assert!(!set_monitor_enabled(&mut d, &ms, "B", false));
        assert_eq!(d, vec!["A"]);
        assert!(set_monitor_enabled(&mut d, &ms, "A", true));
        assert!(d.is_empty());
        assert!(!set_monitor_enabled(&mut d, &ms, "A", true));
    }

    #[test]
    fn single_monitor_cannot_be_disabled() {
        let ms = [mon("A", 10, 10, true)];
        let mut d = vec![];
        assert!(!set_monitor_enabled(&mut d, &ms, "A", false));
        assert!(d.is_empty());
    }

    #[test]
    fn stale_disabled_ids_do_not_count() {
        let ms = [mon("A", 10, 10, true), mon("B", 10, 10, false)];
        let mut d = vec!["GONE".to_string()];
        assert!(set_monitor_enabled(&mut d, &ms, "A", false));
        assert!(!set_monitor_enabled(&mut d, &ms, "B", false));
    }

    #[test]
    fn debounce_timing() {
        let ms = Duration::from_millis;
        let mut d = Debouncer::default();
        assert!(!d.should_save(ms(0)));
        d.mark_dirty();
        assert!(d.should_save(ms(0)));
        d.mark_dirty();
        assert!(!d.should_save(ms(100)));
        assert_eq!(d.pending_in(ms(100)), Some(ms(150)));
        assert!(d.should_save(ms(250)));
        assert!(!d.is_dirty());
        assert_eq!(d.pending_in(ms(300)), None);
    }

    #[test]
    fn language_mapping() {
        assert_eq!(normalize_language("TR"), "tr");
        assert_eq!(normalize_language("tr-TR"), "tr");
        assert_eq!(normalize_language("en"), "en");
        assert_eq!(normalize_language(""), "");
        assert_eq!(normalize_language("de"), "de");
        assert_eq!(normalize_language("pt-BR"), "pt");
        assert_eq!(normalize_language("zh_CN"), "zh");
        assert_eq!(normalize_language("klingon"), "");
        assert_eq!(effective_lang("ja", Some("tr-TR")), "ja");
        assert_eq!(language_name("ja", "en"), "日本語");
        assert_eq!(language_name("", "de"), "Automatisch");
        assert_eq!(effective_lang("", Some("tr-TR")), "tr");
        assert_eq!(effective_lang("", Some("de-DE")), "de");
        assert_eq!(effective_lang("", Some("sv-SE")), "en");
        assert_eq!(effective_lang("en", Some("tr-TR")), "en");
        assert_eq!(effective_lang("tr", None), "tr");
        assert_eq!(language_name("tr", "en"), "Türkçe");
        assert_eq!(language_name("", "tr"), "Otomatik");
    }

    #[test]
    fn effect_choice_list() {
        let c = effect_choices();
        assert_eq!(c.len(), 11);
        assert_eq!(effect_label_key(EffectChoice::Fixed(EffectKind::HeavySnow)), "effect.heavy_snow");
        for x in c {
            assert_ne!(i18n::t("en", &effect_label_key(x)), "???");
        }
    }

    #[test]
    fn percent_format() {
        assert_eq!(format_percent("en", 1.0), "100%");
        assert_eq!(format_percent("tr", 1.0), "%100");
        assert_eq!(format_percent("tr", 0.25), "%25");
        assert_eq!(format_percent("fr", 1.0), "100\u{a0}%");
        assert_eq!(format_percent("de", 0.5), "50\u{a0}%");
        assert_eq!(format_percent("ja", 1.0), "100%");
        assert_eq!(format_percent("en", 1.5), "150%");
        assert_eq!(format_percent("en", 0.504), "50%");
    }

    #[test]
    fn months_and_seasons() {
        assert_eq!(month_from_unix_secs(0), 1);
        assert_eq!(month_from_unix_secs(951_782_400), 2); // 2000-02-29
        assert_eq!(month_from_unix_secs(1_782_864_000), 7); // 2026-07-01
        assert_eq!(month_from_unix_secs(1_798_675_200), 12); // 2026-12-31
        assert_eq!(season_effect(1), EffectKind::Snow);
        assert_eq!(season_effect(4), EffectKind::Sakura);
        assert_eq!(season_effect(7), EffectKind::Fireflies);
        assert_eq!(season_effect(10), EffectKind::Leaves);
        assert_eq!(season_effect(12), EffectKind::Snow);
        for m in 1..=12 {
            let _ = season_effect(m);
        }
    }

    #[test]
    fn reset_confirm_machine() {
        let s = Duration::from_secs;
        let mut r = ResetConfirm::default();
        assert!(!r.is_armed(s(0)));
        assert!(!r.click(s(1)));
        assert!(r.is_armed(s(2)));
        assert_eq!(r.expires_in(s(2)), Some(s(3)));
        assert!(r.click(s(3)));
        assert!(!r.is_armed(s(3)));
        // expiry
        assert!(!r.click(s(10)));
        assert!(!r.is_armed(s(15)));
        assert_eq!(r.expires_in(s(15)), None);
        assert!(!r.click(s(20))); // re-arms instead of confirming
        r.cancel();
        assert!(!r.is_armed(s(20)));
    }

    #[test]
    fn saved_flash_fades() {
        let ms = Duration::from_millis;
        let mut f = SavedFlash::default();
        assert_eq!(f.alpha(ms(0)), 0.0);
        f.mark(ms(100));
        assert_eq!(f.alpha(ms(200)), 1.0);
        let mid = f.alpha(ms(100 + 1600));
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(f.alpha(ms(100 + 1800)), 0.0);
    }

    #[test]
    fn wallpaper_choice() {
        let mut cur = None;
        assert!(!choose_wallpaper(&mut cur, None));
        assert!(choose_wallpaper(&mut cur, Some("aurora")));
        assert_eq!(cur.as_deref(), Some("aurora"));
        assert!(!choose_wallpaper(&mut cur, Some("aurora")));
        assert!(choose_wallpaper(&mut cur, None));
        assert_eq!(cur, None);
        for t in snowy_wallpaper::Theme::ALL {
            assert_ne!(i18n::t("tr", &format!("wallpaper.theme.{}", t.id())), "???");
        }
        for t in snowy_wallpaper::TimeOfDay::ALL {
            assert_ne!(i18n::t("en", &format!("wallpaper.tod.{}", t.id())), "???");
        }
    }

    #[test]
    fn page_cycle() {
        assert_eq!(Page::ALL.len(), 6);
        assert_eq!(Page::default(), Page::Effects);
        assert_eq!(Page::from_id("about"), Some(Page::About));
        assert_eq!(Page::from_id("ABOUT"), Some(Page::About));
        assert_eq!(Page::from_id("nope"), None);
        for page in Page::ALL {
            assert_ne!(i18n::t("en", page.label_key()), "???");
            assert_ne!(i18n::t("tr", page.label_key()), "???");
        }
    }
}
