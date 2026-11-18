//! Selectable effect cards.

use eframe::egui::{self, vec2, FontId, Rect, Sense, Stroke, StrokeKind};
use snowy_core::{i18n, EffectChoice};

use super::{icons, theme::Palette};
use crate::state;

pub const CARD_HEIGHT: f32 = 98.0;

/// Draws one card. `animate` lets the icon move; the caller decides when that is allowed.
/// The card requests repaints only while animating.
pub fn effect_card(
    ui: &mut egui::Ui,
    pal: &Palette,
    lang: &str,
    choice: EffectChoice,
    selected: bool,
    width: f32,
    allow_anim_when_selected: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, CARD_HEIGHT), Sense::click());
    let hovered = resp.hovered();
    let p = ui.painter_at(rect);

    let fill = if selected {
        pal.accent_soft
    } else if hovered {
        pal.card_hover
    } else {
        pal.card
    };
    let stroke = if selected {
        Stroke::new(2.0, pal.accent)
    } else if hovered {
        Stroke::new(1.0, pal.accent.gamma_multiply(0.6))
    } else {
        Stroke::new(1.0, pal.border)
    };
    p.rect_filled(rect, 12, fill);
    p.rect_stroke(rect, 12, stroke, StrokeKind::Inside);

    let animate = icons::is_animated(choice) && (hovered || (selected && allow_anim_when_selected));
    let t = if animate { ui.input(|i| i.time) as f32 } else { 0.0 };
    if animate {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
    let icon_rect = Rect::from_center_size(rect.center_top() + vec2(0.0, 34.0), vec2(44.0, 44.0));
    icons::paint_choice(&p.with_clip_rect(rect.shrink(2.0)), icon_rect, choice, t, pal);

    let label = i18n::t(lang, &state::effect_label_key(choice));
    let color = if selected { pal.accent } else { pal.text };
    let galley = p.layout(label.to_owned(), if selected { super::theme::semibold(13.5) } else { FontId::proportional(13.5) }, color, width - 12.0);
    let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, rect.bottom() - 10.0 - galley.size().y);
    p.galley(pos, galley, color);

    if selected {
        let badge = rect.right_top() + vec2(-14.0, 14.0);
        p.circle_filled(badge, 8.0, pal.accent);
        icons::check(&p, badge, 9.0, pal.on_accent);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
