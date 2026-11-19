//! The egui application: window chrome (sidebar, bottom bar), saving and theming.

use crate::state::{self, Debouncer, Page, ResetConfirm, SavedFlash};
use crate::ui::{icons, theme};
use eframe::egui::{self, vec2, Align, Layout, Margin, RichText, Sense};
use snowy_core::config::Settings;
use snowy_core::{i18n, MonitorInfo};
use std::time::{Duration, Instant};
use crate::ui::theme::RichExt;

pub struct SettingsApp {
    pub(crate) settings: Settings,
    pub(crate) monitors: Vec<MonitorInfo>,
    pub(crate) lang: &'static str,
    /// Language the installed fonts were built for (CJK fonts are loaded on demand).
    font_lang: &'static str,
    pub(crate) windows_locale: Option<String>,
    pub(crate) page: Page,
    pub(crate) pal: theme::Palette,
    /// egui time until which the selected effect card may animate.
    pub(crate) anim_until: f64,
    pub(crate) thumbs: crate::ui::wallpapers::Thumbs,
    debounce: Debouncer,
    reset: ResetConfirm,
    saved: SavedFlash,
    start: Instant,
    auto_close: Option<Duration>,
    page_cycle: Option<Duration>,
    was_focused: bool,
    pub(crate) update: crate::update_ui::UpdateUi,
    pub(crate) friends: crate::ui::friends::FriendsUi,
}

