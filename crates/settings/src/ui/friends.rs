//! The Friends page. The window never talks to the server: it reads `friends.json`, queues
//! commands for the main app and keeps nicknames in `friend-names.json`.

use eframe::egui::{self, vec2, Align, Color32, Key, Layout, RichText, Sense};
use snowy_core::config::Settings;
use snowy_core::i18n;
use snowy_net::state::{self as net, Action, Command, FriendsState, Perms, ShareStatus, SharedState, Status};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use super::theme::RichExt;
use super::{card, divider, page_title, toggle_row};
use crate::app::SettingsApp;

const POLL: Duration = Duration::from_millis(250);
const CONFIRM: Duration = Duration::from_secs(3);
const GREEN: Color32 = Color32::from_rgb(70, 185, 100);

/// Everything the page remembers between frames.
pub struct FriendsUi {
    dir: PathBuf,
    state: FriendsState,
    names: BTreeMap<String, String>,
    /// What each friend may do here (friend-perms.json).
    perms: BTreeMap<String, Perms>,
    /// Shared desktop state (shared.json, written by the app).
    shared: SharedState,
    shared_mtime: Option<SystemTime>,
    mtime: Option<SystemTime>,
    polled: Option<Instant>,
    add_text: String,
    add_invalid: bool,
    /// Friend being renamed and the text typed so far.
    editing: Option<(String, String)>,
    /// Friend whose Remove button was clicked once, and when.
    confirm: Option<(String, Instant)>,
    copied_at: Option<Instant>,
}

impl Default for FriendsUi {
    fn default() -> Self {
        let dir = Settings::dir();
        Self {
            state: net::load_state(&dir),
            names: net::load_names(&dir),
            perms: net::load_perms(&dir),
            shared: net::load_shared(&dir),
            shared_mtime: None,
            mtime: None,
            polled: None,
            dir,
            add_text: String::new(),
            add_invalid: false,
            editing: None,
            confirm: None,
            copied_at: None,
        }
    }
}

impl FriendsUi {
    /// Re-reads friends.json when it changed (checked at most every 250 ms).
    fn poll(&mut self) {
        if self.polled.is_some_and(|t| t.elapsed() < POLL) {
            return;
        }
        self.polled = Some(Instant::now());
        let m = std::fs::metadata(net::state_path(&self.dir)).and_then(|m| m.modified()).ok();
        if m != self.mtime {
            self.mtime = m;
            self.state = net::load_state(&self.dir);
        }
        let m = std::fs::metadata(net::shared_path(&self.dir)).and_then(|m| m.modified()).ok();
        if m != self.shared_mtime {
            self.shared_mtime = m;
            self.shared = net::load_shared(&self.dir);
        }
    }

    fn send(&self, cmd: Command) {
        if let Err(e) = net::send_command(&self.dir, &cmd) {
            eprintln!("snowy-settings: could not queue friends command: {e}");
        }
    }

    fn set_perms(&mut self, code: &str, p: Perms) {
        if p == Perms::default() {
            self.perms.remove(code);
        } else {
            self.perms.insert(code.to_string(), p);
        }
        if let Err(e) = net::save_perms(&self.dir, &self.perms) {
            eprintln!("snowy-settings: could not save friend permissions: {e}");
        }
    }

    fn set_name(&mut self, code: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.names.remove(code);
        } else {
            self.names.insert(code.to_string(), name.chars().take(32).collect());
        }
        if let Err(e) = net::save_names(&self.dir, &self.names) {
            eprintln!("snowy-settings: could not save nicknames: {e}");
        }
    }
}

/// i18n key for an error id from the server.
fn error_key(id: &str) -> String {
    match id {
        "unknown_code" | "own_code" | "not_friends" | "offline" | "too_many" | "too_large" | "bad_message" | "not_logged_in"
        | "bad_signature" | "slow_down" => format!("friends.err.{id}"),
        _ => "friends.err.other".to_string(),
    }
}

fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.0, color);
}

