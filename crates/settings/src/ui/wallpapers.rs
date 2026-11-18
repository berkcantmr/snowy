//! "Wallpapers" page: keep my own, six painted themes with live thumbnails, day & night options.

use eframe::egui::{self, vec2, FontId, Rect, Sense, Stroke, StrokeKind};
use snowy_core::i18n;
use snowy_wallpaper::{Theme, TimeOfDay};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

use super::{card, icons, page_title, toggle_row};
use crate::app::SettingsApp;
use crate::state;

const THUMB_W: u32 = 192;
const THUMB_H: u32 = 108;

/// Tiny in-memory preview cache, rendered on a background thread.
#[derive(Default)]
pub struct Thumbs {
    tod: Option<TimeOfDay>,
    rx: Option<Receiver<(usize, Vec<u8>)>>,
    tex: Vec<Option<egui::TextureHandle>>,
}

impl Thumbs {
    /// Starts (re)rendering when the previewed time of day changed, and uploads finished images.
    fn update(&mut self, ctx: &egui::Context, tod: TimeOfDay) {
        if self.tex.is_empty() {
            self.tex = vec![None; Theme::ALL.len()];
        }
        if self.tod != Some(tod) {
            self.tod = Some(tod);
            let (tx, rx) = channel();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                for (i, theme) in Theme::ALL.into_iter().enumerate() {
                    let img = snowy_wallpaper::render(theme, tod, THUMB_W, THUMB_H);
                    if tx.send((i, img)).is_err() {
                        return;
                    }
                    ctx2.request_repaint();
                }
            });
            self.rx = Some(rx);
        }
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok((i, rgba)) => {
                        let image = egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], &rgba);
                        self.tex[i] = Some(ctx.load_texture(format!("wallpaper-{i}"), image, egui::TextureOptions::LINEAR));
                    }
                    Err(TryRecvError::Empty) => {
                        ctx.request_repaint_after(std::time::Duration::from_millis(80));
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.rx = None;
                        break;
                    }
                }
            }
        }
    }
}

impl SettingsApp {
    pub(crate) fn page_wallpapers(&mut self, ui: &mut egui::Ui) -> bool {
        let (lang, pal) = (self.lang, self.pal);
        let mut dirty = false;
        page_title(ui, i18n::t(lang, "page.wallpapers"));

        let tod = snowy_wallpaper::target_tod(
            self.settings.wallpaper_day_night,
            &self.settings.wallpaper_time,
            snowy_wallpaper::local_hour(),
        );
        self.thumbs.update(ui.ctx(), tod);

        let gap = 10.0;
        ui.spacing_mut().item_spacing = vec2(gap, gap);

        // "Keep my own" card
        let own_selected = self.settings.wallpaper.is_none();
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
        paint_card_frame(ui, &pal, rect, own_selected, resp.hovered());
        let p = ui.painter_at(rect);
        let color = if own_selected { pal.accent } else { pal.text };
        p.text(rect.left_center() + vec2(16.0, 0.0), egui::Align2::LEFT_CENTER, i18n::t(lang, "wallpaper.own"), FontId::proportional(14.5), color);
        if own_selected {
            let badge = rect.right_center() - vec2(18.0, 0.0);
            p.circle_filled(badge, 8.0, pal.accent);
            icons::check(&p, badge, 9.0, pal.on_accent);
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            dirty |= state::choose_wallpaper(&mut self.settings.wallpaper, None);
        }

        // Theme grid, two columns
        let avail = ui.available_width();
        let cols = if avail >= 300.0 { 2 } else { 1 };
        let width = ((avail - gap * (cols as f32 - 1.0)) / cols as f32 - 0.5).floor();
        for row in Theme::ALL.chunks(cols) {
            ui.horizontal(|ui| {
                for &theme in row {
                    let idx = Theme::ALL.iter().position(|t| *t == theme).unwrap_or(0);
                    let selected = self.settings.wallpaper.as_deref() == Some(theme.id());
                    let thumb_h = width * 9.0 / 16.0;
                    let (rect, resp) = ui.allocate_exact_size(vec2(width, thumb_h + 34.0), Sense::click());
                    paint_card_frame(ui, &pal, rect, selected, resp.hovered());
                    let p = ui.painter_at(rect);
                    let img = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(width - 12.0, thumb_h - 6.0 + 0.0));
                    match self.thumbs.tex.get(idx).and_then(|t| t.as_ref()) {
                        Some(tex) => {
                            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                            p.image(tex.id(), img, uv, egui::Color32::WHITE);
                        }
                        None => {
                            p.rect_filled(img, 4, pal.card_hover);
                        }
                    }
                    p.rect_stroke(img, 4, Stroke::new(1.0, pal.border), StrokeKind::Inside);
                    let label = i18n::t(lang, &format!("wallpaper.theme.{}", theme.id()));
                    let color = if selected { pal.accent } else { pal.text };
                    p.text(egui::pos2(rect.center().x, rect.bottom() - 15.0), egui::Align2::CENTER_CENTER, label, FontId::proportional(13.5), color);
                    if selected {
                        let badge = img.right_top() + vec2(-14.0, 14.0);
                        p.circle_filled(badge, 8.0, pal.accent);
                        icons::check(&p, badge, 9.0, pal.on_accent);
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        dirty |= state::choose_wallpaper(&mut self.settings.wallpaper, Some(theme.id()));
                    }
                }
            });
        }

        // Day & night
        ui.add_space(2.0);
        card(ui, &pal, |ui| {
            dirty |= toggle_row(
                ui,
                &pal,
                i18n::t(lang, "wallpaper.day_night"),
                i18n::t(lang, "wallpaper.day_night_hint"),
                &mut self.settings.wallpaper_day_night,
                true,
            );
            if !self.settings.wallpaper_day_night {
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for t in TimeOfDay::ALL {
                        let selected = self.settings.wallpaper_time == t.id();
                        let label = i18n::t(lang, &format!("wallpaper.tod.{}", t.id()));
                        let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(13.0), pal.text);
                        let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 24.0, 28.0), Sense::click());
                        let fill = if selected {
                            pal.accent_soft
                        } else if resp.hovered() {
                            pal.card_hover
                        } else {
                            pal.card
                        };
                        let stroke = if selected { Stroke::new(1.5, pal.accent) } else { Stroke::new(1.0, pal.border) };
                        ui.painter().rect_filled(rect, 14, fill);
                        ui.painter().rect_stroke(rect, 14, stroke, StrokeKind::Inside);
                        let color = if selected { pal.accent } else { pal.text };
                        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !selected {
                            self.settings.wallpaper_time = t.id().to_string();
                            dirty = true;
                        }
                    }
                });
            }
        });
        dirty
    }
}

fn paint_card_frame(ui: &egui::Ui, pal: &super::theme::Palette, rect: Rect, selected: bool, hovered: bool) {
    let p = ui.painter();
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
}
