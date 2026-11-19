//! What's new: the list of highlights of one release. Future releases only edit this file
//! (and add the translation keys): bump `VERSION`, replace the entries, and add a `Preview`
//! variant plus its painter in `ui::whats_new_art` when a highlight needs a new picture.

/// The release these highlights belong to.
pub const VERSION: &str = "0.1.4";

/// Which custom-drawn picture goes on top of a highlight card (see `ui::whats_new_art`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preview {
    EditMode,
    Lights,
    Clicks,
    Gpu,
    Fit,
}

/// One card: a preview plus a title and a body, both i18n keys.
#[derive(Debug, Clone, Copy)]
pub struct Highlight {
    pub preview: Preview,
    pub title: &'static str,
    pub body: &'static str,
}

const HIGHLIGHTS: &[Highlight] = &[
    Highlight { preview: Preview::EditMode, title: "whats_new.edit_mode.title", body: "whats_new.edit_mode.body" },
    Highlight { preview: Preview::Lights, title: "whats_new.lights.title", body: "whats_new.lights.body" },
    Highlight { preview: Preview::Clicks, title: "whats_new.clicks.title", body: "whats_new.clicks.body" },
    Highlight { preview: Preview::Gpu, title: "whats_new.gpu.title", body: "whats_new.gpu.body" },
    Highlight { preview: Preview::Fit, title: "whats_new.fit.title", body: "whats_new.fit.body" },
];

/// The highlights to show, in order.
pub fn highlights() -> &'static [Highlight] {
    HIGHLIGHTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_highlight_is_translated_everywhere() {
        for h in highlights() {
            for (lang, _) in snowy_core::i18n::LANGS {
                for key in [h.title, h.body] {
                    assert_ne!(snowy_core::i18n::t(lang, key), "???", "{lang}: {key}");
                }
            }
        }
    }
}
