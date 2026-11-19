//! What's new: the highlights of each release. A new release adds its entries at the top of
//! `HIGHLIGHTS` (with its version), bumps `VERSION`, adds the translation keys, and adds a
//! `Preview` variant plus its painter in `ui::whats_new_art` when a highlight needs a new
//! picture. Someone who skipped releases sees the highlights of every one they missed.

/// The newest release with highlights.
pub const VERSION: &str = "0.1.6";

/// Which custom-drawn picture goes on top of a highlight card (see `ui::whats_new_art`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preview {
    Friends,
    Throw,
    SharedDesk,
    Drifts,
    Depth,
    Smooth,
    EditMode,
    Lights,
    Clicks,
    Gpu,
    Fit,
}

/// One card: the release it came with, a preview, and a title and a body (i18n keys).
#[derive(Debug, Clone, Copy)]
pub struct Highlight {
    pub version: &'static str,
    pub preview: Preview,
    pub title: &'static str,
    pub body: &'static str,
}

/// Newest first.
const HIGHLIGHTS: &[Highlight] = &[
    Highlight { version: "0.1.6", preview: Preview::Friends, title: "whats_new.friends.title", body: "whats_new.friends.body" },
    Highlight { version: "0.1.6", preview: Preview::Throw, title: "whats_new.throw.title", body: "whats_new.throw.body" },
    Highlight { version: "0.1.6", preview: Preview::SharedDesk, title: "whats_new.shared.title", body: "whats_new.shared.body" },
    Highlight { version: "0.1.5", preview: Preview::Drifts, title: "whats_new.drifts.title", body: "whats_new.drifts.body" },
    Highlight { version: "0.1.5", preview: Preview::Depth, title: "whats_new.depth.title", body: "whats_new.depth.body" },
    Highlight { version: "0.1.5", preview: Preview::Smooth, title: "whats_new.smooth.title", body: "whats_new.smooth.body" },
    Highlight { version: "0.1.4", preview: Preview::EditMode, title: "whats_new.edit_mode.title", body: "whats_new.edit_mode.body" },
    Highlight { version: "0.1.4", preview: Preview::Lights, title: "whats_new.lights.title", body: "whats_new.lights.body" },
    Highlight { version: "0.1.4", preview: Preview::Clicks, title: "whats_new.clicks.title", body: "whats_new.clicks.body" },
    Highlight { version: "0.1.4", preview: Preview::Gpu, title: "whats_new.gpu.title", body: "whats_new.gpu.body" },
    Highlight { version: "0.1.4", preview: Preview::Fit, title: "whats_new.fit.title", body: "whats_new.fit.body" },
];

/// Every highlight, newest first.
#[cfg(test)]
pub fn highlights() -> &'static [Highlight] {
    HIGHLIGHTS
}

/// The highlights someone coming from version `since` has not seen, newest first. An empty or
/// unreadable `since` shows only the newest release's.
pub fn highlights_since(since: &str) -> Vec<&'static Highlight> {
    let since = since.trim().trim_start_matches('v');
    HIGHLIGHTS
        .iter()
        .filter(|h| {
            if since.is_empty() || snowy_update::parse_version(since).is_none() {
                h.version == VERSION
            } else {
                snowy_update::is_newer(h.version, since)
            }
        })
        .collect()
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

    #[test]
    fn skipped_releases_are_included() {
        let n = |v: &str| highlights_since(v).len();
        let newest = highlights().iter().filter(|h| h.version == VERSION).count();
        // The release before the newest one sees only the newest cards.
        let previous = highlights().iter().map(|h| h.version).find(|v| *v != VERSION).unwrap();
        assert_eq!(n(previous), newest);
        assert_eq!(n("0.1.3"), highlights().len());
        assert_eq!(n(""), newest);
        assert_eq!(n("garbage"), newest);
        assert_eq!(n(VERSION), 0);
        assert!(highlights_since("0.1.3")[0].version == VERSION, "newest first");
        assert!(highlights().iter().all(|h| !snowy_update::is_newer(h.version, VERSION)));
    }
}
