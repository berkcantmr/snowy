//! Settings window. Runs in its own process (`snowy.exe --settings`), edits
//! %APPDATA%\Snowy\settings.json; the main process picks up changes.

mod app;
mod editor_app;
mod editor_logic;
mod fonts;
mod state;
mod ui;
mod update_ui;
mod whats_new;
mod whats_new_data;
mod win;

use snowy_core::MonitorInfo;

/// Open the settings window and block until it is closed.
/// `monitors` is passed in so this crate needs no Win32 code.
pub fn run(monitors: Vec<MonitorInfo>) -> anyhow::Result<()> {
    run_with(monitors, None)
}

/// Like `run`, opening on the given page (`about`, `system`, ...; unknown = first page).
pub fn run_page(monitors: Vec<MonitorInfo>, page: Option<&str>) -> anyhow::Result<()> {
    run_inner(monitors, None, None, page.and_then(state::Page::from_id).unwrap_or_default())
}

/// Like `run`, optionally closing automatically after `auto_close` (used by the demo example).
#[doc(hidden)]
pub fn run_with(monitors: Vec<MonitorInfo>, auto_close: Option<std::time::Duration>) -> anyhow::Result<()> {
    run_demo(monitors, auto_close, None)
}

/// Demo helper: like `run_with`, additionally switching pages automatically every `page_cycle`.
#[doc(hidden)]
pub fn run_demo(
    monitors: Vec<MonitorInfo>,
    auto_close: Option<std::time::Duration>,
    page_cycle: Option<std::time::Duration>,
) -> anyhow::Result<()> {
    run_inner(monitors, auto_close, page_cycle, state::Page::default())
}

