//! Font setup: Segoe UI (regular) for body text and Segoe UI Semibold as a separate family
//! for titles, setting names and selected states, so emphasis is a real weight instead of
//! egui's "strong" (which only changes color). egui's bundled fonts stay as fallbacks, so
//! Turkish and symbols render even without the Windows fonts.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

/// Name of the semibold family (see `ui::theme::semibold`).
pub const SEMIBOLD: &str = "semibold";

fn load(path: &str) -> Option<Arc<FontData>> {
    std::fs::read(path).ok().map(|b| Arc::new(FontData::from_owned(b)))
}

/// CJK fallback font files for a language: (regular candidates, bold candidates).
/// Segoe UI has no CJK glyphs, so these are loaded only for the languages that need them
/// (the files are large).
fn cjk_candidates(lang: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match lang {
        "ja" => Some((
            &[r"C:\Windows\Fonts\YuGothM.ttc", r"C:\Windows\Fonts\msgothic.ttc"],
            &[r"C:\Windows\Fonts\YuGothB.ttc", r"C:\Windows\Fonts\YuGothM.ttc", r"C:\Windows\Fonts\msgothic.ttc"],
        )),
        "zh" => Some((
            &[r"C:\Windows\Fonts\msyh.ttc", r"C:\Windows\Fonts\simsun.ttc"],
            &[r"C:\Windows\Fonts\msyhbd.ttc", r"C:\Windows\Fonts\msyh.ttc", r"C:\Windows\Fonts\simsun.ttc"],
        )),
        _ => None,
    }
}

/// First loadable font from `paths` (face index 0), registered under `name`.
fn load_first(fonts: &mut FontDefinitions, name: &str, paths: &[&str]) -> Option<String> {
    let data = paths.iter().find_map(|p| load(p))?;
    fonts.font_data.insert(name.to_string(), data);
    Some(name.to_string())
}

/// Installs the fonts for UI language `lang`; call again when the language changes.
pub fn install(ctx: &egui::Context, lang: &str) {
    let mut fonts = FontDefinitions::default();
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let mut prop = Vec::new();
    prop.extend(load_first(&mut fonts, "segoeui", &[r"C:\Windows\Fonts\segoeui.ttf"]));
    let mut semi = Vec::new();
    semi.extend(load_first(&mut fonts, "seguisb", &[r"C:\Windows\Fonts\seguisb.ttf"]));
    // Semibold falls back to regular Latin, then CJK, then egui's fonts.
    semi.extend(prop.iter().cloned());

    if let Some((regular, bold)) = cjk_candidates(lang) {
        let cjk = load_first(&mut fonts, "cjk", regular);
        let cjk_bold = load_first(&mut fonts, "cjk_bold", bold);
        prop.extend(cjk.clone());
        // Bold CJK first for semibold text; regular CJK as its own fallback.
        semi.extend(cjk_bold);
        semi.extend(cjk);
    }
    prop.extend(fallbacks.iter().cloned());
    semi.extend(fallbacks);

    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), semi);
    ctx.set_fonts(fonts);
}
