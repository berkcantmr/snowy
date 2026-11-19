#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Snowy: desktop weather effects. Main process = tray + overlays + render loop.
//! `snowy.exe --settings [--page about]` = settings window process.

mod shared;

use snowy_core::config::Settings;
use snowy_core::{i18n, Effect, EffectChoice, EffectKind, Env, MonitorInfo, Pointer, Scene, Surface, WorldEvent};
use snowy_props::{Action, Layer};
use snowy_gfx::OverlayWindow;
use snowy_sys::{Tray, TrayCommand, TrayState};
use std::time::{Duration, Instant, SystemTime};

struct Screen {
    monitor: MonitorInfo,
    overlay: OverlayWindow,
    effect: Option<Box<dyn Effect>>,
    surfaces: Vec<Surface>,
    /// Snowmen/trees/plough: a second overlay that always sits behind windows.
    props_overlay: Option<OverlayWindow>,
    props: Layer,
    /// Failed frames so far; only some are logged so a broken driver can't flood the log.
    render_errors: u32,
}

impl Screen {
    fn note_render_error(&mut self, what: &str, e: &anyhow::Error) {
        self.render_errors += 1;
        let n = self.render_errors;
        if n <= 3 || n.is_multiple_of(1000) {
            log::warn!("{what} failed on {} (x{n}): {e:#}", self.monitor.id);
        }
    }
}

struct App {
    settings: Settings,
    /// Friends server connection (only while friends are turned on).
    friends: Option<snowy_net::Hub>,
    /// Revision of the friends state last written to friends.json.
    friends_rev: u64,
    /// Desktop shared with a friend.
    shared: shared::SharedDesktop,
    /// When each friend last did each kind of action here (receiver-side rate limit).
    friend_actions: std::collections::HashMap<(String, snowy_net::state::ActionKind), Instant>,
    /// The snowball notice on screen: from whom, how many so far, when the last one came.
    /// Snowballs from the same friend in quick succession update one notice ("threw 5").
    snowball_notice: Option<(String, u32, Instant)>,
    /// A usage ping is being sent right now.
    usage_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    settings_mtime: Option<SystemTime>,
    screens: Vec<Screen>,
    active: Option<EffectKind>,
    user_paused: bool,
    auto_paused: bool,
    /// Why `auto_paused` is set (i18n key), for the tray tooltip and the log.
    pause_reason: Option<&'static str>,
    /// No overlay could be created on any enabled monitor.
    no_renderer: bool,
    random_since: Instant,
    wind: Wind,
    tray: Tray,
    pointer: PointerTracker,
    /// Swallows clicks on snow so playing doesn't click the desktop. None if the hook failed.
    grab: Option<snowy_sys::MouseGrab>,
    zones_local: Vec<[f32; 4]>,
    zones_screen: Vec<snowy_sys::ScreenRect>,
    /// Started on demand when the effect choice is Weather.
    weather: Option<snowy_weather::WeatherWatch>,
    /// Running while `check_updates` is on.
    update: Option<snowy_update::UpdateWatch>,
    /// Version currently offered in the tray menu.
    update_shown: Option<String>,
    wallpaper: WallpaperState,
    /// Edit mode: props next to desktop icons can be moved (toggled by shortcut or tray).
    edit_mode: bool,
    /// Last activity in edit mode (it turns itself off after `EDIT_IDLE`).
    edit_active: Instant,
    hotkey: Option<snowy_sys::EditHotkey>,
    desktop_icons: snowy_sys::DesktopIcons,
    toast: Option<snowy_sys::Toast>,
    /// Last "locked" hint, so holding the mouse doesn't re-show it every press.
    last_hint: Option<Instant>,
}

/// Edit mode turns itself off after this long without mouse presses.
const EDIT_IDLE: Duration = Duration::from_secs(5 * 60);

type WpKey = (snowy_wallpaper::Theme, snowy_wallpaper::TimeOfDay, u32, u32);

enum WpDone {
    Applied(WpKey),
    Failed(WpKey),
    Restored,
}

/// Painted-wallpaper bookkeeping: what is applied, and the background job (if any).
#[derive(Default)]
struct WallpaperState {
    applied: Option<WpKey>,
    failed: Option<WpKey>,
    job: Option<std::sync::mpsc::Receiver<WpDone>>,
}