impl SettingsApp {
    pub(crate) fn page_friends(&mut self, ui: &mut egui::Ui) -> bool {
        let (lang, pal) = (self.lang, self.pal);
        let mut dirty = false;
        page_title(ui, i18n::t(lang, "page.friends"));
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(POLL);
        let f = &mut self.friends;
        f.poll();

        card(ui, &pal, |ui| {
            dirty |= toggle_row(
                ui,
                &pal,
                i18n::t(lang, "friends.toggle"),
                i18n::t(lang, "friends.toggle_hint"),
                &mut self.settings.friends,
                true,
            );
            if self.settings.friends {
                divider(ui, &pal);
                let (key, color) = match f.state.status {
                    Status::Online => ("friends.status.online", GREEN),
                    Status::Retrying => ("friends.status.retrying", pal.danger),
                    _ => ("friends.status.connecting", pal.text_weak),
                };
                ui.horizontal(|ui| {
                    dot(ui, color);
                    ui.label(RichText::new(i18n::t(lang, key)).color(color));
                });
            }
        });
        if !self.settings.friends {
            ui.add(egui::Label::new(RichText::new(i18n::t(lang, "friends.off_note")).color(pal.text_weak)).wrap());
            return dirty;
        }

        // Shared desktop: a request, a pending invite or an active share.
        if let (Some(with), true) = (f.shared.with.clone(), f.shared.status != ShareStatus::None) {
            let name = f.names.get(&with).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| with.clone());
            card(ui, &pal, |ui| {
                ui.label(RichText::new(i18n::t(lang, "friends.share_title")).semi());
                let key = match f.shared.status {
                    ShareStatus::Asked => "friends.share_asked",
                    ShareStatus::Invited => "friends.share_waiting",
                    _ => "friends.share_active",
                };
                ui.add(egui::Label::new(i18n::t(lang, key).replace("{name}", &name)).wrap());
                ui.label(RichText::new(i18n::t(lang, "friends.share_hint")).small().color(pal.text_weak));
                ui.horizontal(|ui| match f.shared.status {
                    ShareStatus::Asked => {
                        let ok = egui::Button::new(RichText::new(i18n::t(lang, "friends.accept")).color(pal.on_accent)).fill(pal.accent);
                        if ui.add(ok.min_size(vec2(0.0, 28.0))).clicked() {
                            f.send(Command::ShareAccept { code: with.clone() });
                        }
                        if ui.add(egui::Button::new(i18n::t(lang, "friends.decline")).min_size(vec2(0.0, 28.0))).clicked() {
                            f.send(Command::ShareDecline { code: with.clone() });
                        }
                    }
                    ShareStatus::Invited => {
                        if ui.add(egui::Button::new(i18n::t(lang, "friends.cancel")).min_size(vec2(0.0, 28.0))).clicked() {
                            f.send(Command::ShareStop);
                        }
                    }
                    _ => {
                        if ui.add(egui::Button::new(i18n::t(lang, "friends.share_stop")).min_size(vec2(0.0, 28.0))).clicked() {
                            f.send(Command::ShareStop);
                        }
                    }
                });
            });
        }

