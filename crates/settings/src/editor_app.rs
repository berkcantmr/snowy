//! The prop look editor window: live preview, per-slot option thumbnails, size, delete.

use crate::editor_logic as logic;
use crate::state::{Debouncer, ResetConfirm, SavedFlash};
use crate::ui::art_paint::{paint_prims, Transform};
use crate::ui::{icons, percent_slider_row, theme};
use eframe::egui::{self, pos2, vec2, Align, Color32, CornerRadius, FontId, Layout, Margin, Mesh, Rect, RichText, Sense, Shape, Stroke, StrokeKind};
use snowy_core::props::{Prop, PropKind, PropsFile};
use snowy_core::i18n;
use std::time::{Duration, Instant};
use crate::ui::theme::RichExt;

const PREVIEW_H: f32 = 196.0;
const CARD_H: f32 = 100.0;
const THUMB_H: f32 = 68.0;

pub struct EditorApp {
    monitor_id: String,
    prop: Option<Prop>,
    lang: &'static str,
    pal: theme::Palette,
    slot: usize,
    debounce: Debouncer,
    delete: ResetConfirm,
    saved: SavedFlash,
    start: Instant,
    auto_close: Option<Duration>,
    rng: logic::Rng,
    deleted: bool,
    was_focused: bool,
}

/// What the user did in one frame.
#[derive(Default)]
struct Actions {
    changed: bool,
    random: bool,
    delete: bool,
    done: bool,
}