fn main() -> anyhow::Result<()> {
    snowy_sys::init_dpi_awareness();

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--settings") {
        let page = args.iter().position(|a| a == "--page").and_then(|i| args.get(i + 1)).map(String::as_str);
        return snowy_settings::run_page(snowy_sys::monitors(), page);
    }
    if args.iter().any(|a| a == "--whats-new") {
        let since = args.iter().position(|a| a == "--since").and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();
        return snowy_settings::run_whats_new(&since);
    }
    if let Some(i) = args.iter().position(|a| a == "--edit") {
        let monitor = args.get(i + 1).cloned().unwrap_or_default();
        let id = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        return snowy_settings::run_editor(monitor, id);
    }

    let Some(_guard) = snowy_sys::single_instance() else {
        // Already running: just open settings for the user.
        open_settings(None);
        return Ok(());
    };

    init_logger();
    let settings = Settings::load();
    sync_autostart(settings.start_with_windows);
    let settings = maybe_show_whats_new(settings);
    let lang = lang_of(&settings);
    log_startup(&settings);
    let tray = Tray::new(&TrayState { effect: settings.effect, paused: false, lang, update: None, notice: None, edit_mode: false })?;

    let mut app = App {
        settings_mtime: mtime(),
        settings: settings.clone(),
        friends: None,
        friends_rev: 0,
        friend_actions: std::collections::HashMap::new(),
        shared: shared::SharedDesktop::load(&Settings::dir()),
        snowball_notice: None,
        usage_busy: Default::default(),
        screens: Vec::new(),
        active: None,
        user_paused: false,
        auto_paused: false,
        pause_reason: None,
        no_renderer: false,
        random_since: Instant::now(),
        wind: Wind::default(),
        tray,
        pointer: PointerTracker::default(),
        grab: snowy_sys::MouseGrab::start().map_err(|e| log::warn!("mouse grab: {e:#}")).ok(),
        zones_local: Vec::new(),
        zones_screen: Vec::new(),
        weather: None,
        update: settings.check_updates.then(snowy_update::UpdateWatch::start),
        update_shown: None,
        wallpaper: WallpaperState::default(),
        edit_mode: false,
        edit_active: Instant::now(),
        hotkey: snowy_sys::EditHotkey::register(),
        desktop_icons: snowy_sys::DesktopIcons::start(),
        toast: snowy_sys::Toast::new().map_err(|e| log::warn!("toast: {e:#}")).ok(),
        last_hint: None,
    };
    match &app.hotkey {
        Some(h) => log::info!("edit mode shortcut: {:?}", h.combo),
        None => log::warn!("no edit mode shortcut could be registered"),
    }
    app.rebuild_screens();
    app.resolve_effect(true);

    let start = Instant::now();
    let mut last = Instant::now();
    let mut last_slow_check = Instant::now() - Duration::from_secs(10);
    let mut last_friends_check = Instant::now() - Duration::from_secs(10);
    let mut last_scan = Instant::now() - Duration::from_secs(10);
    let mut last_monitors = snowy_sys::monitors();
    let mut prof = Profiler::new();
    let mut pacer = Pacer::new();

    loop {
        if !snowy_sys::pump_messages() {
            break;
        }
        if snowy_core::shutdown::requested() {
            // Windows or an installer (Restart Manager) asked Snowy to close.
            log::info!("close requested by the system, quitting");
            close_children();
            return Ok(());
        }
        while let Some(cmd) = app.tray.poll() {
            if !app.handle(cmd) {
                return Ok(());
            }
        }
        if app.hotkey.as_ref().is_some_and(|h| h.take_presses() % 2 == 1) {
            app.set_edit_mode(!app.edit_mode);
        }
        if let Some(t) = &mut app.toast {
            t.tick();
        }

        // Once per second: settings file, monitors, pause conditions, random rotation.
        if last_slow_check.elapsed() >= Duration::from_secs(1) {
            last_slow_check = Instant::now();
            let m = mtime();
            if m != app.settings_mtime {
                app.settings_mtime = m;
                app.apply_settings(Settings::load());
            }
            if app.available_update() != app.update_shown {
                app.refresh_tray();
            }
            for s in &mut app.screens {
                s.props.poll_reload();
            }
            let mons = snowy_sys::monitors();
            if mons != last_monitors {
                last_monitors = mons;
                app.rebuild_screens();
                app.resolve_effect(true);
            }
            let reason = if app.settings.pause_on_battery && snowy_sys::on_battery() {
                Some("tray.paused_battery")
            } else if app.settings.pause_on_fullscreen && snowy_sys::foreground_is_fullscreen() {
                Some("tray.paused_fullscreen")
            } else {
                None
            };
            if reason != app.pause_reason {
                log::info!("automatic pause: {}", reason.unwrap_or("off"));
                app.pause_reason = reason;
                app.auto_paused = reason.is_some();
                app.refresh_tray();
            }
            app.resolve_effect(false);
            app.tick_wallpaper();
            app.sync_desktop_icons();
            app.tick_usage_ping();
            if app.edit_mode && app.edit_active.elapsed() >= EDIT_IDLE {
                app.set_edit_mode(false);
            }
        }

        // Friends: commands from the settings window and state for it, 4 times a second.
        if last_friends_check.elapsed() >= Duration::from_millis(250) {
            last_friends_check = Instant::now();
            app.tick_friends();
        }

        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;
        let time = (now - start).as_secs_f32();

        let running = app.is_running();
        // Props are decorations: they stay while the weather is user-paused or off, but
        // stop with the automatic pauses (battery, fullscreen).
        let props_running = !app.auto_paused;
        // ~10 Hz: where are the window ledges?
        if running && last_scan.elapsed() >= Duration::from_millis(100) {
            last_scan = Instant::now();
            app.scan_surfaces();
            // Shell menus (taskbar right-click etc.) stay above the snow.
            let popup = snowy_sys::open_shell_popup();
            for s in &mut app.screens {
                s.overlay.stay_under(popup);
            }
        }
        // Props stay interactive while the weather is paused or off.
        let mut raw = if running || props_running { snowy_sys::pointer_sample() } else { None };
        if raw.is_some_and(|r| r.down) {
            app.edit_active = Instant::now();
        }
        // A swallowed press never reaches the system's key state, so ask the hook too.
        if let (Some(r), Some(g)) = (raw.as_mut(), app.grab.as_ref()) {
            r.down |= g.captured();
        }
        let was_down = app.pointer.raw.is_some_and(|r| r.down);
        app.pointer.update(raw, dt);
        app.decide_pointer_owner(was_down);
        let wind = app.wind.update(dt, app.settings.changing_wind);
        let env = Env { wind, intensity: app.settings.intensity, speed: app.settings.speed, time };
        let piling = !app.settings.keep_windows_clear;
        let snowballs = app.settings.snowballs;
        for s in &mut app.screens {
            let res = match (&mut s.effect, running) {
                (Some(e), true) => {
                    let scene = Scene {
                        surfaces: &s.surfaces,
                        ground: piling.then_some(ground_y(&s.monitor)),
                        pointer: app.pointer.local_for(&s.monitor, Owner::Weather),
                        snowballs,
                    };
                    let t0 = Instant::now();
                    e.set_pile_depth(snowy_effects::DEFAULT_PILE_DEPTH * app.settings.snow_depth);
                    e.update(dt, &env, &scene);
                    let t1 = Instant::now();
                    let r = s.overlay.render(e.as_ref(), time);
                    prof.update += t1 - t0;
                    prof.render += t1.elapsed();
                    prof.instances += e.instances().len();
                    r
                }
                _ => s.overlay.clear(),
            };
            if let Err(e) = res {
                s.note_render_error("render", &e);
            }
        }
        app.update_props(dt, &env, props_running);
        app.publish_grab_zones(running, props_running);
        let work = now.elapsed();

        // Frame pacing. Running: present on vblanks (DwmFlush), aiming at an accumulating
        // deadline so the *average* rate equals the FPS cap even when it isn't a divisor of
        // the refresh rate (60 fps on 144 Hz alternates 2 and 3 vblanks).
        // Props alone (weather paused or off) animate too, so they get the same pacing.
        // Idle: poll slowly to stay near 0% CPU.
        let props_animating = props_running && app.screens.iter().any(|s| !s.props.is_empty());
        if running || props_animating {
            pacer.wait(Duration::from_secs_f32(1.0 / app.settings.max_fps as f32));
        } else {
            std::thread::sleep(Duration::from_millis(250));
            pacer.reset();
        }
        prof.frame(work, now.elapsed());
    }
    Ok(())
}

