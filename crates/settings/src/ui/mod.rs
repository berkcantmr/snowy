//! UI building blocks and pages.

pub mod art_paint;
pub mod cards;
pub mod friends;
pub mod icons;
pub mod pages;
pub mod theme;
pub mod wallpapers;
pub mod whats_new_art;

use eframe::egui::{self, pos2, vec2, Align, Color32, Layout, Rect, RichText, Sense};
use theme::Palette;
use crate::ui::theme::RichExt;

/// A rounded card containing grouped settings.
pub fn card(ui: &mut egui::Ui, pal: &Palette, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(pal.card)
        .stroke(egui::Stroke::new(1.0, pal.border))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(10.0);
}

pub fn page_title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).heading().semi());
    ui.add_space(8.0);
}

/// Thin divider between rows of one card.
pub fn divider(ui: &mut egui::Ui, pal: &Palette) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, pal.border.gamma_multiply(0.7));
    ui.add_space(4.0);
}

fn label_and_hint(ui: &mut egui::Ui, pal: &Palette, label: &str, hint: &str, width: f32, enabled: bool) -> Rect {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        let (main, weak) = if enabled { (pal.text, pal.text_weak) } else { (pal.text_weak, pal.text_weak.gamma_multiply(0.8)) };
        ui.add(egui::Label::new(RichText::new(label).semi().color(main)).wrap());
        if !hint.is_empty() {
            ui.add_space(-4.0);
            ui.add(egui::Label::new(RichText::new(hint).small().color(weak)).wrap());
        }
    })
    .response
    .rect
}

/// Label + hint on the left, a switch on the right. Returns true when toggled.
pub fn toggle_row(ui: &mut egui::Ui, pal: &Palette, label: &str, hint: &str, value: &mut bool, enabled: bool) -> bool {
    let right = ui.available_rect_before_wrap().right();
    let text_rect = label_and_hint(ui, pal, label, hint, ui.available_width() - 60.0, enabled);
    let row = Rect::from_min_max(text_rect.min, pos2(right, text_rect.max.y.max(text_rect.min.y + 24.0)));
    let id = ui.id().with(("toggle", label));
    let resp = ui.interact(row, id, if enabled { Sense::click() } else { Sense::hover() });
    let mut changed = false;
    if enabled && resp.clicked() {
        *value = !*value;
        changed = true;
    }
    let k = ui.ctx().animate_bool_responsive(id, *value);
    let track = Rect::from_center_size(pos2(right - 22.0, row.center().y), vec2(42.0, 22.0));
    let off = if pal.dark { Color32::from_rgb(70, 80, 98) } else { Color32::from_rgb(190, 200, 214) };
    let mut fill = off.lerp_to_gamma(pal.accent, k);
    // Light accent in dark mode: a dark knob keeps contrast when on.
    let mut knob = if pal.dark { Color32::WHITE.lerp_to_gamma(pal.on_accent, k) } else { Color32::WHITE };
    if !enabled {
        fill = fill.gamma_multiply(0.45);
        knob = knob.gamma_multiply(0.7);
    } else if resp.hovered() {
        fill = fill.lerp_to_gamma(Color32::WHITE, 0.1);
    }
    let p = ui.painter();
    p.rect_filled(track, 11, fill);
    let kx = track.left() + 11.0 + k * (track.width() - 22.0);
    p.circle_filled(pos2(kx, track.center().y), 8.5, knob);
    ui.add_space(2.0);
    changed
}

/// Label + hint, then a percentage slider underneath. Returns true when changed.
pub fn percent_slider_row(
    ui: &mut egui::Ui,
    pal: &Palette,
    lang: &'static str,
    label: &str,
    hint: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    label_and_hint(ui, pal, label, hint, ui.available_width(), true);
    ui.spacing_mut().slider_width = (ui.available_width() - 78.0).max(120.0);
    let slider = egui::Slider::new(value, range)
        .show_value(true)
        .custom_formatter(move |v, _| crate::state::format_percent(lang, v as f32))
        .custom_parser(|s| {
            let digits: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
            digits.replace(',', ".").parse::<f64>().ok().map(|n| n / 100.0)
        });
    let changed = ui.add(slider).changed();
    ui.add_space(2.0);
    changed
}