impl EditorApp {
    pub fn new(ctx: &egui::Context, monitor_id: String, prop: Option<Prop>, lang: &'static str, auto_close: Option<Duration>) -> Self {
        let pal = theme::palette(crate::win::system_prefers_dark());
        theme::apply(ctx, &pal);
        Self {
            monitor_id,
            prop,
            lang,
            pal,
            slot: 0,
            debounce: Debouncer::default(),
            delete: ResetConfirm::default(),
            saved: SavedFlash::default(),
            start: Instant::now(),
            auto_close,
            rng: logic::Rng::from_time(),
            deleted: false,
            was_focused: true,
        }
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// Reloads the file, applies only this prop's look/balls/scale and writes it back.
    fn save_now(&mut self) {
        self.debounce.clear();
        if self.deleted {
            return;
        }
        let Some(prop) = &self.prop else { return };
        let mut file = PropsFile::load(&self.monitor_id);
        if logic::merge_edit(&mut file, prop) {
            match file.save(&self.monitor_id) {
                Ok(()) => {
                    let now = self.now();
                    self.saved.mark(now);
                }
                Err(e) => eprintln!("snowy-settings: failed to save props: {e:#}"),
            }
        }
    }

    fn delete_now(&mut self) {
        let Some(prop) = &self.prop else { return };
        let mut file = PropsFile::load(&self.monitor_id);
        if logic::remove_prop(&mut file, prop.id) {
            if let Err(e) = file.save(&self.monitor_id) {
                eprintln!("snowy-settings: failed to save props: {e:#}");
            }
        }
        self.deleted = true;
        self.debounce.clear();
    }

    fn not_found(&self, ui: &mut egui::Ui) -> bool {
        let pal = self.pal;
        let mut close = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg).inner_margin(Margin::same(18))).show(ui, |ui| {
            ui.label(RichText::new(i18n::t(self.lang, "editor.not_found")).heading().semi());
            ui.label(RichText::new(i18n::t(self.lang, "editor.not_found_hint")).color(pal.text_weak));
            ui.add_space(12.0);
            let ok = egui::Button::new(RichText::new(i18n::t(self.lang, "editor.done")).semi().color(pal.on_accent)).fill(pal.accent);
            if ui.add(ok).clicked() {
                close = true;
            }
        });
        close
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui, act: &mut Actions) {
        let pal = self.pal;
        let lang = self.lang;
        let now = self.now();
        ui.horizontal(|ui| {
            let armed = self.delete.is_armed(now);
            let (text, fill) = if armed {
                (RichText::new(i18n::t(lang, "editor.delete_confirm")).semi().color(pal.on_accent), pal.danger)
            } else {
                (RichText::new(i18n::t(lang, "editor.delete")).color(pal.danger), pal.card)
            };
            let resp = ui.add(egui::Button::new(text).fill(fill).stroke(Stroke::new(1.0, pal.border)));
            if resp.clicked() && self.delete.click(now) {
                act.delete = true;
            }
            if let Some(left) = self.delete.expires_in(now) {
                ui.ctx().request_repaint_after(left + Duration::from_millis(20));
            }
            let rnd = egui::Button::new(RichText::new(i18n::t(lang, "editor.random")).color(pal.text)).fill(pal.card).stroke(Stroke::new(1.0, pal.border));
            if ui.add(rnd).clicked() {
                act.random = true;
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let done = egui::Button::new(RichText::new(i18n::t(lang, "editor.done")).semi().color(pal.on_accent)).fill(pal.accent);
                if ui.add(done).clicked() {
                    act.done = true;
                }
                let alpha = self.saved.alpha(now);
                if alpha > 0.0 {
                    let color = pal.accent.gamma_multiply(alpha);
                    ui.label(RichText::new(i18n::t(lang, "settings.saved")).color(color));
                    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    icons::check(ui.painter(), rect.center(), 15.0, color);
                    ui.ctx().request_repaint_after(Duration::from_millis(50));
                }
            });
        });
    }

    fn sky_colors(&self) -> (Color32, Color32, Color32) {
        if self.pal.dark {
            (Color32::from_rgb(26, 36, 62), Color32::from_rgb(70, 92, 130), Color32::from_rgb(196, 210, 230))
        } else {
            (Color32::from_rgb(138, 180, 226), Color32::from_rgb(226, 240, 252), Color32::from_rgb(246, 250, 254))
        }
    }

    fn preview(&self, ui: &mut egui::Ui, kind: &PropKind, scale: f32, t: f32) {
        let lights = if let PropKind::Lights { look, .. } = kind { Some(*look) } else { None };
        let (top, bottom, ground) = self.sky_colors();
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), PREVIEW_H), Sense::hover());
        let p = ui.painter_at(rect);
        let ground_y = rect.top() + rect.height() * 0.84;
        let r = 12u8;

        p.rect_filled(rect, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, top);
        let (y0, y1) = (rect.top() + r as f32, ground_y);
        let mut mesh = Mesh::default();
        let c0 = top.lerp_to_gamma(bottom, r as f32 / (y1 - rect.top()));
        mesh.colored_vertex(pos2(rect.left(), y0), c0);
        mesh.colored_vertex(pos2(rect.right(), y0), c0);
        mesh.colored_vertex(pos2(rect.right(), y1), bottom);
        mesh.colored_vertex(pos2(rect.left(), y1), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(Shape::mesh(mesh));
        p.rect_filled(
            Rect::from_min_max(pos2(rect.left(), ground_y), rect.max),
            CornerRadius { nw: 0, ne: 0, sw: r, se: r },
            ground,
        );
        p.line_segment([pos2(rect.left(), ground_y), pos2(rect.right(), ground_y)], Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.6)));

        // Drifting snow dots (deterministic positions, moving with time).
        for i in 0..42u32 {
            let f = |k: f32| ((i as f32 * k).fract() + 1.0).fract();
            let speed = 0.03 + 0.05 * f(0.37);
            let x = rect.left() + rect.width() * ((f(0.618) + (t * 0.4 + i as f32).sin() * 0.01).clamp(0.0, 1.0));
            let y = rect.top() + (ground_y - rect.top()) * ((f(0.7548777) + t * speed).fract());
            p.circle_filled(pos2(x, y), 1.0 + 1.6 * f(0.73), Color32::WHITE.gamma_multiply(0.55 + 0.35 * f(0.29)));
        }

        if let Some(look) = lights {
            // The string hangs across the whole preview.
            let (a, b) = ([rect.left() + 26.0, rect.top() + 36.0], [rect.right() - 26.0, rect.top() + 46.0]);
            let len = (b[0] - a[0]).hypot(b[1] - a[1]);
            let mut prims = Vec::new();
            snowy_art::lights_scaled(&look, a, b, 0.07 * len, t, 0.0, 1.1, &mut prims);
            paint_prims(&p, &prims, &Transform { scale: 1.0, offset: vec2(0.0, 0.0) });
            return;
        }

        // The prop itself, standing on the ground.
        let base_h = if logic::is_tree(kind) { 150.0 } else { 118.0 };
        let max_h = ground_y - rect.top() - 6.0;
        let height = (base_h * scale).min(max_h);
        let cx = rect.center().x;
        let shadow = Color32::from_rgba_unmultiplied(60, 80, 120, 40);
        p.add(Shape::ellipse_filled(pos2(cx, ground_y + 6.0), vec2(height * 0.34, 5.0), shadow));
        let prims = logic::build_prims(kind, [cx, ground_y + 6.0], height, t);
        paint_prims(&p, &prims, &Transform { scale: 1.0, offset: vec2(0.0, 0.0) });
    }

    fn pill(&self, ui: &mut egui::Ui, size: egui::Vec2, label: &str, selected: bool) -> egui::Response {
        let pal = self.pal;
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let fill = if selected {
            pal.accent_soft
        } else if resp.hovered() {
            pal.card_hover
        } else {
            pal.card
        };
        let stroke = if selected { Stroke::new(1.5, pal.accent) } else { Stroke::new(1.0, pal.border) };
        let p = ui.painter();
        p.rect_filled(rect, 9, fill);
        p.rect_stroke(rect, 9, stroke, StrokeKind::Inside);
        let color = if selected { pal.accent } else { pal.text };
        p.text(rect.center(), egui::Align2::CENTER_CENTER, label, if selected { crate::ui::theme::semibold(14.0) } else { FontId::proportional(14.0) }, color);
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    #[allow(clippy::too_many_arguments)]
    fn option_card(&self, ui: &mut egui::Ui, width: f32, kind: &PropKind, variant: &PropKind, slot: usize, option: u8, selected: bool) -> egui::Response {
        let pal = self.pal;
        let (rect, resp) = ui.allocate_exact_size(vec2(width, CARD_H), Sense::click());
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

        // Thumbnail: the prop with only this option changed, cropped to the slot's area.
        let thumb = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(width - 12.0, THUMB_H));
        let (_, mid, _) = self.sky_colors();
        p.rect_filled(thumb, 8, mid);
        let tp = p.with_clip_rect(thumb);
        let base_h = if logic::is_tree(kind) { 140.0 } else { 100.0 };
        let tt = if logic::is_lights(kind) { ui.ctx().input(|i| i.time) as f32 } else { 0.0 };
        let prims = logic::build_prims(variant, [0.0, 0.0], base_h, tt);
        let crop = logic::crop_bounds(snowy_art::bounds(&prims), logic::thumb_crop(kind, slot));
        paint_prims(&tp, &prims, &Transform::fit(crop, thumb.shrink(5.0)));

        let key = format!("look.{}.{}", logic::slot_names(kind)[slot], option);
        let label = i18n::t(self.lang, &key);
        let color = if selected { pal.accent } else { pal.text };
        let galley = p.layout(label.to_owned(), if selected { crate::ui::theme::semibold(12.5) } else { FontId::proportional(12.5) }, color, width - 10.0);
        let pos = pos2(rect.center().x - galley.size().x / 2.0, thumb.bottom() + 4.0 + ((CARD_H - THUMB_H - 12.0 - galley.size().y) / 2.0).max(0.0));
        p.galley(pos, galley, color);

        if selected {
            let badge = rect.right_top() + vec2(-14.0, 14.0);
            p.circle_filled(badge, 8.0, pal.accent);
            icons::check(&p, badge, 9.0, pal.on_accent);
        }
        resp.on_hover_text(label).on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    fn body(&mut self, ui: &mut egui::Ui, t: f32, act: &mut Actions) {
        let pal = self.pal;
        let lang = self.lang;
        let Some(mut prop) = self.prop else { return };

        self.preview(ui, &prop.kind, prop.scale, t);
        ui.add_space(4.0);

        // Slot tabs.
        let names = logic::slot_names(&prop.kind);
        let gap = 6.0;
        let tab_w = (ui.available_width() - 3.0 * gap) / 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (i, name) in names.iter().enumerate() {
                let label = i18n::t(lang, &format!("editor.slot.{name}"));
                if self.pill(ui, vec2(tab_w, 32.0), label, self.slot == i).clicked() {
                    self.slot = i;
                }
            }
        });
        ui.add_space(2.0);

        // 8 option thumbnails (4 x 2).
        let slot = self.slot.min(3);
        let current = logic::get_slot(&prop.kind, slot);
        let card_w = (ui.available_width() - 3.0 * 8.0) / 4.0;
        let variants = logic::thumb_variants(&prop.kind, slot);
        let mut pick = None;
        for row in 0..2u8 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for col in 0..4u8 {
                    let option = row * 4 + col;
                    if self.option_card(ui, card_w, &prop.kind, &variants[option as usize], slot, option, option == current).clicked() {
                        pick = Some(option);
                    }
                }
            });
        }
        if let Some(o) = pick {
            if o != current {
                prop.kind = logic::with_slot(&prop.kind, slot, o);
                act.changed = true;
            }
        }
        ui.add_space(2.0);

        // Controls card.
        crate::ui::card(ui, &pal, |ui| {
            if let PropKind::Snowman { balls, .. } = prop.kind {
                ui.horizontal(|ui| {
                    ui.label(i18n::t(lang, "editor.balls"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for n in [3u8, 2] {
                            let label = i18n::t(lang, if n == 3 { "editor.balls_3" } else { "editor.balls_2" });
                            if self.pill(ui, vec2(84.0, 28.0), label, balls == n).clicked() && balls != n {
                                prop.kind = logic::with_balls(&prop.kind, n);
                                act.changed = true;
                            }
                        }
                    });
                });
                crate::ui::divider(ui, &pal);
            }
            if logic::is_lights(&prop.kind) {
                ui.label(RichText::new(i18n::t(lang, "editor.lights_hint")).color(pal.text_weak));
            } else if percent_slider_row(ui, &pal, lang, i18n::t(lang, "editor.size"), "", &mut prop.scale, logic::SCALE_MIN..=logic::SCALE_MAX) {
                prop.scale = logic::clamp_scale(prop.scale);
                act.changed = true;
            }
        });

        if act.random {
            prop.kind = logic::random_kind(&prop.kind, &mut self.rng);
            act.changed = true;
        }
        self.prop = Some(prop);
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::win::tick_window_guard(ui.ctx());
        let ctx = ui.ctx().clone();
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && !self.was_focused {
            let dark = crate::win::system_prefers_dark();
            if dark != self.pal.dark {
                self.pal = theme::palette(dark);
                theme::apply(&ctx, &self.pal);
            }
        }
        self.was_focused = focused;

        if self.prop.is_none() {
            if self.not_found(ui) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            self.tick_auto_close(&ctx);
            return;
        }

        let pal = self.pal;
        let t = ctx.input(|i| i.time) as f32;
        let mut act = Actions::default();
        egui::Panel::bottom("editor_bottom")
            .frame(egui::Frame::new().fill(pal.sidebar).stroke(Stroke::new(1.0, pal.border)).inner_margin(Margin::symmetric(16, 10)))
            .show(ui, |ui| self.bottom_bar(ui, &mut act));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg).inner_margin(Margin::symmetric(16, 12))).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.body(ui, t, &mut act));
        });

        if act.changed {
            self.debounce.mark_dirty();
        }
        if act.delete {
            self.delete_now();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let now = self.now();
        if act.done || ctx.input(|i| i.viewport().close_requested()) {
            if self.debounce.is_dirty() {
                self.save_now();
            }
            if act.done {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if self.debounce.should_save(now) {
            self.save_now();
        } else if let Some(wait) = self.debounce.pending_in(now) {
            ctx.request_repaint_after(wait + Duration::from_millis(5));
        }

        // Animate the preview (~30 fps) only while the window is focused; idle otherwise.
        if focused {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        self.tick_auto_close(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.debounce.is_dirty() {
            self.save_now();
        }
    }
}

impl EditorApp {
    fn tick_auto_close(&self, ctx: &egui::Context) {
        if let Some(limit) = self.auto_close {
            if self.start.elapsed() >= limit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }
    }
}