impl App {
    fn is_running(&self) -> bool {
        !self.user_paused && !self.auto_paused && self.active.is_some()
    }

    fn handle(&mut self, cmd: TrayCommand) -> bool {
        match cmd {
            TrayCommand::SetEffect(choice) => {
                let mut s = self.settings.clone();
                s.effect = choice;
                self.user_paused = false;
                let _ = s.save();
                self.settings_mtime = mtime();
                self.apply_settings(s);
            }
            TrayCommand::TogglePause => {
                self.user_paused = !self.user_paused;
                self.refresh_tray();
            }
            TrayCommand::ClearSnow => {
                for s in &mut self.screens {
                    if let Some(e) = &mut s.effect {
                        e.clear_accumulation();
                    }
                }
            }
            TrayCommand::BuildSnowman => {
                if let Some(s) = self.primary_screen() {
                    s.props.add_snowman();
                }
            }
            TrayCommand::AddTree => {
                if let Some(s) = self.primary_screen() {
                    s.props.add_tree();
                }
            }
            TrayCommand::AddLights => {
                if let Some(s) = self.primary_screen() {
                    s.props.add_lights();
                }
            }
            TrayCommand::SendPlough => {
                for s in &mut self.screens {
                    s.props.send_plough();
                }
            }
            TrayCommand::CallYeti => {
                for s in &mut self.screens {
                    s.props.call_yeti();
                }
            }
            TrayCommand::ToggleEditMode => self.set_edit_mode(!self.edit_mode),
            TrayCommand::OpenSettings => open_settings(None),
            TrayCommand::UpdateNow => open_settings(Some("about")),
            TrayCommand::SkipUpdate => {
                if let Some(v) = self.available_update() {
                    snowy_update::skip(&v);
                }
                self.refresh_tray();
            }
            TrayCommand::Quit => {
                close_children();
                return false;
            }
        }
        true
    }

