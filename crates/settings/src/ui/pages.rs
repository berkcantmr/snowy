//! The five settings pages. Each returns true when a setting changed.

use eframe::egui::{self, vec2, RichText, Sense};
use snowy_core::config::Settings;
use snowy_core::{i18n, EffectChoice};
use crate::ui::theme::RichExt;

use super::{card, cards, divider, icons, page_title, percent_slider_row, toggle_row};
use crate::app::SettingsApp;
use crate::state::{self, Page};

impl SettingsApp {
    pub(crate) fn show_page(&mut self, ui: &mut egui::Ui) -> bool {
        match self.page {
            Page::Effects => self.page_effects(ui),
            Page::Appearance => self.page_appearance(ui),
            Page::Desktop => self.page_desktop(ui),
            Page::Wallpapers => self.page_wallpapers(ui),
            Page::System => self.page_system(ui),
            Page::About => self.page_about(ui),
        }
    }

    fn page_effects(&mut self, ui: &mut egui::Ui) -> bool {
        let (lang, pal) = (self.lang, self.pal);
        let mut dirty = false;
        page_title(ui, i18n::t(lang, "page.effects"));

        let gap = 10.0;
        let avail = ui.available_width();
        let cols = if avail >= 330.0 { 3 } else { 2 };
        let width = ((avail - gap * (cols as f32 - 1.0)) / cols as f32 - 0.5).floor();
        let now = ui.input(|i| i.time);
        let animate_selected = now < self.anim_until;
        ui.spacing_mut().item_spacing = vec2(gap, gap);
        for row in state::effect_choices().chunks(cols) {
            ui.horizontal(|ui| {
                for &c in row {
                    let selected = self.settings.effect == c;
                    if cards::effect_card(ui, &pal, lang, c, selected, width, animate_selected).clicked() && !selected {
                        self.settings.effect = c;
                        self.anim_until = now + 2.5;
                        dirty = true;
                    }
                }
            });
        }
        ui.add_space(4.0);

        match self.settings.effect {
            EffectChoice::Random => card(ui, &pal, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(i18n::t(lang, "settings.random_minutes")).semi());
                    let r = ui.add(egui::DragValue::new(&mut self.settings.random_minutes).range(1..=24 * 60).speed(0.5));
                    dirty |= r.changed();
                });
                ui.label(RichText::new(i18n::t(lang, "settings.random_hint")).small().color(pal.text_weak));
            }),
            EffectChoice::Season => card(ui, &pal, |ui| {
                let kind = state::season_effect(state::current_month());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(i18n::t(lang, "settings.season_now")).color(pal.text_weak));
                    let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                    icons::paint_choice(ui.painter(), rect, EffectChoice::Fixed(kind), 0.0, &pal);
                    ui.label(RichText::new(i18n::t(lang, &i18n::effect_key(kind))).semi().color(pal.accent));
                });
            }),
            EffectChoice::Weather => card(ui, &pal, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(i18n::t(lang, "settings.weather_now")).color(pal.text_weak));
                    match snowy_weather::cached() {
                        Some((cond, _)) => {
                            match snowy_weather::effect_for(&cond, state::current_month()) {
                                Some(kind) => {
                                    let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                                    icons::paint_choice(ui.painter(), rect, EffectChoice::Fixed(kind), 0.0, &pal);
                                    ui.label(RichText::new(i18n::t(lang, &i18n::effect_key(kind))).semi().color(pal.accent));
                                }
                                None => {
                                    ui.label(RichText::new(i18n::t(lang, "effect.off")).semi().color(pal.accent));
                                }
                            }
                            ui.label(RichText::new(format!("{:.0}\u{00b0}C", cond.temperature)).color(pal.text_weak));
                        }
                        None => {
                            ui.label(RichText::new(i18n::t(lang, "settings.weather_unknown")).color(pal.text_weak));
                        }
                    }
                });
                ui.label(RichText::new(i18n::t(lang, "settings.weather_hint")).small().color(pal.text_weak));
            }),
            _ => {}
        }
        dirty
    }

    fn page_appearance(&mut self, ui: &mut egui::Ui) -> bool {
        let (lang, pal) = (self.lang, self.pal);
        let mut dirty = false;
        page_title(ui, i18n::t(lang, "page.appearance"));
        card(ui, &pal, |ui| {
            dirty |= percent_slider_row(
                ui,
                &pal,
                lang,
                i18n::t(lang, "settings.intensity"),
                i18n::t(lang, "settings.intensity_hint"),
                &mut self.settings.intensity,
                0.25..=2.0,
            );
            divider(ui, &pal);
            dirty |= percent_slider_row(
                ui,
                &pal,
                lang,
                i18n::t(lang, "settings.speed"),
                i18n::t(lang, "settings.speed_hint"),
                &mut self.settings.speed,
                0.5..=2.0,
            );
            divider(ui, &pal);
            dirty |= toggle_row(
                ui,
                &pal,
                i18n::t(lang, "settings.changing_wind"),
                i18n::t(lang, "settings.changing_wind_hint"),
                &mut self.settings.changing_wind,
                true,
            );
            divider(ui, &pal);
            ui.label(RichText::new(i18n::t(lang, "settings.max_fps")).semi());
            ui.label(RichText::new(i18n::t(lang, "settings.max_fps_hint")).small().color(pal.text_weak));
            ui.horizontal(|ui| {
                for f in state::FPS_CHOICES {
                    let on = self.settings.max_fps == f;
                    let text = if on { RichText::new(f.to_string()).color(pal.on_accent).semi() } else { RichText::new(f.to_string()) };
                    let b = egui::Button::new(text).fill(if on { pal.accent } else { pal.card_hover }).min_size(vec2(56.0, 28.0));
                    if ui.add(b).clicked() && !on {
                        self.settings.max_fps = f;
                        dirty = true;
                    }
                }
            });
        });
        dirty
    }

    fn page_desktop(&mut self, ui: &mut egui::Ui) -> bool {
        let (lang, pal) = (self.lang, self.pal);
        let mut dirty = false;
        page_title(ui, i18n::t(lang, "page.desktop"));
        card(ui, &pal, |ui| {
            dirty |= toggle_row(
                ui,
                &pal,
                i18n::t(lang, "settings.behind_windows"),
                i18n::t(lang, "settings.behind_windows_hint"),
                &mut self.settings.behind_windows,
                true,
            );
            divider(ui, &pal);
            let behind = self.settings.behind_windows;
            let hint = if behind {
                i18n::t(lang, "settings.keep_windows_clear_disabled")
            } else {
                i18n::t(lang, "settings.keep_windows_clear_hint")
            };
            dirty |= toggle_row(ui, &pal, i18n::t(lang, "settings.keep_windows_clear"), hint, &mut self.settings.keep_windows_clear, !behind);
            divider(ui, &pal);
            dirty |= toggle_row(
                ui,
                &pal,
                i18n::t(lang, "settings.snowballs"),
                i18n::t(lang, "settings.snowballs_hint"),
                &mut self.settings.snowballs,
                true,
            );
        });
        dirty
    }

    fn page_system(&mut self, ui: &mut egui::Ui) -> bool {
        let (lang, pal) = (self.lang, self.pal);
        let mut dirty = false;
        page_title(ui, i18n::t(lang, "page.system"));
        card(ui, &pal, |ui| {
            dirty |= toggle_row(
                ui,
                &pal,
                i18n::t(lang, "settings.pause_on_battery"),
                i18n::t(lang, "settings.pause_on_battery_hint"),
                &mut self.settings.pause_on_battery,
                true,
            );
            divider(ui, &pal);
            dirty |= toggle_row(
                ui,
                &pal,
                i18n::t(lang, "settings.pause_on_fullscreen"),
                i18n::t(lang, "settings.pause_on_fullscreen_hint"),
                &mut self.settings.pause_on_fullscreen,
                true,
            );
            divider(ui, &pal);
            dirty |= toggle_row(ui, &pal, i18n::t(lang, "settings.start_with_windows"), "", &mut self.settings.start_with_windows, true);
            divider(ui, &pal);
            dirty |= toggle_row(
                ui,
                &pal,
                i18n::t(lang, "settings.check_updates"),
                i18n::t(lang, "settings.check_updates_hint"),
                &mut self.settings.check_updates,
                true,
            );
        });
        // Same update state as the About page; checking here is manual only.
        let ctx = ui.ctx().clone();
        self.update_card(ui, &ctx);

        card(ui, &pal, |ui| {
            ui.label(RichText::new(i18n::t(lang, "settings.monitors")).semi());
            ui.label(RichText::new(i18n::t(lang, "settings.monitors_hint")).small().color(pal.text_weak));
            if self.monitors.is_empty() {
                ui.label(RichText::new(i18n::t(lang, "settings.no_monitors")).color(pal.text_weak));
            }
            for row in state::monitor_rows(&self.monitors, &self.settings.disabled_monitors, lang) {
                divider(ui, &pal);
                let mut on = row.enabled;
                if toggle_row(ui, &pal, &row.label, "", &mut on, true)
                    && state::set_monitor_enabled(&mut self.settings.disabled_monitors, &self.monitors, &row.id, on)
                {
                    dirty = true;
                }
            }
        });

        card(ui, &pal, |ui| {
            ui.label(RichText::new(i18n::t(lang, "settings.language")).semi());
            let current = state::normalize_language(&self.settings.language);
            let mut picked = current;
            egui::ComboBox::from_id_salt("language").selected_text(state::language_name(current, lang)).show_ui(ui, |ui| {
                ui.selectable_value(&mut picked, "", state::language_name("", lang));
                for (code, name) in i18n::LANGS {
                    ui.selectable_value(&mut picked, *code, *name);
                }
            });
            if picked != current {
                self.settings.language = picked.to_string();
                self.lang = state::effective_lang(picked, self.windows_locale.as_deref());
                dirty = true;
            }
        });
        dirty
    }

    fn page_about(&mut self, ui: &mut egui::Ui) -> bool {
        let (lang, pal) = (self.lang, self.pal);
        page_title(ui, i18n::t(lang, "page.about"));
        let ctx = ui.ctx().clone();
        self.update.auto_check(&ctx, self.settings.check_updates);
        card(ui, &pal, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::hover());
                icons::logo(ui.painter(), rect, &pal);
                ui.vertical(|ui| {
                    ui.label(RichText::new(i18n::t(lang, "app_name")).size(26.0).semi());
                    ui.label(
                        RichText::new(format!("{} {}", i18n::t(lang, "settings.version"), env!("CARGO_PKG_VERSION"))).color(pal.text_weak),
                    );
                });
            });
            ui.add_space(4.0);
            ui.label(i18n::t(lang, "settings.about_desc"));
        });
        self.update_card(ui, &ctx);
        card(ui, &pal, |ui| {
            ui.label(RichText::new(i18n::t(lang, "settings.settings_file")).semi());
            let path = Settings::path();
            ui.add(egui::Label::new(RichText::new(path.display().to_string()).small().monospace().color(pal.text_weak)).wrap());
            if ui.button(i18n::t(lang, "settings.open_folder")).clicked() {
                let dir = Settings::dir();
                let _ = std::fs::create_dir_all(&dir);
                crate::win::open_folder(&dir);
            }
        });
        false
    }

    /// Version, update status, progress and the check / update buttons.
    fn update_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use crate::update_ui::{format_mb, Status};
        let (lang, pal) = (self.lang, self.pal);
        let version = self.update.release.as_ref().map(|r| r.version.clone()).unwrap_or_default();
        card(ui, &pal, |ui| {
            let (text, color) = match &self.update.status {
                Status::Idle => (i18n::t(lang, "update.idle").to_string(), pal.text_weak),
                Status::Checking => (i18n::t(lang, "update.checking").to_string(), pal.text_weak),
                Status::UpToDate => (i18n::t(lang, "update.up_to_date").to_string(), pal.text_weak),
                Status::Available => (i18n::t(lang, "update.available").replace("{v}", &version), pal.accent),
                Status::Downloading { .. } => (i18n::t(lang, "update.downloading").to_string(), pal.text),
                Status::Installing(_) => (i18n::t(lang, "update.installing").to_string(), pal.accent),
                Status::Failed(_) => (i18n::t(lang, "update.failed").to_string(), pal.danger),
            };
            ui.add(egui::Label::new(RichText::new(text).semi().color(color)).wrap());
            match &self.update.status {
                Status::Failed(detail) => {
                    ui.add(egui::Label::new(RichText::new(detail).small().color(pal.danger)).wrap());
                }
                Status::Downloading { done, total } => {
                    let frac = if *total > 0 { (*done as f32 / *total as f32).clamp(0.0, 1.0) } else { 0.0 };
                    let label = if *total > 0 {
                        format!("{} / {}", format_mb(*done), format_mb(*total))
                    } else {
                        format_mb(*done)
                    };
                    ui.add(egui::ProgressBar::new(frac).text(label).fill(pal.accent).desired_height(20.0));
                }
                _ => {}
            }
            if let (Status::Available, Some(r)) = (&self.update.status, &self.update.release) {
                let notes = r.notes.trim();
                if !notes.is_empty() {
                    divider(ui, &pal);
                    ui.label(RichText::new(i18n::t(lang, "update.whats_new")).semi());
                    egui::ScrollArea::vertical().max_height(110.0).id_salt("update_notes").show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(notes).small().color(pal.text_weak)).wrap());
                    });
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let idle = !self.update.busy();
                if self.update.release.is_some() && !matches!(self.update.status, Status::Checking | Status::UpToDate) {
                    let b = egui::Button::new(RichText::new(i18n::t(lang, "update.update_now")).semi().color(pal.on_accent))
                        .fill(pal.accent)
                        .min_size(vec2(0.0, 30.0));
                    if ui.add_enabled(idle, b).clicked() {
                        self.update.install(ctx);
                    }
                }
                let b = egui::Button::new(i18n::t(lang, "update.check_now")).min_size(vec2(0.0, 30.0));
                if ui.add_enabled(idle, b).clicked() {
                    self.update.check(ctx);
                }
            });
        });
    }
}
