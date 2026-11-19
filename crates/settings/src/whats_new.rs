//! The "What's new" window (`snowy.exe --whats-new`): a header, one card per highlight with a
//! custom-drawn preview, and a bottom bar with "Got it" / "Open settings".

use crate::ui::theme::{self, RichExt};
use crate::ui::{icons, whats_new_art};
use crate::win;
pub use crate::whats_new_data::{highlights, Highlight, VERSION};
use eframe::egui::{self, vec2, Align, FontId, Layout, Margin, RichText, Sense, Stroke, StrokeKind};
use snowy_core::i18n;
use std::time::{Duration, Instant};

const PREVIEW_H: f32 = 150.0;
const CARD_PAD: f32 = 10.0;

pub struct WhatsNewApp {
    pal: theme::Palette,
    lang: &'static str,
    start: Instant,
    auto_close: Option<Duration>,
    /// Whether "Open settings" really launches `snowy.exe --settings` (off in the demo).
    spawn_settings: bool,
    /// Demo only (`DEMO_SCROLL` px): fixed scroll offset so screenshots are reproducible.
    demo_scroll: Option<f32>,
}

impl WhatsNewApp {
    pub fn new(ctx: &egui::Context, lang: &'static str, auto_close: Option<Duration>, spawn_settings: bool) -> Self {
        let pal = theme::palette(win::system_prefers_dark());
        theme::apply(ctx, &pal);
        let demo_scroll = if spawn_settings { None } else { std::env::var("DEMO_SCROLL").ok().and_then(|v| v.parse().ok()) };
        Self { pal, lang, start: Instant::now(), auto_close, spawn_settings, demo_scroll }
    }

    fn header(&self, ui: &mut egui::Ui) {
        let pal = self.pal;
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
            icons::logo(ui.painter(), rect, &pal);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new(i18n::t(self.lang, "whats_new.title")).size(23.0).semi());
                let version = i18n::t(self.lang, "whats_new.version").replace("{v}", VERSION);
                ui.label(RichText::new(version).color(pal.accent).semi().size(13.5));
            });
        });
    }

    fn card(&self, ui: &mut egui::Ui, h: &Highlight, t: f32) {
        let pal = self.pal;
        let w = ui.available_width();
        let inner_w = w - 2.0 * CARD_PAD;
        let painter = ui.painter().clone();
        let title = painter.layout(i18n::t(self.lang, h.title).to_owned(), theme::semibold(16.5), pal.text, inner_w - 4.0);
        let body = painter.layout(i18n::t(self.lang, h.body).to_owned(), FontId::proportional(13.5), pal.text_weak, inner_w - 4.0);
        let height = CARD_PAD + PREVIEW_H + 12.0 + title.size().y + 4.0 + body.size().y + 14.0;
        let (rect, _) = ui.allocate_exact_size(vec2(w, height), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 14, pal.card);
        p.rect_stroke(rect, 14, Stroke::new(1.0, pal.border), StrokeKind::Inside);

        let prev = egui::Rect::from_min_size(rect.min + vec2(CARD_PAD, CARD_PAD), vec2(inner_w, PREVIEW_H));
        whats_new_art::paint(&p, prev, h.preview, t, &pal);

        let mut y = prev.bottom() + 12.0;
        let x = prev.left() + 2.0;
        let th = title.size().y;
        p.galley(egui::pos2(x, y), title, pal.text);
        y += th + 4.0;
        p.galley(egui::pos2(x, y), body, pal.text_weak);
    }

    fn bottom_bar(&self, ui: &mut egui::Ui) -> (bool, bool) {
        let pal = self.pal;
        let (mut got_it, mut settings) = (false, false);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let primary = egui::Button::new(RichText::new(i18n::t(self.lang, "whats_new.got_it")).semi().color(pal.on_accent))
                    .fill(pal.accent)
                    .stroke(Stroke::NONE)
                    .corner_radius(9)
                    .min_size(vec2(112.0, 34.0));
                got_it = ui.add(primary).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
                ui.add_space(6.0);
                let link = egui::Button::new(RichText::new(i18n::t(self.lang, "whats_new.open_settings")).color(pal.accent))
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(Stroke::NONE)
                    .min_size(vec2(0.0, 34.0));
                settings = ui.add(link).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
            });
        });
        (got_it, settings)
    }
}

impl eframe::App for WhatsNewApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        win::tick_window_guard(ui.ctx());
        let ctx = ui.ctx().clone();
        let pal = self.pal;
        let t = self.start.elapsed().as_secs_f32();

        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter));
        let (mut got_it, mut settings) = (false, false);
        egui::Panel::bottom("bottom_bar")
            .frame(egui::Frame::new().fill(pal.sidebar).stroke(Stroke::new(1.0, pal.border)).inner_margin(Margin::symmetric(16, 10)))
            .show(ui, |ui| (got_it, settings) = self.bottom_bar(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg).inner_margin(Margin::symmetric(18, 16))).show(ui, |ui| {
            self.header(ui);
            ui.add_space(10.0);
            let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if let Some(y) = self.demo_scroll {
                area = area.vertical_scroll_offset(y);
            }
            area.show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.set_max_width(ui.available_width() - 8.0);
                for h in highlights() {
                    self.card(ui, h, t);
                }
                ui.add_space(2.0);
            });
        });

        if settings && self.spawn_settings {
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).arg("--settings").spawn();
            }
        }
        close |= got_it || settings;
        if let Some(limit) = self.auto_close {
            close |= self.start.elapsed() >= limit;
        }
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if !ctx.input(|i| i.viewport().minimized.unwrap_or(false)) {
            // Previews drift and pulse: ~30 fps, but only while the window is shown.
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}
