//! Translation tables. Keys are shared by the tray menu and the settings window.
//! One file per language; `t` falls back to English, then to "???".

mod de;
mod en;
mod es;
mod fr;
mod ja;
mod pt;
mod ru;
mod tr;
mod zh;

/// Supported UI languages: (code, native name).
pub const LANGS: &[(&str, &str)] = &[
    ("en", "English"),
    ("tr", "Türkçe"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("fr", "Français"),
    ("ja", "日本語"),
    ("pt", "Português (Brasil)"),
    ("ru", "Русский"),
    ("zh", "简体中文"),
];

/// Every translation key (defined by the English table).
pub use en::KEYS;

/// Resolve the effective language: explicit setting (a code or a Windows locale name such as
/// "pt-BR"), else the system hint, else "en".
pub fn resolve_lang(setting: &str) -> &'static str {
    let code = if setting.is_empty() {
        sys_locale_hint()
    } else {
        setting.to_ascii_lowercase()
    };
    let code = code.replace('_', "-");
    // Chinese: only Simplified is available; Traditional locales fall back to English.
    if code == "zh" || code.starts_with("zh-") {
        let traditional = ["zh-hant", "zh-tw", "zh-hk", "zh-mo"].iter().any(|p| code.starts_with(p));
        return if traditional { "en" } else { "zh" };
    }
    let primary = code.split('-').next().unwrap_or("");
    LANGS.iter().find(|(c, _)| *c == primary).map_or("en", |(c, _)| *c)
}

fn sys_locale_hint() -> String {
    // Cheap heuristic without extra deps; snowy-sys may pass a better value via settings.
    std::env::var("LANG").unwrap_or_default().to_ascii_lowercase()
}

fn lookup(lang: &str, key: &str) -> Option<&'static str> {
    match lang {
        "tr" => tr::get(key),
        "de" => de::get(key),
        "es" => es::get(key),
        "fr" => fr::get(key),
        "ja" => ja::get(key),
        "pt" => pt::get(key),
        "ru" => ru::get(key),
        "zh" => zh::get(key),
        _ => en::get(key),
    }
}

pub fn t(lang: &str, key: &str) -> &'static str {
    lookup(lang, key).or_else(|| en::get(key)).unwrap_or("???")
}

/// Translation key for an effect kind.
pub fn effect_key(kind: crate::EffectKind) -> String {
    format!("effect.{}", kind.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_key() {
        assert!(KEYS.len() > 200);
        for (code, _) in LANGS {
            for key in KEYS {
                let v = lookup(code, key).unwrap_or_else(|| panic!("{code} is missing {key}"));
                assert!(!v.trim().is_empty(), "{code}/{key} is empty");
                assert_ne!(v, "???", "{code}/{key}");
            }
        }
    }

    #[test]
    fn no_unknown_extra_keys_and_unknown_key_is_placeholder() {
        assert_eq!(t("en", "nope"), "???");
        assert_eq!(t("xx", "effect.snow"), "Snow");
        assert_eq!(t("tr", "effect.snow"), "Kar");
    }

    #[test]
    fn effect_keys_exist() {
        for k in crate::EffectKind::ALL {
            for (code, _) in LANGS {
                assert_ne!(t(code, &effect_key(k)), "???", "{code} {k:?}");
            }
        }
    }

    #[test]
    fn resolves_languages() {
        assert_eq!(resolve_lang("tr"), "tr");
        assert_eq!(resolve_lang("TR-tr"), "tr");
        assert_eq!(resolve_lang("de-DE"), "de");
        assert_eq!(resolve_lang("pt-BR"), "pt");
        assert_eq!(resolve_lang("pt"), "pt");
        assert_eq!(resolve_lang("pt-PT"), "pt");
        assert_eq!(resolve_lang("zh-Hans"), "zh");
        assert_eq!(resolve_lang("zh-CN"), "zh");
        assert_eq!(resolve_lang("zh"), "zh");
        assert_eq!(resolve_lang("zh-TW"), "en");
        assert_eq!(resolve_lang("ja-JP"), "ja");
        assert_eq!(resolve_lang("ru"), "ru");
        assert_eq!(resolve_lang("klingon"), "en");
        for (code, _) in LANGS {
            assert_eq!(resolve_lang(code), *code);
        }
    }
}