impl SettingsApp {
    pub fn new(
        ctx: &egui::Context,
        settings: Settings,
        monitors: Vec<MonitorInfo>,
        auto_close: Option<Duration>,
        page_cycle: Option<Duration>,
        page: Page,
    ) -> Self {
        let pal = theme::palette(crate::win::system_prefers_dark());
        theme::apply(ctx, &pal);
        let windows_locale = crate::win::windows_locale();
        let lang = state::effective_lang(&settings.language, windows_locale.as_deref());
        Self {
            settings,
            monitors,
            lang,
            font_lang: lang,
            windows_locale,
            page,
            pal,
            anim_until: 2.5,
            thumbs: Default::default(),
            debounce: Debouncer::default(),
            reset: ResetConfirm::default(),
            saved: SavedFlash::default(),
            start: Instant::now(),
            auto_close,
            page_cycle,
            was_focused: true,
            update: Default::default(),
            friends: Default::default(),
        }
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn changed(&mut self) {
        self.settings.clamp();
        self.debounce.mark_dirty();
    }

    fn save_now(&mut self) {
        self.settings.clamp();
        match self.settings.save() {
            Ok(()) => {
                let now = self.now();
                self.saved.mark(now);
            }
            Err(e) => eprintln!("snowy-settings: failed to save: {e:#}"),
        }
        self.debounce.clear();
    }

    fn reload_from_disk(&mut self) {
        let loaded = Settings::load();
        if loaded != self.settings {
            self.settings = loaded;
            self.lang = state::effective_lang(&self.settings.language, self.windows_locale.as_deref());
        }
    }

    fn refresh_theme(&mut self, ctx: &egui::Context) {
        let dark = crate::win::system_prefers_dark();
        if dark != self.pal.dark {
            self.pal = theme::palette(dark);
            theme::apply(ctx, &self.pal);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
            icons::logo(ui.painter(), rect, &pal);
            ui.label(RichText::new(i18n::t(self.lang, "app_name")).size(19.0).semi());
        });
        ui.add_space(14.0);
        for page in Page::ALL {
            let selected = self.page == page;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
            let p = ui.painter();
            if selected {
                // In light mode accent_soft is too close to the sidebar tint: deepen it a bit.
                let sel = if pal.dark { pal.accent_soft } else { pal.accent_soft.lerp_to_gamma(pal.accent, 0.12) };
                p.rect_filled(rect, 9, sel);
                let bar = egui::Rect::from_min_size(rect.left_center() + vec2(0.0, -9.0), vec2(3.5, 18.0));
                p.rect_filled(bar, 2, pal.accent);
            } else if resp.hovered() {
                p.rect_filled(rect, 9, pal.card_hover);
            }
            let color = if selected { pal.accent } else { pal.text };
            p.text(
                rect.left_center() + vec2(16.0, 0.0),
                egui::Align2::LEFT_CENTER,
                i18n::t(self.lang, page.label_key()),
                if selected { crate::ui::theme::semibold(14.5) } else { egui::FontId::proportional(14.5) },
                color,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !selected {
                self.page = page;
                self.reset.cancel();
                if page == Page::Effects {
                    self.anim_until = ui.input(|i| i.time) + 2.5;
                }
            }
        }
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui, now: Duration) -> bool {
        let pal = self.pal;
        let lang = self.lang;
        let mut reset = false;
        ui.horizontal(|ui| {
            let armed = self.reset.is_armed(now);
            let (text, fill) = if armed {
                (RichText::new(i18n::t(lang, "settings.reset_confirm")).semi().color(pal.on_accent), pal.danger)
            } else {
                (RichText::new(i18n::t(lang, "settings.reset")).color(pal.text), pal.card)
            };
            let resp = ui
                .add(egui::Button::new(text).fill(fill).stroke(egui::Stroke::new(1.0, pal.border)))
                .on_hover_text(i18n::t(lang, "settings.reset_hint"));
            if resp.clicked() && self.reset.click(now) {
                reset = true;
            }
            if let Some(left) = self.reset.expires_in(now) {
                ui.ctx().request_repaint_after(left + Duration::from_millis(20));
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
        reset
    }
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::win::tick_window_guard(ui.ctx());
        let ctx = ui.ctx().clone();

        if self.font_lang != self.lang {
            crate::fonts::install(&ctx, self.lang);
            self.font_lang = self.lang;
        }

        // Reload when the window regains focus (tray may have changed the effect).
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && !self.was_focused {
            self.refresh_theme(&ctx);
            if !self.debounce.is_dirty() {
                self.reload_from_disk();
            }
        }
        self.was_focused = focused;

        if self.update.poll(&ctx) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if let Some(interval) = self.page_cycle {
            let idx = (self.start.elapsed().as_secs_f32() / interval.as_secs_f32().max(0.1)) as usize;
            let target = Page::ALL[idx % Page::ALL.len()];
            if target != self.page {
                self.page = target;
                self.anim_until = ctx.input(|i| i.time) + 2.5;
            }
        }

        let pal = self.pal;
        let now = self.now();
        let mut reset = false;
        egui::Panel::bottom("bottom_bar")
            .frame(
                egui::Frame::new()
                    .fill(pal.sidebar)
                    .stroke(egui::Stroke::new(1.0, pal.border))
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| reset = self.bottom_bar(ui, now));
        egui::Panel::left("nav")
            .exact_size(172.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(pal.sidebar).inner_margin(Margin::symmetric(12, 10)))
            .show(ui, |ui| self.sidebar(ui));

        let mut dirty = false;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg).inner_margin(Margin::symmetric(18, 12))).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                dirty = self.show_page(ui);
            });
        });

        if reset {
            self.settings = Settings::default();
            self.lang = state::effective_lang(&self.settings.language, self.windows_locale.as_deref());
            self.anim_until = ctx.input(|i| i.time) + 2.5;
            dirty = true;
        }
        if dirty {
            self.changed();
        }

        let now = self.now();
        if self.debounce.should_save(now) {
            self.save_now();
        } else if let Some(wait) = self.debounce.pending_in(now) {
            ctx.request_repaint_after(wait + Duration::from_millis(5));
        }

        if ctx.input(|i| i.viewport().close_requested()) && self.debounce.is_dirty() {
            self.save_now();
        }

        if let Some(limit) = self.auto_close {
            if self.start.elapsed() >= limit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.debounce.is_dirty() {
            self.save_now();
        }
    }
}