        // Your code.
        card(ui, &pal, |ui| {
            ui.label(RichText::new(i18n::t(lang, "friends.your_code")).semi());
            ui.horizontal(|ui| {
                let code = if f.state.code.is_empty() { "\u{2026}" } else { f.state.code.as_str() };
                ui.label(RichText::new(code).size(24.0).monospace().color(pal.accent));
                if !f.state.code.is_empty() {
                    let copied = f.copied_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2));
                    let text = i18n::t(lang, if copied { "friends.copied" } else { "friends.copy" });
                    if ui.add(egui::Button::new(text).min_size(vec2(0.0, 28.0))).clicked() {
                        ctx.copy_text(f.state.code.clone());
                        f.copied_at = Some(Instant::now());
                    }
                }
            });
            ui.label(RichText::new(i18n::t(lang, "friends.your_code_hint")).small().color(pal.text_weak));
        });

        // Add a friend.
        card(ui, &pal, |ui| {
            ui.label(RichText::new(i18n::t(lang, "friends.add_title")).semi());
            ui.label(RichText::new(i18n::t(lang, "friends.add_hint")).small().color(pal.text_weak));
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 90.0).max(80.0);
                let edit = egui::TextEdit::singleline(&mut f.add_text).desired_width(w).font(egui::TextStyle::Monospace).char_limit(24);
                let r = ui.add_sized(vec2(w, 28.0), edit);
                if r.changed() {
                    f.add_invalid = false;
                }
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                let b = egui::Button::new(RichText::new(i18n::t(lang, "friends.add")).semi().color(pal.on_accent))
                    .fill(pal.accent)
                    .min_size(vec2(0.0, 28.0));
                if (ui.add(b).clicked() || enter) && !f.add_text.trim().is_empty() {
                    match snowy_net::identity::normalize_code(&f.add_text) {
                        Some(code) => {
                            f.send(Command::Add { code });
                            f.add_text.clear();
                            f.add_invalid = false;
                        }
                        None => f.add_invalid = true,
                    }
                }
            });
            let msg = if f.add_invalid {
                Some(i18n::t(lang, "friends.invalid_code"))
            } else {
                f.state.last_error.as_deref().map(|e| i18n::t(lang, &error_key(e)))
            };
            if let Some(m) = msg {
                ui.add(egui::Label::new(RichText::new(m).small().color(pal.danger)).wrap());
            }
        });

        // Incoming and sent requests.
        if !f.state.requests.is_empty() {
            card(ui, &pal, |ui| {
                ui.label(RichText::new(i18n::t(lang, "friends.requests")).semi());
                for code in f.state.requests.clone() {
                    divider(ui, &pal);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&code).monospace());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let b = egui::Button::new(RichText::new(i18n::t(lang, "friends.accept")).semi().color(pal.on_accent))
                                .fill(pal.accent)
                                .min_size(vec2(0.0, 26.0));
                            if ui.add(b).clicked() {
                                f.send(Command::Accept { code: code.clone() });
                            }
                            if ui.add(egui::Button::new(i18n::t(lang, "friends.decline")).min_size(vec2(0.0, 26.0))).clicked() {
                                f.send(Command::Decline { code: code.clone() });
                            }
                        });
                    });
                }
            });
        }
        if !f.state.sent.is_empty() {
            card(ui, &pal, |ui| {
                ui.label(RichText::new(i18n::t(lang, "friends.sent")).semi());
                for code in f.state.sent.clone() {
                    divider(ui, &pal);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&code).monospace());
                        ui.label(RichText::new(i18n::t(lang, "friends.waiting")).small().color(pal.text_weak));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(egui::Button::new(i18n::t(lang, "friends.cancel")).min_size(vec2(0.0, 26.0))).clicked() {
                                f.send(Command::Decline { code: code.clone() });
                            }
                        });
                    });
                }
            });
        }

        // Friends.
        card(ui, &pal, |ui| {
            ui.label(RichText::new(i18n::t(lang, "friends.list")).semi());
            if f.state.friends.is_empty() {
                ui.label(RichText::new(i18n::t(lang, "friends.none")).color(pal.text_weak));
            }
            if f.confirm.as_ref().is_some_and(|(_, t)| t.elapsed() >= CONFIRM) {
                f.confirm = None;
            }
            for friend in f.state.friends.clone() {
                divider(ui, &pal);
                let code = friend.code;
                let nick = f.names.get(&code).cloned().filter(|n| !n.is_empty());
                let editing = f.editing.as_ref().is_some_and(|(c, _)| *c == code);
                ui.horizontal(|ui| {
                    dot(ui, if friend.online { GREEN } else { pal.text_weak.gamma_multiply(0.6) });
                    if editing {
                        let mut save = false;
                        let mut cancel = false;
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            save = ui.add(egui::Button::new(i18n::t(lang, "friends.save")).min_size(vec2(0.0, 26.0))).clicked();
                            if let Some((_, text)) = f.editing.as_mut() {
                                let w = (ui.available_width() - 8.0).max(60.0);
                                let r = ui.add_sized(
                                    vec2(w, 26.0),
                                    egui::TextEdit::singleline(text).hint_text(i18n::t(lang, "friends.nickname")).char_limit(32),
                                );
                                r.request_focus();
                                if r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                    save = true;
                                }
                                if ui.input(|i| i.key_pressed(Key::Escape)) {
                                    cancel = true;
                                }
                            }
                        });
                        if save {
                            if let Some((c, text)) = f.editing.take() {
                                f.set_name(&c, &text);
                            }
                        } else if cancel {
                            f.editing = None;
                        }
                    } else {
                        // The name has the whole line (cut with "…" if it still doesn't fit);
                        // the buttons are on the line below.
                        ui.with_layout(Layout::top_down(Align::Min), |ui| match &nick {
                            Some(n) => {
                                ui.add(egui::Label::new(RichText::new(n).semi()).truncate());
                                ui.add_space(-4.0);
                                ui.add(egui::Label::new(RichText::new(&code).small().monospace().color(pal.text_weak)).truncate());
                            }
                            None => {
                                ui.add(egui::Label::new(RichText::new(&code).monospace()).truncate());
                            }
                        });
                    }
                });
                if !editing {
                    // Things to do on their desktop (while they're online); wraps onto a
                    // second line in a narrow window ...
                    ui.horizontal_wrapped(|ui| {
                        let online = friend.online;
                        let throw = egui::Button::new(i18n::t(lang, "friends.throw")).min_size(vec2(0.0, 26.0));
                        if ui.add_enabled(online, throw).on_disabled_hover_text(i18n::t(lang, "friends.needs_online")).clicked() {
                            let x = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.subsec_millis() as f32 / 1000.0).unwrap_or(0.5);
                            f.send(Command::Send { to: code.clone(), action: Action::Snowball { x: 0.1 + 0.8 * x } });
                        }
                        let gift = egui::Button::new(i18n::t(lang, "friends.gift")).min_size(vec2(0.0, 26.0));
                        if ui.add_enabled(online, gift).on_disabled_hover_text(i18n::t(lang, "friends.needs_online")).clicked() {
                            f.send(Command::Send { to: code.clone(), action: Action::Gift });
                        }
                        // One shared desktop at a time.
                        let free = online && f.shared.status == ShareStatus::None;
                        let share = egui::Button::new(i18n::t(lang, "friends.share")).min_size(vec2(0.0, 26.0));
                        if ui.add_enabled(free, share).on_hover_text(i18n::t(lang, "friends.share_hint")).clicked() {
                            f.send(Command::ShareInvite { code: code.clone() });
                            f.shared = SharedState { with: Some(code.clone()), status: ShareStatus::Invited };
                        }
                        if ui.add(egui::Button::new(i18n::t(lang, "friends.rename")).min_size(vec2(0.0, 26.0))).clicked() {
                            f.editing = Some((code.clone(), nick.clone().unwrap_or_default()));
                            f.confirm = None;
                        }
                        let armed = f.confirm.as_ref().is_some_and(|(c, _)| *c == code);
                        let b = if armed {
                            egui::Button::new(RichText::new(i18n::t(lang, "friends.sure")).semi().color(pal.on_accent)).fill(pal.danger)
                        } else {
                            egui::Button::new(i18n::t(lang, "friends.remove"))
                        };
                        if ui.add(b.min_size(vec2(0.0, 26.0))).clicked() {
                            if armed {
                                f.send(Command::Remove { code: code.clone() });
                                f.confirm = None;
                                if f.names.contains_key(&code) {
                                    f.set_name(&code, "");
                                }
                            } else {
                                f.confirm = Some((code.clone(), Instant::now()));
                            }
                        }
                    });
                    // ... and what they may do on this one.
                    ui.horizontal_wrapped(|ui| {
                        let mut p = f.perms.get(&code).copied().unwrap_or_default();
                        let before = p;
                        ui.checkbox(&mut p.snowballs, RichText::new(i18n::t(lang, "friends.allow_snowballs")).small());
                        ui.checkbox(&mut p.gifts, RichText::new(i18n::t(lang, "friends.allow_gifts")).small());
                        if p != before {
                            f.set_perms(&code, p);
                        }
                    });
                }
            }
        });
        dirty
    }
}