    /// Once per second: keep the painted wallpaper in sync with the settings. Rendering,
    /// caching and applying happen on a background thread; the same image is never re-applied.
    fn tick_wallpaper(&mut self) {
        if let Some(rx) = &self.wallpaper.job {
            match rx.try_recv() {
                Ok(WpDone::Applied(k)) => {
                    self.wallpaper.applied = Some(k);
                    self.wallpaper.job = None;
                }
                Ok(WpDone::Failed(k)) => {
                    self.wallpaper.failed = Some(k);
                    self.wallpaper.job = None;
                }
                Ok(WpDone::Restored) => {
                    self.wallpaper.applied = None;
                    self.wallpaper.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.wallpaper.job = None,
            }
        }
        let Some(id) = self.settings.wallpaper.clone() else {
            // Switched back to "keep my own": restore the saved original once.
            if let Some(orig) = self.settings.original_wallpaper.clone() {
                let mut fresh = Settings::load();
                fresh.original_wallpaper = None;
                let _ = fresh.save();
                self.settings.original_wallpaper = None;
                self.settings_mtime = mtime();
                let (tx, rx) = std::sync::mpsc::channel();
                self.wallpaper.job = Some(rx);
                std::thread::spawn(move || {
                    let path = std::path::PathBuf::from(orig);
                    if path.is_file() {
                        if let Err(e) = snowy_wallpaper::apply(&path) {
                            log::warn!("restore wallpaper: {e}");
                        }
                    }
                    let _ = tx.send(WpDone::Restored);
                });
            } else {
                self.wallpaper.applied = None;
                self.wallpaper.failed = None;
            }
            return;
        };
        let Some(theme) = snowy_wallpaper::Theme::from_id(&id) else { return };
        let tod = snowy_wallpaper::target_tod(
            self.settings.wallpaper_day_night,
            &self.settings.wallpaper_time,
            snowy_wallpaper::local_hour(),
        );
        let Some(m) = snowy_sys::monitors().into_iter().max_by_key(|m| m.rect.2 as i64 * m.rect.3 as i64) else { return };
        let key: WpKey = (theme, tod, m.rect.2.max(64) as u32, m.rect.3.max(64) as u32);
        if self.wallpaper.applied == Some(key) || self.wallpaper.failed == Some(key) {
            return;
        }
        // Remember the user's own wallpaper the first time we are about to change it.
        if self.settings.original_wallpaper.is_none() {
            if let Some(cur) = snowy_wallpaper::current_wallpaper() {
                if !cur.starts_with(snowy_wallpaper::cache_dir()) {
                    let mut fresh = Settings::load();
                    fresh.original_wallpaper = Some(cur.to_string_lossy().into_owned());
                    let _ = fresh.save();
                    self.settings.original_wallpaper = fresh.original_wallpaper.clone();
                    self.settings_mtime = mtime();
                }
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.wallpaper.job = Some(rx);
        std::thread::spawn(move || {
            let (theme, tod, w, h) = key;
            let result = snowy_wallpaper::ensure_cached(theme, tod, w, h).and_then(|path| {
                if snowy_wallpaper::current_wallpaper().as_deref() == Some(path.as_path()) {
                    Ok(())
                } else {
                    snowy_wallpaper::apply(&path)
                }
            });
            let _ = tx.send(match result {
                Ok(()) => WpDone::Applied(key),
                Err(e) => {
                    log::warn!("wallpaper: {e}");
                    WpDone::Failed(key)
                }
            });
        });
    }

    fn apply_settings(&mut self, new: Settings) {
        let old = std::mem::replace(&mut self.settings, new);
        if old.wallpaper != self.settings.wallpaper || old.wallpaper_time != self.settings.wallpaper_time {
            self.wallpaper.failed = None;
        }
        if self.settings.effect != EffectChoice::Weather {
            // Stop polling the network as soon as the user leaves "Match my weather".
            self.weather = None;
        }
        if old.check_updates != self.settings.check_updates {
            self.update = self.settings.check_updates.then(snowy_update::UpdateWatch::start);
        }
        if old.start_with_windows != self.settings.start_with_windows {
            sync_autostart(self.settings.start_with_windows);
        }
        if old.disabled_monitors != self.settings.disabled_monitors {
            self.rebuild_screens();
            self.resolve_effect(true);
        } else if old.behind_windows != self.settings.behind_windows {
            for s in &mut self.screens {
                let _ = s.overlay.set_behind_windows(self.settings.behind_windows);
            }
        }
        if old.behind_windows != self.settings.behind_windows
            || old.keep_windows_clear != self.settings.keep_windows_clear
        {
            self.scan_surfaces();
        }
        if old.effect != self.settings.effect {
            self.random_since = Instant::now();
            self.resolve_effect(true);
        }
        self.refresh_tray();
    }

    fn rebuild_screens(&mut self) {
        self.screens.clear();
        let mut wanted = 0;
        for m in snowy_sys::monitors() {
            if self.settings.disabled_monitors.contains(&m.id) {
                continue;
            }
            wanted += 1;
            log::info!(
                "monitor {}: {:?}, work area {:?}, scale {}{}",
                m.id,
                m.rect,
                m.work_rect,
                m.dpi_scale,
                if m.primary { ", primary" } else { "" }
            );
            match OverlayWindow::new(&m, self.settings.behind_windows) {
                Ok(overlay) => {
                    let props_overlay = OverlayWindow::new(&m, true)
                        .map_err(|e| log::warn!("props overlay for {} failed: {e:#}", m.id))
                        .ok();
                    let (w, h) = overlay.size();
                    let mut props = Layer::new(&m, w as f32, h as f32, ground_y(&m));
                    props.set_edit_mode(self.edit_mode);
                    self.screens.push(Screen {
                        monitor: m,
                        overlay,
                        effect: None,
                        surfaces: Vec::new(),
                        props_overlay,
                        props,
                        render_errors: 0,
                    });
                }
                Err(e) => log::error!("overlay for {} failed: {e:#}", m.id),
            }
        }
        self.active = None;
        let no_renderer = wanted > 0 && self.screens.is_empty();
        if no_renderer != self.no_renderer {
            self.no_renderer = no_renderer;
            self.refresh_tray();
        }
    }

    /// Decide which concrete effect should run; (re)create per-screen effects if it changed.
    fn resolve_effect(&mut self, force: bool) {
        let want = match self.settings.effect {
            EffectChoice::Off => None,
            EffectChoice::Fixed(k) => Some(k),
            EffectChoice::Season => Some(season_effect()),
            EffectChoice::Weather => {
                // Network use only while this choice is active (opt-in).
                let w = self.weather.get_or_insert_with(snowy_weather::WeatherWatch::start);
                match w.current() {
                    Some(c) => snowy_weather::effect_for(&c, month_now()),
                    // Not fetched yet / offline: fall back to the season's effect.
                    None => Some(season_effect()),
                }
            }
            EffectChoice::Random => {
                let due = self.random_since.elapsed()
                    >= Duration::from_secs(self.settings.random_minutes as u64 * 60);
                if due || self.active.is_none() || force {
                    self.random_since = Instant::now();
                    let pool: Vec<_> =
                        EffectKind::ALL.iter().copied().filter(|k| Some(*k) != self.active).collect();
                    Some(pool[fastrand::usize(..pool.len())])
                } else {
                    self.active
                }
            }
        };
        if want == self.active && !force {
            return;
        }
        if force && want == self.active && self.screens.iter().all(|s| s.effect.is_some() == want.is_some()) {
            return;
        }
        self.active = want;
        for s in &mut self.screens {
            s.effect = want.map(|k| {
                let (w, h) = s.overlay.size();
                snowy_effects::create(k, w as f32, h as f32)
            });
            s.overlay.set_visible(want.is_some());
        }
    }

    /// Refresh window ledges per screen. None in behind-windows mode (effects are under the
    /// windows, so nothing can land on them) or when the user keeps windows clear.
    fn scan_surfaces(&mut self) {
        if self.settings.behind_windows || self.settings.keep_windows_clear {
            for s in &mut self.screens {
                s.surfaces.clear();
            }
            return;
        }
        let mons: Vec<MonitorInfo> = self.screens.iter().map(|s| s.monitor.clone()).collect();
        for (s, mut surf) in self.screens.iter_mut().zip(snowy_sys::scan_surfaces(&mons)) {
            // A bottom taskbar's top edge is the ground itself: one pile, not two.
            let g = ground_y(&s.monitor);
            surf.retain(|l| (l.y - g).abs() > 3.0);
            s.surfaces = surf;
        }
    }

    fn primary_screen(&mut self) -> Option<&mut Screen> {
        let i = self.screens.iter().position(|s| s.monitor.primary).unwrap_or(0);
        self.screens.get_mut(i)
    }

    /// Simulate and draw the props layer of every screen; route events and actions between
    /// the weather effect and the props.
    /// Connects to / disconnects from the friends server as the setting says, passes the
    /// settings window's commands on and publishes the state for it.
    fn tick_friends(&mut self) {
        let dir = Settings::dir();
        if self.settings.friends && self.friends.is_none() {
            match snowy_net::identity::load_or_create(&dir.join("identity.key")) {
                Ok(key) => {
                    log::info!("friends: on");
                    self.friends = Some(snowy_net::Hub::start(key, snowy_net::hub_url(), env!("CARGO_PKG_VERSION").to_string()));
                }
                Err(e) => log::warn!("friends: no identity key: {e:#}"),
            }
        } else if !self.settings.friends && self.friends.is_some() {
            log::info!("friends: off");
            self.friends = None;
            let mut st = snowy_net::state::load_state(&dir);
            st.status = snowy_net::Status::Off;
            for f in &mut st.friends {
                f.online = false;
            }
            st.rev = st.rev.wrapping_add(1);
            let _ = snowy_net::state::save_state(&dir, &st);
            self.friends_rev = 0;
        }
        let commands = snowy_net::state::take_commands(&dir);
        if let Some(hub) = &self.friends {
            let monitor = self.screens.iter().find(|s| s.monitor.primary).map(|s| s.monitor.id.clone()).unwrap_or_default();
            let mut out = Vec::new();
            let mut notices = Vec::new();
            for c in commands {
                if c.is_share() {
                    notices.extend(self.shared.command(&c, &monitor, &mut out));
                } else {
                    hub.send(c);
                }
            }
            let incoming = hub.take_incoming();
            let scene = hub.take_scene();
            let st = hub.state();
            let changed = !scene.is_empty();
            for (from, msg) in scene {
                notices.extend(self.shared.message(&from, msg, &monitor, &mut out));
            }
            // Sharing with someone who is no longer a friend ends it.
            if st.status == snowy_net::Status::Online {
                if let Some(w) = self.shared.state().with.clone() {
                    if !st.friends.iter().any(|f| f.code == w) {
                        self.shared.command(&snowy_net::Command::ShareStop, &monitor, &mut Vec::new());
                    }
                }
            }
            self.shared.tick(&monitor, |c| st.friends.iter().any(|f| f.code == c && f.online), &mut out);
            for (to, msg) in out {
                hub.send(snowy_net::Command::Scene { to, msg });
            }
            if changed || !notices.is_empty() {
                // Show friends' changes right away instead of at the next 1 s reload.
                for s in &mut self.screens {
                    s.props.poll_reload();
                }
            }
            if !notices.is_empty() {
                self.share_notices(&dir, notices);
            }
            if !incoming.is_empty() {
                self.friend_actions_arrived(&dir, incoming);
            }
            if st.rev != self.friends_rev {
                self.friends_rev = st.rev;
                if let Err(e) = snowy_net::state::save_state(&dir, &st) {
                    log::warn!("friends: writing state: {e}");
                }
            }
        }
    }

    fn share_notices(&mut self, dir: &std::path::Path, notices: Vec<shared::Notice>) {
        let names = snowy_net::state::load_names(dir);
        let lang = lang_of(&self.settings);
        for n in notices {
            let (key, who) = match &n {
                shared::Notice::Asked(c) => ("toast.share_asked", c),
                shared::Notice::Started(c) => ("toast.share_started", c),
                shared::Notice::Stopped(c) => ("toast.share_stopped", c),
            };
            let name = names.get(who).filter(|n| !n.trim().is_empty()).cloned().unwrap_or_else(|| who.clone());
            let text = i18n::t(&lang, key).replace("{name}", &name);
            if let Some(t) = &mut self.toast {
                t.show(snowy_sys::ToastIcon::Friend, &text, &[], None);
            }
        }
    }

    /// Shows what friends did, if this person allows it from them and they aren't overdoing it.
    fn friend_actions_arrived(&mut self, dir: &std::path::Path, incoming: Vec<(String, snowy_net::state::Action)>) {
        use snowy_net::state::Action;
        let perms = snowy_net::state::load_perms(dir);
        let names = snowy_net::state::load_names(dir);
        let lang = lang_of(&self.settings);
        for (from, action) in incoming {
            let kind = action.kind();
            if !perms.get(&from).copied().unwrap_or_default().allows(kind) {
                continue;
            }
            let key = (from.clone(), kind);
            if self.friend_actions.get(&key).is_some_and(|t| t.elapsed() < kind.min_gap()) {
                log::info!("friends: {from} is sending too fast; dropped");
                continue;
            }
            self.friend_actions.insert(key, Instant::now());
            let name = names.get(&from).filter(|n| !n.trim().is_empty()).cloned().unwrap_or_else(|| from.clone());
            let text = match action {
                Action::Snowball { x } => {
                    if self.is_running() {
                        if let Some(e) = self.primary_screen().and_then(|s| s.effect.as_mut()) {
                            e.incoming_snowball(x);
                        }
                    }
                    let n = match &self.snowball_notice {
                        Some((who, n, t)) if *who == from && t.elapsed() < Duration::from_secs(8) => n + 1,
                        _ => 1,
                    };
                    self.snowball_notice = Some((from.clone(), n, Instant::now()));
                    if n == 1 {
                        i18n::t(&lang, "toast.friend_snowball").replace("{name}", &name)
                    } else {
                        i18n::t(&lang, "toast.friend_snowballs").replace("{name}", &name).replace("{n}", &n.to_string())
                    }
                }
                Action::Gift => {
                    if let Some(s) = self.primary_screen() {
                        s.props.drop_gift();
                    }
                    self.snowball_notice = None;
                    i18n::t(&lang, "toast.friend_gift").replace("{name}", &name)
                }
            };
            if let Some(t) = &mut self.toast {
                t.show(snowy_sys::ToastIcon::Friend, &text, &[], None);
            }
        }
    }

    /// Once a day (if allowed), sends the anonymous usage ping on a background thread.
    fn tick_usage_ping(&mut self) {
        use std::sync::atomic::Ordering;
        if !self.settings.usage_stats || self.usage_busy.load(Ordering::Relaxed) {
            return;
        }
        let dir = Settings::dir();
        if !snowy_net::usage::due(&dir) {
            return;
        }
        self.usage_busy.store(true, Ordering::Relaxed);
        let busy = self.usage_busy.clone();
        std::thread::spawn(move || {
            match snowy_net::usage::send(&dir, snowy_net::PING_URL, env!("CARGO_PKG_VERSION")) {
                Ok(()) => log::info!("usage ping sent"),
                Err(e) => log::info!("usage ping failed: {e}"),
            }
            // A failed ping is retried after an hour, not every second.
            if snowy_net::usage::due(&dir) {
                std::thread::sleep(Duration::from_secs(3600));
            }
            busy.store(false, Ordering::Relaxed);
        });
    }

    /// On a new press: a snowball under the cursor wins, then a decoration, then the weather
    /// (starting a snowball on the snow). Released: nobody.
    fn decide_pointer_owner(&mut self, was_down: bool) {
        let down = self.pointer.raw.is_some_and(|r| r.down);
        if !down {
            self.pointer.owner = Owner::Nobody;
            return;
        }
        if was_down {
            return;
        }
        let mut owner = Owner::Weather;
        for s in &self.screens {
            let Some(p) = self.pointer.local(&s.monitor).filter(|p| p.over_desktop) else { continue };
            owner = if s.effect.as_ref().is_some_and(|e| e.grabs_at(p.pos)) {
                Owner::Weather
            } else if s.props.hits(p.pos) {
                Owner::Props
            } else {
                Owner::Weather
            };
            break;
        }
        self.pointer.owner = owner;
    }

    fn update_props(&mut self, dt: f32, env: &Env, running: bool) {
        let mut events: Vec<WorldEvent> = Vec::new();
        let mut actions: Vec<Action> = Vec::new();
        let mut hints: Vec<(i32, i32)> = Vec::new();
        for s in &mut self.screens {
            events.clear();
            if let Some(e) = &mut s.effect {
                e.take_events(&mut events);
            }
            for ev in events.drain(..) {
                s.props.handle(ev);
            }
            let Some(ov) = &mut s.props_overlay else { continue };
            let res = if running && !s.props.is_empty() {
                let scene = Scene {
                    surfaces: &[],
                    ground: Some(ground_y(&s.monitor)),
                    pointer: self.pointer.local_for(&s.monitor, Owner::Props),
                    snowballs: false,
                };
                s.props.update(dt, env, &scene);
                ov.render(&s.props, env.time)
            } else {
                ov.clear()
            };
            if let Err(e) = res {
                s.note_render_error("props render", &e);
            }
            actions.clear();
            s.props.take_actions(&mut actions);
            for a in actions.drain(..) {
                match a {
                    Action::LockedHint { pos } => {
                        let (mx, my, _, _) = s.monitor.rect;
                        hints.push((mx + pos[0] as i32, my + pos[1] as i32));
                    }
                    Action::OpenEditor { prop_id } => open_editor(&s.monitor.id, prop_id),
                    Action::ClearGround { x0, x1 } => {
                        if let Some(e) = &mut s.effect {
                            e.clear_region(x0, x1);
                        }
                    }
                }
            }
        }
        if let Some(&pt) = hints.last() {
            self.show_locked_hint(pt);
        }
    }

    /// Toggle edit mode on every screen, with a short notice showing the shortcut.
    fn set_edit_mode(&mut self, on: bool) {
        if self.edit_mode == on {
            return;
        }
        log::info!("edit mode {}", if on { "on" } else { "off" });
        self.edit_mode = on;
        self.edit_active = Instant::now();
        for s in &mut self.screens {
            s.props.set_edit_mode(on);
        }
        let lang = lang_of(&self.settings);
        let keys = self.shortcut_keys();
        if let Some(t) = &mut self.toast {
            if on {
                t.show(snowy_sys::ToastIcon::EditOn, i18n::t(&lang, "toast.edit_on"), &keys, None);
            } else {
                t.show(snowy_sys::ToastIcon::EditOff, i18n::t(&lang, "toast.edit_off"), &[], None);
            }
        }
        self.refresh_tray();
    }

    /// The press at `pt` (screen px) hit a prop locked next to desktop icons.
    fn show_locked_hint(&mut self, pt: (i32, i32)) {
        if self.last_hint.is_some_and(|t| t.elapsed() < Duration::from_millis(1500))
            && self.toast.as_ref().is_some_and(|t| t.is_visible())
        {
            return;
        }
        self.last_hint = Some(Instant::now());
        let lang = lang_of(&self.settings);
        let keys = self.shortcut_keys();
        if let Some(t) = &mut self.toast {
            t.show(snowy_sys::ToastIcon::Lock, i18n::t(&lang, "toast.locked"), &keys, Some(pt));
        }
    }

    /// The registered shortcut as keycaps (empty if none could be registered; the tray
    /// menu item still works).
    fn shortcut_keys(&self) -> Vec<snowy_sys::Key> {
        use snowy_sys::Key;
        let Some(h) = &self.hotkey else { return Vec::new() };
        let c = h.combo;
        let mut keys = Vec::new();
        for (on, k) in [(c.win, Key::Win), (c.ctrl, Key::Ctrl), (c.alt, Key::Alt), (c.shift, Key::Shift)] {
            if on {
                keys.push(k);
            }
        }
        keys.push(Key::Char(c.key));
        keys
    }

    /// Hand the desktop icon boxes to each screen's props (in that overlay's coordinates).
    fn sync_desktop_icons(&mut self) {
        let rects = self.desktop_icons.rects();
        let mut local = Vec::new();
        for s in &mut self.screens {
            let (mx, my, w, h) = s.monitor.rect;
            local.clear();
            local.extend(
                rects
                    .iter()
                    .filter(|r| r.2 > mx && r.0 < mx + w && r.3 > my && r.1 < my + h)
                    .map(|r| [(r.0 - mx) as f32, (r.1 - my) as f32, (r.2 - mx) as f32, (r.3 - my) as f32]),
            );
            s.props.set_desktop_icons(&local);
        }
    }

    /// Collect every effect's grab zones, convert to screen coordinates, hand to the hook.
    fn publish_grab_zones(&mut self, running: bool, props_running: bool) {
        let Some(grab) = &self.grab else { return };
        self.zones_screen.clear();
        for s in &self.screens {
            let (mx, my, _, _) = s.monitor.rect;
            self.zones_local.clear();
            if running {
                if let Some(e) = &s.effect {
                    e.grab_zones(&mut self.zones_local);
                }
            }
            if props_running {
                s.props.grab_zones(&mut self.zones_local);
            }
            self.zones_screen.extend(self.zones_local.iter().map(|z| {
                (
                    mx + z[0].floor() as i32,
                    my + z[1].floor() as i32,
                    mx + z[2].ceil() as i32,
                    my + z[3].ceil() as i32,
                )
            }));
        }
        grab.set_zones(&self.zones_screen);
    }

    /// Version of a newer, non-skipped release, if the update watch found one.
    fn available_update(&self) -> Option<String> {
        self.update.as_ref()?.current().map(|r| r.version)
    }

    fn refresh_tray(&mut self) {
        self.update_shown = self.available_update();
        let state = TrayState {
            effect: self.settings.effect,
            paused: self.user_paused,
            lang: lang_of(&self.settings),
            update: self.update_shown.clone(),
            notice: if self.no_renderer { Some("tray.no_renderer") } else { self.pause_reason },
            edit_mode: self.edit_mode,
        };
        self.tray.update(&state);
    }
}

/// Vblank-aligned frame pacing towards an accumulating deadline.
struct Pacer {
    deadline: Instant,
    /// Estimated vblank period (EMA of DwmFlush intervals).
    vblank: Duration,
}

impl Pacer {
    fn new() -> Self {
        Self { deadline: Instant::now(), vblank: Duration::from_micros(16_667) }
    }

    fn reset(&mut self) {
        self.deadline = Instant::now();
    }

    fn wait(&mut self, target: Duration) {
        self.deadline += target;
        let now = Instant::now();
        // Fell far behind (stall, sleep, debugger): resync instead of racing to catch up.
        if self.deadline + target * 2 < now {
            self.deadline = now + target;
        }
        loop {
            let left = self.deadline.saturating_duration_since(Instant::now());
            // Stop when the next vblank would land further past the deadline than it is now
            // before it, i.e. pick the vblank closest to the deadline.
            if left <= self.vblank / 2 {
                break;
            }
            let t = Instant::now();
            if !snowy_sys::wait_vblank() {
                std::thread::sleep(left);
                break;
            }
            let dt = t.elapsed();
            // Ignore outliers (e.g. a flush that returned immediately or a long stall).
            if dt > Duration::from_millis(2) && dt < Duration::from_millis(50) {
                self.vblank = (self.vblank * 7 + dt) / 8;
            }
        }
    }
}

/// Frame-time breakdown printed every 5 s when SNOWY_PROFILE is set (stderr).
struct Profiler {
    on: bool,
    since: Instant,
    frames: u32,
    update: Duration,
    render: Duration,
    work: Duration,
    total: Duration,
    instances: usize,
}

impl Profiler {
    fn new() -> Self {
        Self {
            on: std::env::var_os("SNOWY_PROFILE").is_some(),
            since: Instant::now(),
            frames: 0,
            update: Duration::ZERO,
            render: Duration::ZERO,
            work: Duration::ZERO,
            total: Duration::ZERO,
            instances: 0,
        }
    }

    fn frame(&mut self, work: Duration, total: Duration) {
        if !self.on {
            return;
        }
        self.frames += 1;
        self.work += work;
        self.total += total;
        if self.since.elapsed() >= Duration::from_secs(5) {
            let n = self.frames.max(1);
            eprintln!(
                "fps {:.1} | update {:?} | render {:?} | all work {:?} | frame {:?} | instances {}",
                n as f32 / self.since.elapsed().as_secs_f32(),
                self.update / n,
                self.render / n,
                self.work / n,
                self.total / n,
                self.instances / n as usize,
            );
            *self = Self { on: true, ..Self::new() };
        }
    }
}

/// Who a press belongs to, decided when the button goes down and kept until it is released,
/// so dragging a snowball in front of a tree doesn't drag the tree too (and the other way round).
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
enum Owner {
    #[default]
    Nobody,
    Weather,
    Props,
}

/// Cursor position (virtual screen) with smoothed velocity.
#[derive(Default)]
struct PointerTracker {
    raw: Option<snowy_sys::RawPointer>,
    vel: [f32; 2],
    owner: Owner,
}

impl PointerTracker {
    fn update(&mut self, raw: Option<snowy_sys::RawPointer>, dt: f32) {
        match (self.raw, raw) {
            (Some(a), Some(b)) if dt > 0.0 => {
                let v = [(b.x - a.x) as f32 / dt, (b.y - a.y) as f32 / dt];
                // Exponential smoothing, ~50 ms time constant.
                let k = (dt / 0.05).min(1.0);
                self.vel = [self.vel[0] + (v[0] - self.vel[0]) * k, self.vel[1] + (v[1] - self.vel[1]) * k];
            }
            _ => self.vel = [0.0; 2],
        }
        self.raw = raw;
    }

    /// Pointer in `m`'s overlay-local coordinates, if the cursor is on that monitor.
    /// The pointer as `who` sees it: the button counts as up while someone else owns the press.
    fn local_for(&self, m: &MonitorInfo, who: Owner) -> Option<Pointer> {
        let mut p = self.local(m)?;
        if self.owner != Owner::Nobody && self.owner != who {
            p.down = false;
        }
        Some(p)
    }

    fn local(&self, m: &MonitorInfo) -> Option<Pointer> {
        let r = self.raw?;
        let (x, y, w, h) = m.rect;
        if r.x < x || r.y < y || r.x >= x + w || r.y >= y + h {
            return None;
        }
        Some(Pointer {
            pos: [(r.x - x) as f32, (r.y - y) as f32],
            vel: self.vel,
            down: r.down,
            over_desktop: r.over_desktop,
        })
    }
}

/// Smoothly wandering wind in -1..1.
#[derive(Default)]
struct Wind {
    value: f32,
    target: f32,
    timer: f32,
}

impl Wind {
    fn update(&mut self, dt: f32, changing: bool) -> f32 {
        if !changing {
            self.value += (0.0 - self.value) * (dt * 0.5).min(1.0);
            return self.value;
        }
        self.timer -= dt;
        if self.timer <= 0.0 {
            self.timer = 8.0 + fastrand::f32() * 12.0;
            self.target = (fastrand::f32() * 2.0 - 1.0) * 0.8;
        }
        self.value += (self.target - self.value) * (dt * 0.3).min(1.0);
        self.value
    }
}

/// Overlay-local y of the floor: bottom of the work area (top of a bottom taskbar), so
/// snow lies on the taskbar instead of being drawn over its icons.
fn ground_y(m: &MonitorInfo) -> f32 {
    let (_, my, _, _) = m.rect;
    let (_, wy, _, wh) = m.work_rect;
    (wy + wh - my) as f32
}

fn month_now() -> u32 {
    // Month from days since epoch (UTC is fine for season granularity).
    let days = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    month_from_days(days)
}

fn season_effect() -> EffectKind {
    match month_now() {
        3..=5 => EffectKind::Sakura,
        6..=8 => EffectKind::Fireflies,
        9..=11 => EffectKind::Leaves,
        _ => EffectKind::Snow,
    }
}

/// Civil-from-days (Howard Hinnant), returns month 1..=12.
fn month_from_days(z: i64) -> u32 {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    (if mp < 10 { mp + 3 } else { mp - 9 }) as u32
}

fn lang_of(s: &Settings) -> String {
    if s.language.is_empty() {
        i18n::resolve_lang(&snowy_sys::ui_language()).to_string()
    } else {
        i18n::resolve_lang(&s.language).to_string()
    }
}

/// Debug builds never touch the Run key, so `cargo run` doesn't register a dev exe.
fn sync_autostart(enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    if let Err(e) = snowy_sys::set_autostart(enabled) {
        log::warn!("autostart: {e:#}");
    }
}

/// Log sink: stderr in debug builds; in release (no console) `%APPDATA%\Snowy\snowy.log`,
/// so problems on other PCs can be diagnosed. The previous run's log is kept as
/// `snowy.old.log`; one run writes at most `LOG_CAP` bytes.
struct Logger {
    file: Option<std::sync::Mutex<(std::fs::File, u64)>>,
    start: Instant,
}

const LOG_CAP: u64 = 1 << 20;

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        let t = self.start.elapsed().as_secs_f32();
        let line = format!("[{t:9.2}] {} {}: {}\n", r.level(), r.target(), r.args());
        match &self.file {
            Some(f) => {
                use std::io::Write;
                if let Ok(mut g) = f.lock() {
                    if g.1 < LOG_CAP && g.0.write_all(line.as_bytes()).is_ok() {
                        g.1 += line.len() as u64;
                    }
                }
            }
            None => eprint!("{line}"),
        }
    }
    fn flush(&self) {}
}

fn init_logger() {
    let file = if cfg!(debug_assertions) {
        None
    } else {
        let dir = Settings::dir();
        let path = dir.join("snowy.log");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::rename(&path, dir.join("snowy.old.log"));
        std::fs::File::create(&path).ok().map(|f| std::sync::Mutex::new((f, 0)))
    };
    let logger: &'static Logger = Box::leak(Box::new(Logger { file, start: Instant::now() }));
    let _ = log::set_logger(logger);
    log::set_max_level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info });
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        default_hook(info);
    }));
}

