#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Snowy: desktop weather effects. Main process = tray + overlays + render loop.
//! `snowy.exe --settings` = settings window process.

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
}

struct App {
    settings: Settings,
    settings_mtime: Option<SystemTime>,
    screens: Vec<Screen>,
    active: Option<EffectKind>,
    user_paused: bool,
    auto_paused: bool,
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
    wallpaper: WallpaperState,
}

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
        return snowy_settings::run(snowy_sys::monitors());
    }
    if let Some(i) = args.iter().position(|a| a == "--edit") {
        let monitor = args.get(i + 1).cloned().unwrap_or_default();
        let id = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        return snowy_settings::run_editor(monitor, id);
    }

    let Some(_guard) = snowy_sys::single_instance() else {
        // Already running: just open settings for the user.
        open_settings();
        return Ok(());
    };

    init_logger();
    let settings = Settings::load();
    sync_autostart(settings.start_with_windows);
    let lang = lang_of(&settings);
    let tray = Tray::new(&TrayState { effect: settings.effect, paused: false, lang })?;

    let mut app = App {
        settings_mtime: mtime(),
        settings,
        screens: Vec::new(),
        active: None,
        user_paused: false,
        auto_paused: false,
        random_since: Instant::now(),
        wind: Wind::default(),
        tray,
        pointer: PointerTracker::default(),
        grab: snowy_sys::MouseGrab::start().map_err(|e| log::warn!("mouse grab: {e:#}")).ok(),
        zones_local: Vec::new(),
        zones_screen: Vec::new(),
        weather: None,
        wallpaper: WallpaperState::default(),
    };
    app.rebuild_screens();
    app.resolve_effect(true);

    let start = Instant::now();
    let mut last = Instant::now();
    let mut last_slow_check = Instant::now() - Duration::from_secs(10);
    let mut last_scan = Instant::now() - Duration::from_secs(10);
    let mut last_monitors = snowy_sys::monitors();
    let mut prof = Profiler::new();
    let mut pacer = Pacer::new();

    loop {
        if !snowy_sys::pump_messages() {
            break;
        }
        while let Some(cmd) = app.tray.poll() {
            if !app.handle(cmd) {
                return Ok(());
            }
        }

        // Once per second: settings file, monitors, pause conditions, random rotation.
        if last_slow_check.elapsed() >= Duration::from_secs(1) {
            last_slow_check = Instant::now();
            let m = mtime();
            if m != app.settings_mtime {
                app.settings_mtime = m;
                app.apply_settings(Settings::load());
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
            app.auto_paused = (app.settings.pause_on_battery && snowy_sys::on_battery())
                || (app.settings.pause_on_fullscreen && snowy_sys::foreground_is_fullscreen());
            app.resolve_effect(false);
            app.tick_wallpaper();
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
        }
        let mut raw = if running { snowy_sys::pointer_sample() } else { None };
        // A swallowed press never reaches the system's key state, so ask the hook too.
        if let (Some(r), Some(g)) = (raw.as_mut(), app.grab.as_ref()) {
            r.down |= g.captured();
        }
        app.pointer.update(raw, dt);
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
                        pointer: app.pointer.local(&s.monitor),
                        snowballs,
                    };
                    let t0 = Instant::now();
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
                log::warn!("render failed on {}: {e:#}", s.monitor.id);
            }
        }
        app.update_props(dt, &env, props_running);
        app.publish_grab_zones(running, props_running);
        let work = now.elapsed();

        // Frame pacing. Running: present on vblanks (DwmFlush), aiming at an accumulating
        // deadline so the *average* rate equals the FPS cap even when it isn't a divisor of
        // the refresh rate (60 fps on 144 Hz alternates 2 and 3 vblanks).
        // Idle: poll slowly to stay near 0% CPU.
        if running {
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
            TrayCommand::OpenSettings => open_settings(),
            TrayCommand::Quit => return false,
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
        for m in snowy_sys::monitors() {
            if self.settings.disabled_monitors.contains(&m.id) {
                continue;
            }
            match OverlayWindow::new(&m, self.settings.behind_windows) {
                Ok(overlay) => {
                    let props_overlay = OverlayWindow::new(&m, true)
                        .map_err(|e| log::warn!("props overlay for {} failed: {e:#}", m.id))
                        .ok();
                    let (w, h) = overlay.size();
                    let props = Layer::new(&m, w as f32, h as f32, ground_y(&m));
                    self.screens.push(Screen { monitor: m, overlay, effect: None, surfaces: Vec::new(), props_overlay, props });
                }
                Err(e) => log::error!("overlay for {} failed: {e:#}", m.id),
            }
        }
        self.active = None;
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
    fn update_props(&mut self, dt: f32, env: &Env, running: bool) {
        let mut events: Vec<WorldEvent> = Vec::new();
        let mut actions: Vec<Action> = Vec::new();
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
                    pointer: self.pointer.local(&s.monitor),
                    snowballs: false,
                };
                s.props.update(dt, env, &scene);
                ov.render(&s.props, env.time)
            } else {
                ov.clear()
            };
            if let Err(e) = res {
                log::warn!("props render failed on {}: {e:#}", s.monitor.id);
            }
            actions.clear();
            s.props.take_actions(&mut actions);
            for a in actions.drain(..) {
                match a {
                    Action::OpenEditor { prop_id } => open_editor(&s.monitor.id, prop_id),
                    Action::ClearGround { x0, x1 } => {
                        if let Some(e) = &mut s.effect {
                            e.clear_region(x0, x1);
                        }
                    }
                }
            }
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

    fn refresh_tray(&mut self) {
        let state = TrayState {
            effect: self.settings.effect,
            paused: self.user_paused,
            lang: lang_of(&self.settings),
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

/// Cursor position (virtual screen) with smoothed velocity.
#[derive(Default)]
struct PointerTracker {
    raw: Option<snowy_sys::RawPointer>,
    vel: [f32; 2],
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

struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, r: &log::Record) {
        eprintln!("[{}] {}: {}", r.level(), r.target(), r.args());
    }
    fn flush(&self) {}
}

fn init_logger() {
    static LOGGER: StderrLogger = StderrLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Warn });
}

fn mtime() -> Option<SystemTime> {
    std::fs::metadata(Settings::path()).and_then(|m| m.modified()).ok()
}

fn open_editor(monitor_id: &str, prop_id: u64) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--edit").arg(monitor_id).arg(prop_id.to_string()).spawn();
    }
}

fn open_settings() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--settings").spawn();
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
