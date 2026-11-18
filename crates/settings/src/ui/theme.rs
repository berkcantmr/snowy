//! Light and dark palettes and the egui style derived from them.
//!
//! Design notes
//! - Three clearly separated surfaces: sidebar (furthest back) < content background < cards.
//!   In dark mode the sidebar is the darkest layer; in light mode it is a cool tint and
//!   cards are pure white.
//! - One accent: an icy winter blue. Light-mode accent is deep enough to be used as text on
//!   white (>= 4.5:1, WCAG AA); dark-mode accent is a light ice blue on deep slate.
//! - Secondary text keeps >= 4.5:1 on every surface it appears on.
//! - Typography: Segoe UI for body, Segoe UI Semibold (real weight, see fonts.rs) for page
//!   titles, setting names and selected items. Sizes: title 22, app name 19, body 14.5,
//!   secondary 12.5.

use eframe::egui::{self, Color32, FontFamily, FontId, RichText};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Content area background.
    pub bg: Color32,
    /// Navigation sidebar (and the bottom bar), the furthest-back surface.
    pub sidebar: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            dark,
            bg: Color32::from_rgb(25, 30, 39),
            sidebar: Color32::from_rgb(18, 22, 30),
            card: Color32::from_rgb(34, 41, 53),
            card_hover: Color32::from_rgb(42, 51, 66),
            border: Color32::from_rgb(52, 62, 79),
            text: Color32::from_rgb(234, 239, 246),
            text_weak: Color32::from_rgb(156, 168, 186),
            accent: Color32::from_rgb(125, 190, 250),
            accent_soft: Color32::from_rgb(33, 54, 82),
            on_accent: Color32::from_rgb(10, 22, 38),
            danger: Color32::from_rgb(242, 139, 130),
        }
    } else {
        Palette {
            dark,
            bg: Color32::from_rgb(245, 247, 251),
            sidebar: Color32::from_rgb(233, 238, 245),
            card: Color32::from_rgb(255, 255, 255),
            card_hover: Color32::from_rgb(241, 246, 252),
            border: Color32::from_rgb(218, 225, 235),
            text: Color32::from_rgb(24, 33, 48),
            text_weak: Color32::from_rgb(88, 101, 118),
            accent: Color32::from_rgb(38, 108, 186),
            accent_soft: Color32::from_rgb(226, 238, 252),
            on_accent: Color32::WHITE,
            danger: Color32::from_rgb(194, 48, 72),
        }
    }
}

/// Semibold font of the given size (falls back to regular when Segoe UI Semibold is missing).
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(crate::fonts::SEMIBOLD.into()))
}

/// `RichText` helpers so emphasis means weight, not just a brighter color.
pub trait RichExt {
    /// Semibold at the current text style size.
    fn semi(self) -> RichText;
}

impl RichExt for RichText {
    fn semi(self) -> RichText {
        self.family(FontFamily::Name(crate::fonts::SEMIBOLD.into()))
    }
}

/// Applies the palette to egui (theme, visuals and spacing).
pub fn apply(ctx: &egui::Context, pal: &Palette) {
    ctx.set_theme(if pal.dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
    let mut v = if pal.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = pal.bg;
    v.window_fill = pal.card;
    v.extreme_bg_color = if pal.dark { pal.sidebar } else { pal.card_hover };
    v.faint_bg_color = pal.card_hover;
    v.override_text_color = Some(pal.text);
    v.selection.stroke = egui::Stroke::new(1.0, pal.text);
    v.hyperlink_color = pal.accent;
    v.window_corner_radius = 10.into();
    v.weak_text_color = Some(pal.text_weak);
    let border = egui::Stroke::new(1.0, pal.border);
    v.widgets.noninteractive.bg_stroke = border;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, pal.text);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 8.into();
        w.fg_stroke = egui::Stroke::new(1.0, pal.text);
    }
    // Resting controls sit slightly above the card they live on.
    v.widgets.inactive.bg_fill = pal.card_hover;
    v.widgets.inactive.weak_bg_fill = pal.card_hover;
    v.widgets.inactive.bg_stroke = border;
    v.widgets.hovered.bg_fill = pal.accent_soft;
    v.widgets.hovered.weak_bg_fill = pal.accent_soft;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, pal.accent);
    v.widgets.active.bg_fill = pal.accent_soft;
    v.widgets.active.weak_bg_fill = pal.accent_soft;
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, pal.accent);
    // Slider fill / checked state use the accent.
    v.selection.bg_fill = pal.accent;
    // Show slider progress in the accent color, not only the knob position.
    v.slider_trailing_fill = true;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.slider_rail_height = 6.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.5));
        s.text_styles.insert(egui::TextStyle::Heading, semibold(22.0));
    });
}