/// What the log needs to explain "nothing shows up" on someone else's PC.
fn log_startup(s: &Settings) {
    log::info!("Snowy {} starting", env!("CARGO_PKG_VERSION"));
    log::info!(
        "settings: effect {:?}, behind windows {}, pause on battery {} (on battery now: {}), \
         pause in fullscreen {} (fullscreen now: {}), disabled monitors {:?}",
        s.effect,
        s.behind_windows,
        s.pause_on_battery,
        snowy_sys::on_battery(),
        s.pause_on_fullscreen,
        snowy_sys::foreground_is_fullscreen(),
        s.disabled_monitors,
    );
}

fn mtime() -> Option<SystemTime> {
    std::fs::metadata(Settings::path()).and_then(|m| m.modified()).ok()
}

/// Settings and look-editor windows this process opened; closed with the app on Quit.
static CHILDREN: std::sync::Mutex<Vec<std::process::Child>> = std::sync::Mutex::new(Vec::new());

fn spawn_child(cmd: &mut std::process::Command) {
    match cmd.spawn() {
        Ok(child) => {
            if let Ok(mut c) = CHILDREN.lock() {
                // Forget windows the user already closed.
                c.retain_mut(|ch| matches!(ch.try_wait(), Ok(None)));
                c.push(child);
            }
        }
        Err(e) => log::warn!("could not open window: {e}"),
    }
}