fn run_inner(
    monitors: Vec<MonitorInfo>,
    auto_close: Option<std::time::Duration>,
    page_cycle: Option<std::time::Duration>,
    page: state::Page,
) -> anyhow::Result<()> {
    let Some(_guard) = win::acquire_single_instance() else {
        return Ok(());
    };
    let settings = snowy_core::config::Settings::load();
    let lang = state::effective_lang(&settings.language, win::windows_locale().as_deref());
    let title = snowy_core::i18n::t(lang, "settings.title");

    let place = win::fit_window([560.0, 640.0], [480.0, 520.0], win::primary_work_area_logical());
    win::start_window_guard(place.min);
    let mut viewport = eframe::egui::ViewportBuilder::default();
    if let Some(p) = place.pos {
        viewport = viewport.with_position(p);
    }
    let options = eframe::NativeOptions {
        viewport: viewport
            .with_inner_size(place.size)
            .with_min_inner_size(place.min)
            .with_maximize_button(false)
            .with_title(title),
        centered: place.pos.is_none(),
        ..Default::default()
    };
    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            fonts::install(&cc.egui_ctx, lang);
            Ok(Box::new(app::SettingsApp::new(&cc.egui_ctx, settings, monitors, auto_close, page_cycle, page)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("settings window failed: {e}"))
}

/// Look editor for one desktop prop (`snowy.exe --edit <monitor_id> <prop_id>`).
/// Edits %APPDATA%\Snowy\props-<monitor>.json in place; the main process reloads it.
pub fn run_editor(monitor_id: String, prop_id: u64) -> anyhow::Result<()> {
    run_editor_demo(monitor_id, prop_id, None)
}

/// Like `run_editor`, optionally closing automatically after `auto_close` (used by the demo example).
#[doc(hidden)]
pub fn run_editor_demo(monitor_id: String, prop_id: u64, auto_close: Option<std::time::Duration>) -> anyhow::Result<()> {
    use snowy_core::props::PropsFile;
    let settings = snowy_core::config::Settings::load();
    let lang = state::effective_lang(&settings.language, win::windows_locale().as_deref());
    let prop = editor_logic::find_prop(&PropsFile::load(&monitor_id), prop_id);

    let title_for = |lang: &str, key: &str| format!("{} \u{2014} Snowy", snowy_core::i18n::t(lang, key));
    let title = match &prop {
        Some(p) => title_for(
            lang,
            if editor_logic::is_tree(&p.kind) {
                "editor.title_tree"
            } else if editor_logic::is_lights(&p.kind) {
                "editor.title_lights"
            } else {
                "editor.title_snowman"
            },
        ),
        None => title_for(lang, "editor.not_found"),
    };
    let mut all_titles = Vec::new();
    for (l, _) in snowy_core::i18n::LANGS {
        for k in ["editor.title_snowman", "editor.title_tree", "editor.title_lights"] {
            all_titles.push(title_for(l, k));
        }
    }
    let Some(_guard) = win::acquire_editor_instance(prop_id, &all_titles) else {
        return Ok(());
    };

    let found = prop.is_some();
    let size = if found { [520.0, 720.0] } else { [380.0, 170.0] };
    let place = win::fit_window(size, if found { [480.0, 560.0] } else { size }, win::primary_work_area_logical());
    win::start_window_guard(place.min);
    win::bring_to_front_once(&title);
    let mut viewport = eframe::egui::ViewportBuilder::default();
    if let Some(p) = place.pos {
        viewport = viewport.with_position(p);
    }
    let options = eframe::NativeOptions {
        viewport: viewport
            .with_inner_size(place.size)
            .with_min_inner_size(place.min)
            .with_maximize_button(false)
            .with_title(title.clone())
            // Opened by clicking a decoration on the desktop: stay in front until closed.
            .with_always_on_top()
            .with_active(true),
        centered: place.pos.is_none(),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            fonts::install(&cc.egui_ctx, lang);
            Ok(Box::new(editor_app::EditorApp::new(&cc.egui_ctx, monitor_id, prop, lang, auto_close)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("editor window failed: {e}"))
}

/// The "What's new" window (`snowy.exe --whats-new --since <version>`), shown once after an
/// update with the highlights of every release newer than `since` (the version used before;
/// empty = only the newest release). Blocks until it is closed.
pub fn run_whats_new(since: &str) -> anyhow::Result<()> {
    let settings = snowy_core::config::Settings::load();
    let lang = state::effective_lang(&settings.language, win::windows_locale().as_deref());
    run_whats_new_inner(lang, since.to_string(), None, true)
}

/// Like `run_whats_new`, optionally closing automatically after `auto_close` (used by the demo
/// example). `SNOWY_LANG` overrides the language and "Open settings" only closes the window.
#[doc(hidden)]
pub fn run_whats_new_demo(auto_close: Option<std::time::Duration>) -> anyhow::Result<()> {
    let settings = snowy_core::config::Settings::load();
    let lang = match std::env::var("SNOWY_LANG") {
        Ok(l) if !l.is_empty() => snowy_core::i18n::resolve_lang(&l),
        _ => state::effective_lang(&settings.language, win::windows_locale().as_deref()),
    };
    // DEMO_SINCE: the version to pretend the user came from (default: show everything).
    let since = std::env::var("DEMO_SINCE").unwrap_or_else(|_| "0.0.0".to_string());
    run_whats_new_inner(lang, since, auto_close, false)
}

fn run_whats_new_inner(lang: &'static str, since: String, auto_close: Option<std::time::Duration>, spawn_settings: bool) -> anyhow::Result<()> {
    let title = snowy_core::i18n::t(lang, "whats_new.title");
    let place = win::fit_window([560.0, 640.0], [480.0, 520.0], win::primary_work_area_logical());
    win::start_window_guard(place.min);
    let mut viewport = eframe::egui::ViewportBuilder::default();
    if let Some(p) = place.pos {
        viewport = viewport.with_position(p);
    }
    let options = eframe::NativeOptions {
        viewport: viewport
            .with_inner_size(place.size)
            .with_min_inner_size(place.min)
            .with_maximize_button(false)
            .with_title(title),
        centered: place.pos.is_none(),
        ..Default::default()
    };
    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            fonts::install(&cc.egui_ctx, lang);
            Ok(Box::new(whats_new::WhatsNewApp::new(&cc.egui_ctx, lang, &since, auto_close, spawn_settings)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("what's new window failed: {e}"))
}