/// Closes the windows opened by this process: politely first (like clicking X, so the
/// editor/settings finish saving), then by force after a short grace period.
fn close_children() {
    let Ok(mut children) = CHILDREN.lock() else { return };
    children.retain_mut(|ch| matches!(ch.try_wait(), Ok(None)));
    for ch in children.iter() {
        snowy_sys::close_process_windows(ch.id());
    }
    let deadline = Instant::now() + Duration::from_millis(1500);
    while !children.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        children.retain_mut(|ch| matches!(ch.try_wait(), Ok(None)));
    }
    for ch in children.iter_mut() {
        log::warn!("window process {} did not close; ending it", ch.id());
        let _ = ch.kill();
        let _ = ch.wait();
    }
    children.clear();
}

/// After an update, opens the "What's new" window once. A fresh install (no earlier Snowy
/// data) just records the version.
fn maybe_show_whats_new(mut s: Settings) -> Settings {
    let current = env!("CARGO_PKG_VERSION");
    if s.last_seen_version == current {
        return s;
    }
    let dir = Settings::dir();
    let used_before = ["settings.json", "update.json", "weather.json"].iter().any(|f| dir.join(f).exists())
        || std::fs::read_dir(&dir).is_ok_and(|mut d| d.any(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with("props-"))));
    let upgraded = s.last_seen_version.is_empty() || snowy_update::is_newer(current, &s.last_seen_version);
    if used_before && upgraded {
        if let Ok(exe) = std::env::current_exe() {
            log::info!("showing what's new ({} -> {current})", s.last_seen_version);
            spawn_child(std::process::Command::new(exe).arg("--whats-new").arg("--since").arg(&s.last_seen_version));
        }
    }
    s.last_seen_version = current.to_string();
    if let Err(e) = s.save() {
        log::warn!("saving settings: {e}");
    }
    s
}

fn open_editor(monitor_id: &str, prop_id: u64) {
    if let Ok(exe) = std::env::current_exe() {
        spawn_child(std::process::Command::new(exe).arg("--edit").arg(monitor_id).arg(prop_id.to_string()));
    }
}

fn open_settings(page: Option<&str>) {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--settings");
        if let Some(p) = page {
            cmd.arg("--page").arg(p);
        }
        spawn_child(&mut cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::month_from_days;
    #[test]
    fn months() {
        assert_eq!(month_from_days(0), 1); // 1970-01-01
        assert_eq!(month_from_days(20_772), 11); // 2026-11-15
        assert_eq!(month_from_days(20_604), 5); // 2026-05-31
    }
}
