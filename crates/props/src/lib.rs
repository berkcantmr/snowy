//! Desktop props layer for one monitor: snowmen, Christmas trees and the snowplough,
//! drawn behind windows on their own overlay. Handles persistence, dragging and clicks.
//! FROZEN CONTRACT: the pub API of `Layer` and `Action`.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::path::PathBuf;
use std::time::SystemTime;

use snowy_art::Prim;
use snowy_core::props::{sanitize_prop, LightsLook, Prop, PropKind, PropsFile, SnowmanLook, TreeLook, DEFAULT_LIGHTS};
use snowy_core::{Effect, Env, Instance, MonitorInfo, Scene, WorldEvent};

/// Something the app must do on behalf of the layer.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Open the look editor for this prop (app spawns `snowy.exe --edit <monitor_id> <prop_id>`).
    OpenEditor { prop_id: u64 },
    /// The plough cleared ground between x0..x1 this frame: forward to the weather effect's
    /// `clear_region` on the same monitor.
    ClearGround { x0: f32, x1: f32 },
    /// The user pressed a prop that is locked because it sits next to desktop icons (outside
    /// edit mode). The app explains how to turn edit mode on. `pos` = overlay-local press point.
    LockedHint { pos: [f32; 2] },
}

const SNOWMAN_H: f32 = 150.0;
const TREE_H: f32 = 220.0;
const MAX_PROPS: usize = 12;
const GROW_SECS: f32 = 2.0;
const PLOUGH_H: f32 = 70.0;
const PLOUGH_SPEED: f32 = 260.0;
const DRAG_PX: f32 = 6.0;
const CLICK_SECS: f32 = 0.6;
const SAVE_DEBOUNCE: f32 = 0.5;
const ZONE_PAD: f32 = 4.0;
const YETI_H: f32 = 170.0;
const YETI_SPEED: f32 = 140.0;
const YETI_FLEE_SPEED: f32 = 520.0;
const LIFT_SECS: f32 = 1.0;
const DROP_SECS: f32 = 0.4;
const GIFT_H: f32 = 90.0;
const GIFT_DELAY: f32 = 10.0;
const GIFT_FALL_SPEED: f32 = 110.0;
const OPEN_SECS: f32 = 0.8;
const CONFETTI_MAX: usize = 80;
/// String lights: sag as a fraction of the string length, grab radius of the end handles.
const LIGHTS_SAG: f32 = 0.06;
const END_GRAB: f32 = 16.0;
/// Light ends closer than this (px at scale 1) snap together.
const END_SNAP: f32 = 22.0;
/// Ends this close count as joined (they move together when one is dragged).
const END_JOINED: f32 = 1.5;
/// A prop whose grab area comes this close to a desktop icon is locked outside edit mode.
const ICON_MARGIN: f32 = 6.0;

/// Days since 1970-01-01 of a civil date.
fn days_from_civil(y: i32, m: u32, d: u32) -> u32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i32;
    let doy = (153 * mp + 2) / 5 + d as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468).max(0) as u32
}

/// Local calendar day as days since the Unix epoch.
fn local_day() -> u32 {
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime has no preconditions.
        let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        days_from_civil(st.wYear as i32, st.wMonth as u32, st.wDay as u32)
    }
    #[cfg(not(windows))]
    {
        let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        (secs / 86_400) as u32
    }
}

#[derive(Clone, Copy, PartialEq)]
enum YStage {
    Approach,
    Lift,
    Leave,
    Flee,
}

struct Yeti {
    x: f32,
    dir: f32,
    /// Direction it walks off in (back where it came from).
    exit_dir: f32,
    phase: f32,
    stage: YStage,
    t: f32,
    /// Prop id of the snowman it is lifting / carrying.
    carry: Option<u64>,
}

/// A snowman falling back to the ground after being dropped.
struct Drop {
    id: u64,
    t: f32,
    from: [f32; 2],
}

#[derive(Clone, Copy, PartialEq)]
enum GStage {
    Falling,
    Idle,
    Opening,
}

struct Gift {
    x: f32,
    y: f32,
    stage: GStage,
    t: f32,
    seed: f32,
}

struct Particle {
    pos: [f32; 2],
    vel: [f32; 2],
    rot: f32,
    spin: f32,
    color: [f32; 4],
    life: f32,
    size: f32,
    star: bool,
    flutter: f32,
}

/// Where the props file lives (overridable for tests).
struct Store {
    monitor_id: String,
    custom: Option<PathBuf>,
}

impl Store {
    fn path(&self) -> PathBuf {
        self.custom.clone().unwrap_or_else(|| PropsFile::path(&self.monitor_id))
    }

    fn load(&self) -> PropsFile {
        match &self.custom {
            None => PropsFile::load(&self.monitor_id),
            Some(p) => {
                let mut f: PropsFile = std::fs::read_to_string(p)
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default();
                for pr in &mut f.props {
                    sanitize_prop(pr);
                }
                f
            }
        }
    }

    fn save(&self, f: &PropsFile) -> Result<(), String> {
        match &self.custom {
            None => f.save(&self.monitor_id).map_err(|e| e.to_string()),
            Some(p) => {
                let tmp = p.with_extension("json.tmp");
                let s = serde_json::to_string_pretty(f).map_err(|e| e.to_string())?;
                std::fs::write(&tmp, s).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, p).map_err(|e| e.to_string())
            }
        }
    }

    fn mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(self.path()).and_then(|m| m.modified()).ok()
    }
}

/// Per-prop transient animation state (never saved).
#[derive(Clone, Copy)]
struct Trans {
    grow: f32,
    next_sway: f32,
    sway_t: f32,
    sway_dur: f32,
    hover: f32,
    tilt: f32,
}

impl Trans {
    fn new() -> Self {
        Self { grow: 1.0, next_sway: 6.0 + fastrand::f32() * 8.0, sway_t: -1.0, sway_dur: 1.0, hover: 0.0, tilt: 0.0 }
    }

    fn wobble(&self) -> f32 {
        if self.sway_t < 0.0 {
            return 0.0;
        }
        let p = (self.sway_t / self.sway_dur).clamp(0.0, 1.0);
        0.07 * (p * std::f32::consts::TAU * 2.0).sin() * (p * std::f32::consts::PI).sin()
    }
}

/// Which part of a prop a press grabbed (string lights have draggable ends).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Part {
    Move,
    EndA,
    EndB,
}

struct Press {
    part: Part,
    /// Endpoints of a lights string when the press began (fractions).
    orig: [[f32; 2]; 2],
    id: u64,
    start: [f32; 2],
    t: f32,
    dragging: bool,
    off: f32,
    /// Ends of other strings joined to the grabbed end: (prop id, is end A).
    linked: Vec<(u64, bool)>,
}

struct Plough {
    x: f32,
    dir: f32,
    t: f32,
}

pub struct Layer {
    monitor_id: String,
    store: Store,
    file: PropsFile,
    inst: Vec<Instance>,
    width: f32,
    height: f32,
    ground: f32,
    time: f32,
    trans: HashMap<u64, Trans>,
    zones: Vec<[f32; 4]>,
    /// Grab boxes of string lights: (prop index, part, box).
    lzones: Vec<(usize, Part, [f32; 4])>,
    order: Vec<usize>,
    scratch: Vec<Prim>,
    actions: Vec<Action>,
    press: Option<Press>,
    prev_down: bool,
    dirty: bool,
    since_save: f32,
    own_mtime: Option<SystemTime>,
    plough: Option<Plough>,
    next_plough_dir: f32,
    primary: bool,
    yeti: Option<Yeti>,
    yeti_zone: Option<[f32; 4]>,
    next_visit: f32,
    drop: Option<Drop>,
    gift: Option<Gift>,
    /// Gifts from friends waiting for the current gift to be opened.
    pending_gifts: u32,
    gift_zone: Option<[f32; 4]>,
    uptime: f32,
    day_check: f32,
    /// Test hook: pretend today is this day.
    day_override: Option<u32>,
    confetti: Vec<Particle>,
    /// Desktop icon boxes (overlay-local px).
    icons: Vec<[f32; 4]>,
    /// Edit mode: props next to desktop icons can be moved too.
    edit_mode: bool,
    /// Per prop index: next to an icon and not in edit mode, so clicks go to the desktop.
    locked: Vec<bool>,
}

fn overlaps(a: &[f32; 4], b: &[f32; 4], m: f32) -> bool {
    a[0] - m < b[2] && b[0] < a[2] + m && a[1] - m < b[3] && b[1] < a[3] + m
}

fn is_snowman(p: &Prop) -> bool {
    matches!(p.kind, PropKind::Snowman { .. })
}

fn is_lights(p: &Prop) -> bool {
    matches!(p.kind, PropKind::Lights { .. })
}

/// Back-to-front draw class: lights, then trees, then snowmen.
fn class(p: &Prop) -> u8 {
    match p.kind {
        PropKind::Lights { .. } => 0,
        PropKind::Tree { .. } => 1,
        PropKind::Snowman { .. } => 2,
    }
}

fn base_height(p: &Prop) -> f32 {
    if is_snowman(p) {
        SNOWMAN_H
    } else {
        TREE_H
    }
}

impl Layer {
    /// Load this monitor's props. `width`/`height` = overlay size (physical px),
    /// `ground` = overlay-local y where props stand (work-area bottom).
    pub fn new(monitor: &MonitorInfo, width: f32, height: f32, ground: f32) -> Self {
        Self::build(Store { monitor_id: monitor.id.clone(), custom: None }, monitor.primary, width, height, ground)
    }

    #[cfg(test)]
    fn with_store(monitor: &MonitorInfo, width: f32, height: f32, ground: f32, path: PathBuf) -> Self {
        Self::build(Store { monitor_id: monitor.id.clone(), custom: Some(path) }, monitor.primary, width, height, ground)
    }

    fn build(store: Store, primary: bool, width: f32, height: f32, ground: f32) -> Self {
        let file = store.load();
        let own_mtime = store.mtime();
        let mut l = Self {
            monitor_id: store.monitor_id.clone(),
            store,
            file,
            inst: Vec::new(),
            width,
            height,
            ground,
            time: 0.0,
            trans: HashMap::new(),
            zones: Vec::new(),
            lzones: Vec::new(),
            order: Vec::new(),
            scratch: Vec::new(),
            actions: Vec::new(),
            press: None,
            prev_down: false,
            dirty: false,
            since_save: 1.0,
            own_mtime,
            plough: None,
            next_plough_dir: 1.0,
            primary,
            yeti: None,
            yeti_zone: None,
            next_visit: 1500.0 + fastrand::f32() * 1500.0,
            drop: None,
            gift: None,
            pending_gifts: 0,
            gift_zone: None,
            uptime: 0.0,
            day_check: 0.0,
            day_override: None,
            confetti: Vec::new(),
            icons: Vec::new(),
            edit_mode: false,
            locked: Vec::new(),
        };
        l.sync_trans();
        l.rebuild();
        l
    }

    /// Monitor geometry changed.
    pub fn set_geometry(&mut self, width: f32, height: f32, ground: f32) {
        self.width = width;
        self.height = height;
        self.ground = ground;
        self.rebuild();
    }

    /// A friend sent a gift: one floats down now (primary screen only; at most one at a time).
    /// Returns false when it can't be shown here.
    pub fn drop_gift(&mut self) -> bool {
        if !self.primary {
            return false;
        }
        if self.gift.is_some() {
            // One at a time: this one comes down once the current box is opened.
            self.pending_gifts = (self.pending_gifts + 1).min(5);
            return true;
        }
        let sc = self.sc();
        let x = self.free_spot(SNOWMAN_H * 0.6) * self.width;
        self.gift = Some(Gift { x, y: -1.3 * GIFT_H * sc, stage: GStage::Falling, t: 0.0, seed: fastrand::f32() * TAU });
        true
    }

    /// True when a press at `pos` would act on a decoration: drag or open an unlocked prop,
    /// scare the yeti or open the gift.
    pub fn hits(&self, pos: [f32; 2]) -> bool {
        let inside = |z: Option<[f32; 4]>| z.is_some_and(|z| pos[0] >= z[0] && pos[0] <= z[2] && pos[1] >= z[1] && pos[1] <= z[3]);
        inside(self.yeti_zone)
            || inside(self.gift_zone)
            || self.hit_part(pos).is_some_and(|(i, _)| !self.locked.get(i).copied().unwrap_or(false))
    }

    /// True when there is nothing to draw (lets the app skip rendering this overlay).
    pub fn is_empty(&self) -> bool {
        self.file.props.is_empty()
            && self.plough.is_none()
            && self.yeti.is_none()
            && self.gift.is_none()
            && self.drop.is_none()
            && self.confetti.is_empty()
    }

    /// Plant a snowman (3 balls, default look) at a free spot near the middle. Saves.
    pub fn add_snowman(&mut self) {
        let x = self.free_spot(SNOWMAN_H * 0.6);
        self.insert(PropKind::Snowman { look: SnowmanLook::default(), balls: 3 }, x, 1.0, false);
    }

    /// Plant a Christmas tree (grows in over ~2 s) at a free spot. Saves.
    pub fn add_tree(&mut self) {
        let x = self.free_spot(TREE_H * 0.7);
        let look = TreeLook { lights: 1, ornaments: 3, topper: 1, garland: 0 };
        self.insert(PropKind::Tree { look }, x, 1.0, true);
    }

    /// Hang a string of lights across the upper middle of the screen (further ones a bit
    /// lower). Saves.
    pub fn add_lights(&mut self) {
        let n = self.file.props.iter().filter(|p| is_lights(p)).count();
        let dy = (0.07 * n as f32).min(0.5);
        let (a, b) = ([DEFAULT_LIGHTS.0[0], DEFAULT_LIGHTS.0[1] + dy], [DEFAULT_LIGHTS.1[0], DEFAULT_LIGHTS.1[1] + dy]);
        let look = LightsLook { colors: 1, bulbs: 0, pattern: 1, wire: 0 };
        self.insert(PropKind::Lights { look, a, b }, 0.5, 1.0, false);
    }

    /// Send the snowplough across this monitor's ground.
    pub fn send_plough(&mut self) {
        if self.plough.is_some() {
            return;
        }
        let dir = self.next_plough_dir;
        self.next_plough_dir = -dir;
        let half = self.plough_half();
        let x = if dir > 0.0 { -1.05 * half } else { self.width + 1.05 * half };
        self.plough = Some(Plough { x, dir, t: 0.0 });
        self.rebuild();
    }

    /// Call the yeti now. Does nothing without a snowman on this monitor or while a yeti is
    /// already here.
    pub fn call_yeti(&mut self) {
        if self.yeti.is_some() {
            return;
        }
        let w = self.width.max(1.0);
        let edge = |x: f32| (x * w).min((1.0 - x) * w);
        let best = self
            .file
            .props
            .iter()
            .filter(|p| is_snowman(p))
            .min_by(|a, b| edge(a.x).partial_cmp(&edge(b.x)).unwrap_or(std::cmp::Ordering::Equal));
        let Some(best) = best else { return };
        let from_left = best.x < 0.5;
        let half = 0.6 * YETI_H * self.sc();
        let (x, dir) = if from_left { (-half, 1.0) } else { (w + half, -1.0) };
        self.yeti = Some(Yeti { x, dir, exit_dir: -dir, phase: 0.0, stage: YStage::Approach, t: 0.0, carry: None });
        self.rebuild();
    }

    /// React to weather-effect events on this monitor (e.g. snowman built from snowballs).
    pub fn handle(&mut self, ev: WorldEvent) {
        let WorldEvent::SnowmanBuilt { x, balls, height } = ev;
        let frac = if self.width > 0.0 { (x / self.width).clamp(0.0, 1.0) } else { 0.5 };
        let sc = self.sc();
        let scale = if sc > 0.0 { height / (SNOWMAN_H * sc) } else { 1.0 };
        let scale = if scale.is_finite() { scale.clamp(0.6, 1.6) } else { 1.0 };
        let kind = PropKind::Snowman { look: SnowmanLook::default(), balls: balls.clamp(2, 3) };
        self.insert(kind, frac, scale, false);
    }

    /// Reload from disk if the file changed (editor saved). Call ~once per second.
    pub fn poll_reload(&mut self) {
        let m = self.store.mtime();
        if m == self.own_mtime || self.dirty || self.press.as_ref().is_some_and(|p| p.dragging) {
            return;
        }
        self.own_mtime = m;
        self.file = self.store.load();
        if let Some(pr) = &self.press {
            if !self.file.props.iter().any(|p| p.id == pr.id) {
                self.press = None;
            }
        }
        self.sync_trans();
        self.rebuild();
    }

    /// Desktop icon boxes in overlay-local px (props next to them lock outside edit mode).
    pub fn set_desktop_icons(&mut self, icons: &[[f32; 4]]) {
        if self.icons != icons {
            self.icons.clear();
            self.icons.extend_from_slice(icons);
            self.rebuild();
        }
    }

    /// Edit mode: every prop can be dragged, even next to desktop icons.
    pub fn set_edit_mode(&mut self, on: bool) {
        if self.edit_mode != on {
            self.edit_mode = on;
            self.rebuild();
        }
    }

    /// True when some prop is locked because it is next to desktop icons.
    pub fn has_locked(&self) -> bool {
        self.locked.iter().any(|&l| l)
    }

    /// Drain pending actions.
    pub fn take_actions(&mut self, out: &mut Vec<Action>) {
        out.append(&mut self.actions);
    }

    pub fn monitor_id(&self) -> &str {
        &self.monitor_id
    }

    // ---- internals ----

    fn sc(&self) -> f32 {
        self.height / 1080.0
    }

    /// Distance from the plough's center to the front of its blade, measured from the art
    /// itself so the cleared strip matches what is drawn.
    fn plough_half(&self) -> f32 {
        let h = PLOUGH_H * self.sc();
        let mut prims = Vec::new();
        snowy_art::plough([0.0, 0.0], h, 1.0, 0.0, &mut prims);
        let b = snowy_art::bounds(&prims);
        if b[2].is_finite() && b[2] > 0.0 { b[2] } else { 0.9 * h }
    }

    fn prop_width(&self, p: &Prop) -> f32 {
        if is_lights(p) {
            return 0.0;
        }
        let f = if is_snowman(p) { 0.6 } else { 0.7 };
        base_height(p) * f * self.sc() * p.scale
    }

    fn free_spot(&self, new_w_ref: f32) -> f32 {
        let new_w = new_w_ref * self.sc();
        let w = self.width.max(1.0);
        let step = (new_w / w).max(0.02);
        let ok = |x: f32| {
            self.file.props.iter().filter(|p| !is_lights(p)).all(|p| {
                let min = (self.prop_width(p) + new_w) * 0.5;
                (p.x * w - x * w).abs() >= min
            })
        };
        for k in 0..40 {
            let n = (k + 1) / 2;
            let sign = if k % 2 == 1 { -1.0 } else { 1.0 };
            let x = 0.5 + sign * n as f32 * step;
            if (0.05..=0.95).contains(&x) && ok(x) {
                return x;
            }
        }
        0.05 + fastrand::f32() * 0.9
    }

    fn insert(&mut self, kind: PropKind, x: f32, scale: f32, grow: bool) {
        if self.file.props.len() >= MAX_PROPS {
            let cls = class(&Prop { id: 0, kind, x: 0.0, scale: 1.0 });
            let idx = self.file.props.iter().position(|p| class(p) == cls).unwrap_or(0);
            let old = self.file.props.remove(idx);
            self.trans.remove(&old.id);
            if self.press.as_ref().is_some_and(|p| p.id == old.id) {
                self.press = None;
            }
        }
        let mut id = fastrand::u64(..);
        while self.file.props.iter().any(|p| p.id == id) {
            id = fastrand::u64(..);
        }
        self.file.props.push(Prop { id, kind, x: x.clamp(0.0, 1.0), scale });
        let mut tr = Trans::new();
        if grow {
            tr.grow = 0.0;
        }
        self.trans.insert(id, tr);
        self.save_now();
        self.rebuild();
    }

    fn today(&self) -> u32 {
        self.day_override.unwrap_or_else(local_day)
    }

    fn snowman_h(&self, p: &Prop) -> f32 {
        SNOWMAN_H * self.sc() * p.scale
    }

    /// Where the carried snowman's base is, given the yeti and how far the lift has come.
    fn carried_base(&self, y: &Yeti, src_x: f32) -> ([f32; 2], f32) {
        let yh = YETI_H * self.sc();
        let p = if y.stage == YStage::Lift { (y.t / LIFT_SECS).clamp(0.0, 1.0) } else { 1.0 };
        let e = p * p * (3.0 - 2.0 * p);
        let top = [y.x + y.dir * 0.02 * yh, self.ground - 0.97 * yh];
        let arc = (p * PI).sin() * 0.25 * yh;
        let x = src_x + (top[0] - src_x) * e;
        let yy = self.ground + (top[1] - self.ground) * e - arc;
        ([x, yy], 1.0 - 0.3 * e)
    }

    fn scare_yeti(&mut self) {
        let Some(mut y) = self.yeti.take() else { return };
        if y.stage == YStage::Flee {
            self.yeti = Some(y);
            return;
        }
        if let Some(id) = y.carry.take() {
            if let Some(p) = self.file.props.iter().find(|p| p.id == id).copied() {
                let (b, _) = self.carried_base(&y, p.x * self.width);
                self.drop = Some(Drop { id, t: 0.0, from: b });
            }
        }
        y.stage = YStage::Flee;
        y.t = 0.0;
        y.dir = if y.x < self.width * 0.5 { -1.0 } else { 1.0 };
        self.yeti = Some(y);
    }

    fn update_yeti(&mut self, dt: f32, dragging: Option<u64>) {
        let sc = self.sc();
        let yh = YETI_H * sc;
        let w = self.width;
        if self.yeti.is_none() {
            self.next_visit -= dt;
            if self.next_visit <= 0.0 {
                self.next_visit = 1500.0 + fastrand::f32() * 1500.0;
                self.call_yeti();
            }
        }
        let Some(mut y) = self.yeti.take() else { return };
        // The carried snowman vanished (reload, prop cap): carry on without it.
        if let Some(id) = y.carry {
            if !self.file.props.iter().any(|p| p.id == id) {
                y.carry = None;
                if y.stage == YStage::Lift {
                    y.stage = YStage::Leave;
                    y.dir = y.exit_dir;
                }
            }
        }
        let off = |x: f32| x < -0.6 * yh || x > w + 0.6 * yh;
        let mut done = false;
        match y.stage {
            YStage::Approach => {
                let target = self
                    .file
                    .props
                    .iter()
                    .filter(|p| is_snowman(p))
                    .min_by(|a, b| {
                        let (da, db) = ((a.x * w - y.x).abs(), (b.x * w - y.x).abs());
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .copied();
                match target {
                    None => {
                        y.stage = YStage::Leave;
                        y.dir = y.exit_dir;
                    }
                    Some(tp) => {
                        let dx = tp.x * w - y.x;
                        let stand = 0.42 * yh;
                        y.dir = if dx >= 0.0 { 1.0 } else { -1.0 };
                        if dx.abs() <= stand * 1.05 {
                            if dragging != Some(tp.id) {
                                y.stage = YStage::Lift;
                                y.t = 0.0;
                                y.carry = Some(tp.id);
                            }
                        } else {
                            let step = (YETI_SPEED * sc * dt).min(dx.abs() - stand);
                            y.x += y.dir * step;
                            y.phase = (y.phase + YETI_SPEED * sc * dt / (0.3 * yh)) % TAU;
                        }
                    }
                }
            }
            YStage::Lift => {
                y.t += dt;
                if y.t >= LIFT_SECS {
                    y.stage = YStage::Leave;
                    y.dir = y.exit_dir;
                }
            }
            YStage::Leave => {
                y.x += y.dir * YETI_SPEED * sc * dt;
                y.phase = (y.phase + YETI_SPEED * sc * dt / (0.3 * yh)) % TAU;
                if off(y.x) {
                    if let Some(id) = y.carry {
                        self.file.props.retain(|p| p.id != id);
                        self.trans.remove(&id);
                        self.save_now();
                    }
                    done = true;
                }
            }
            YStage::Flee => {
                y.t += dt;
                y.x += y.dir * YETI_FLEE_SPEED * sc * dt;
                y.phase = (y.phase + YETI_FLEE_SPEED * sc * dt / (0.3 * yh)) % TAU;
                done = off(y.x);
            }
        }
        if !done {
            self.yeti = Some(y);
        }
    }

    fn update_drop(&mut self, dt: f32) {
        if let Some(d) = &mut self.drop {
            d.t += dt;
            if d.t >= DROP_SECS {
                self.drop = None;
            }
        }
    }

    fn spawn_confetti(&mut self, x: f32, y: f32) {
        let sc = self.sc();
        for i in 0..46 {
            if self.confetti.len() >= CONFETTI_MAX {
                break;
            }
            let a = -PI / 2.0 + (fastrand::f32() - 0.5) * 2.4;
            let sp = (260.0 + fastrand::f32() * 520.0) * sc;
            let col = match i % 4 {
                0 => [1.0, 0.35, 0.4, 1.0],
                1 => [1.0, 0.85, 0.3, 1.0],
                2 => [0.4, 0.8, 1.0, 1.0],
                _ => [0.55, 1.0, 0.6, 1.0],
            };
            self.confetti.push(Particle {
                pos: [x, y],
                vel: [a.cos() * sp, a.sin() * sp],
                rot: fastrand::f32() * TAU,
                spin: (fastrand::f32() - 0.5) * 20.0,
                color: col,
                life: 1.2 + fastrand::f32() * 0.9,
                size: (3.5 + fastrand::f32() * 3.5) * sc,
                star: i % 3 == 0,
                flutter: fastrand::f32() * TAU,
            });
        }
    }

    fn update_confetti(&mut self, dt: f32) {
        let sc = self.sc();
        let ground = self.ground + 10.0 * sc;
        for p in &mut self.confetti {
            p.vel[1] += 900.0 * sc * dt;
            let drag = (1.0 - 1.8 * dt).max(0.0);
            p.vel[0] *= drag;
            p.vel[1] *= if p.vel[1] > 0.0 { (1.0 - 2.5 * dt).max(0.0) } else { drag };
            p.pos[0] += p.vel[0] * dt;
            p.pos[1] += p.vel[1] * dt;
            p.rot += p.spin * dt;
            p.life -= dt;
            if p.pos[1] > ground {
                p.pos[1] = ground;
                p.vel = [0.0; 2];
                p.spin = 0.0;
                p.life -= dt * 2.0;
            }
        }
        self.confetti.retain(|p| p.life > 0.0);
    }

    fn update_gift(&mut self, dt: f32) {
        let sc = self.sc();
        if self.primary && self.gift.is_none() && self.pending_gifts > 0 {
            self.pending_gifts -= 1;
            self.drop_gift();
        }
        if self.primary {
            self.uptime += dt;
            if self.gift.is_none() && self.uptime >= GIFT_DELAY {
                self.day_check -= dt;
                if self.day_check <= 0.0 {
                    self.day_check = 1.0;
                    let today = self.today();
                    if self.file.last_gift_day != today {
                        self.file.last_gift_day = today;
                        self.save_now();
                        let x = self.free_spot(SNOWMAN_H * 0.6) * self.width;
                        let y = -1.3 * GIFT_H * sc;
                        self.gift = Some(Gift { x, y, stage: GStage::Falling, t: 0.0, seed: fastrand::f32() * TAU });
                    }
                }
            }
        }
        let ground = self.ground;
        let mut reveal = None;
        if let Some(g) = &mut self.gift {
            g.t += dt;
            match g.stage {
                GStage::Falling => {
                    g.y += GIFT_FALL_SPEED * sc * dt;
                    if g.y >= ground {
                        g.y = ground;
                        g.stage = GStage::Idle;
                        g.t = 0.0;
                    }
                }
                GStage::Idle => {}
                GStage::Opening => {
                    if g.t >= OPEN_SECS {
                        reveal = Some(g.x);
                    }
                }
            }
        }
        if let Some(x) = reveal {
            self.gift = None;
            let mut look = SnowmanLook::default();
            for s in 0..4 {
                look.set(s, fastrand::u8(0..snowy_core::props::OPTIONS));
            }
            let balls = 2 + fastrand::u8(0..2);
            let frac = (x / self.width.max(1.0)).clamp(0.02, 0.98);
            self.insert(PropKind::Snowman { look, balls }, frac, 1.0, false);
            if let Some(id) = self.file.props.last().map(|p| p.id) {
                if let Some(tr) = self.trans.get_mut(&id) {
                    tr.sway_t = 0.0;
                    tr.sway_dur = 1.3;
                }
            }
            self.spawn_confetti(x, ground - 0.6 * SNOWMAN_H * sc);
        }
    }

    fn sync_trans(&mut self) {
        for p in &self.file.props {
            self.trans.entry(p.id).or_insert_with(Trans::new);
        }
        let file = &self.file;
        self.trans.retain(|id, _| file.props.iter().any(|p| p.id == *id));
    }

    fn save_now(&mut self) {
        if let Err(e) = self.store.save(&self.file) {
            log::warn!("saving props for {} failed: {e}", self.monitor_id);
        }
        self.own_mtime = self.store.mtime();
        self.dirty = false;
        self.since_save = 0.0;
    }

    fn hit(&self, pos: [f32; 2]) -> Option<usize> {
        self.order.iter().rev().copied().find(|&i| {
            let z = self.zones[i];
            z[2] > z[0] && pos[0] >= z[0] && pos[0] <= z[2] && pos[1] >= z[1] && pos[1] <= z[3]
        })
    }

    /// Props first (front to back), then the ends and wire of string lights.
    fn hit_part(&self, pos: [f32; 2]) -> Option<(usize, Part)> {
        if let Some(i) = self.hit(pos) {
            return Some((i, Part::Move));
        }
        let inside = |z: &[f32; 4]| pos[0] >= z[0] && pos[0] <= z[2] && pos[1] >= z[1] && pos[1] <= z[3];
        self.lzones.iter().rev().find(|(_, _, z)| inside(z)).map(|&(i, part, _)| (i, part))
    }

    /// Wire point of a string at fraction `s` (no sway), same curve as the art.
    fn wire_at(a: [f32; 2], b: [f32; 2], sag: f32, s: f32) -> [f32; 2] {
        let k = 4.0 * s * (1.0 - s);
        [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s + sag * k]
    }

    fn lights_px(&self, a: [f32; 2], b: [f32; 2]) -> ([f32; 2], [f32; 2], f32) {
        let (ap, bp) = ([a[0] * self.width, a[1] * self.height], [b[0] * self.width, b[1] * self.height]);
        let sag = LIGHTS_SAG * (bp[0] - ap[0]).hypot(bp[1] - ap[1]);
        (ap, bp, sag)
    }

    /// Which props sit next to desktop icons (locked unless in edit mode).
    fn update_locks(&mut self) {
        let n = self.file.props.len();
        self.locked.clear();
        self.locked.resize(n, false);
        if self.edit_mode || self.icons.is_empty() {
            return;
        }
        let m = ICON_MARGIN * self.sc();
        let icons = &self.icons;
        let near = |z: &[f32; 4]| z[2] > z[0] && icons.iter().any(|ic| overlaps(z, ic, m));
        for (i, z) in self.zones.iter().enumerate() {
            if near(z) {
                self.locked[i] = true;
            }
        }
        for (i, _, z) in &self.lzones {
            if near(z) {
                self.locked[*i] = true;
            }
        }
        // A prop being dragged stays movable until it is let go.
        if let Some(pr) = self.press.as_ref().filter(|p| p.dragging) {
            if let Some(i) = self.file.props.iter().position(|p| p.id == pr.id) {
                self.locked[i] = false;
            }
        }
    }

    /// Ends of other light strings (px): (prop id, is end A, position).
    fn other_ends(&self, id: u64) -> Vec<(u64, bool, [f32; 2])> {
        let mut v = Vec::new();
        for p in &self.file.props {
            if let (PropKind::Lights { a, b, .. }, true) = (p.kind, p.id != id) {
                v.push((p.id, true, [a[0] * self.width, a[1] * self.height]));
                v.push((p.id, false, [b[0] * self.width, b[1] * self.height]));
            }
        }
        v
    }

    /// Other strings' ends sitting on the end `end` (fractions) of string `id`.
    fn joined_ends(&self, id: u64, end: [f32; 2]) -> Vec<(u64, bool)> {
        let e = [end[0] * self.width, end[1] * self.height];
        self.other_ends(id)
            .into_iter()
            .filter(|(_, _, q)| (q[0] - e[0]).hypot(q[1] - e[1]) <= END_JOINED)
            .map(|(pid, is_a, _)| (pid, is_a))
            .collect()
    }

    /// Drags string `id` by `d` (fractions of the screen) from its `orig` ends. Ends snap
    /// onto nearby ends of other strings; `linked` ends follow a dragged end (a joint).
    fn drag_lights(&mut self, id: u64, part: Part, orig: [[f32; 2]; 2], d: [f32; 2], linked: &[(u64, bool)]) {
        let (w, h) = (self.width.max(1.0), self.height.max(1.0));
        let snap = END_SNAP * self.sc();
        let others: Vec<[f32; 2]> = self
            .other_ends(id)
            .into_iter()
            .filter(|(pid, is_a, _)| !linked.contains(&(*pid, *is_a)))
            .map(|(_, _, q)| q)
            .collect();
        // Closest other end within the snap radius of `p` (px): (distance, position).
        let nearest = |p: [f32; 2]| {
            others
                .iter()
                .map(|q| ((q[0] - p[0]).hypot(q[1] - p[1]), *q))
                .filter(|(dist, _)| *dist <= snap)
                .min_by(|x, y| x.0.total_cmp(&y.0))
        };
        let c = |v: f32| v.clamp(0.0, 1.0);
        let (mut na, mut nb) = (orig[0], orig[1]);
        match part {
            Part::Move => {
                let lim = |k: usize, dk: f32| {
                    let (lo, hi) = (orig[0][k].min(orig[1][k]), orig[0][k].max(orig[1][k]));
                    dk.clamp(-lo, 1.0 - hi)
                };
                let (mut dx, mut dy) = (lim(0, d[0]), lim(1, d[1]));
                // Snap whichever of our ends is closest to another string's end.
                let best = [orig[0], orig[1]]
                    .iter()
                    .filter_map(|e| {
                        let p = [(e[0] + dx) * w, (e[1] + dy) * h];
                        nearest(p).map(|(dist, q)| (dist, [(q[0] - p[0]) / w, (q[1] - p[1]) / h]))
                    })
                    .min_by(|x, y| x.0.total_cmp(&y.0));
                if let Some((_, off)) = best {
                    dx = lim(0, dx + off[0]);
                    dy = lim(1, dy + off[1]);
                }
                na = [orig[0][0] + dx, orig[0][1] + dy];
                nb = [orig[1][0] + dx, orig[1][1] + dy];
            }
            Part::EndA | Part::EndB => {
                let o = if part == Part::EndA { orig[0] } else { orig[1] };
                let mut e = [c(o[0] + d[0]), c(o[1] + d[1])];
                if let Some((_, q)) = nearest([e[0] * w, e[1] * h]) {
                    e = [c(q[0] / w), c(q[1] / h)];
                }
                if part == Part::EndA {
                    na = e;
                } else {
                    nb = e;
                }
            }
        }
        let moved_end = match part {
            Part::EndA => Some(na),
            Part::EndB => Some(nb),
            Part::Move => None,
        };
        for p in &mut self.file.props {
            let PropKind::Lights { a, b, .. } = &mut p.kind else { continue };
            if p.id == id {
                *a = na;
                *b = nb;
            } else if let Some(e) = moved_end {
                if linked.contains(&(p.id, true)) {
                    *a = e;
                }
                if linked.contains(&(p.id, false)) {
                    *b = e;
                }
            }
        }
    }

    fn rebuild(&mut self) {
        self.inst.clear();
        let n = self.file.props.len();
        self.zones.clear();
        self.zones.resize(n, [0.0; 4]);
        self.lzones.clear();
        self.order.clear();
        self.order.extend(0..n);
        let props = &self.file.props;
        self.order.sort_unstable_by(|&a, &b| {
            let (pa, pb) = (&props[a], &props[b]);
            class(pa)
                .cmp(&class(pb))
                .then(pa.x.partial_cmp(&pb.x).unwrap_or(std::cmp::Ordering::Equal))
                .then(pa.id.cmp(&pb.id))
        });
        let sc = self.sc();
        let t = self.time;
        let mut scratch = std::mem::take(&mut self.scratch);
        for oi in 0..self.order.len() {
            let i = self.order[oi];
            let p = self.file.props[i];
            if self.yeti.as_ref().and_then(|y| y.carry) == Some(p.id) || self.drop.as_ref().is_some_and(|d| d.id == p.id) {
                continue;
            }
            let tr = self.trans.get(&p.id).copied().unwrap_or_else(Trans::new);
            if let PropKind::Lights { look, a, b } = p.kind {
                let (ap, bp, sag) = self.lights_px(a, b);
                scratch.clear();
                snowy_art::lights_scaled(&look, ap, bp, sag, t, tr.wobble(), sc, &mut scratch);
                if tr.hover > 0.02 {
                    for e in [ap, bp] {
                        let h = tr.hover.min(1.0);
                        let r = 11.0 * sc;
                        scratch.push(Prim { kind: snowy_art::PrimKind::Ellipse, pos: e, size: [r, r], rot: 0.0, color: [1.0, 1.0, 1.0, 0.22 * h], round: 0.0 });
                        scratch.push(Prim { kind: snowy_art::PrimKind::Ring, pos: e, size: [r, r], rot: 0.0, color: [1.0, 1.0, 1.0, 0.85 * h], round: 0.16 });
                    }
                }
                // Wire boxes first, then the ends (scanned back to front, so ends win).
                let len = (bp[0] - ap[0]).hypot(bp[1] - ap[1]);
                let segs = ((len / (120.0 * sc)).ceil() as usize).clamp(1, 64);
                for k in 0..segs {
                    let mut z = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                    for m in 0..=4 {
                        let s = (k as f32 + m as f32 / 4.0) / segs as f32;
                        let q = Self::wire_at(ap, bp, sag, s);
                        z[0] = z[0].min(q[0] - 12.0 * sc);
                        z[1] = z[1].min(q[1] - 8.0 * sc);
                        z[2] = z[2].max(q[0] + 12.0 * sc);
                        z[3] = z[3].max(q[1] + 34.0 * sc);
                    }
                    self.lzones.push((i, Part::Move, z));
                }
                let g = END_GRAB * sc;
                self.lzones.push((i, Part::EndA, [ap[0] - g, ap[1] - g, ap[0] + g, ap[1] + g]));
                self.lzones.push((i, Part::EndB, [bp[0] - g, bp[1] - g, bp[0] + g, bp[1] + g]));
                self.inst.extend(scratch.iter().map(snowy_art::to_instance));
                continue;
            }
            let base = [p.x * self.width, self.ground];
            let h = base_height(&p) * sc * p.scale;
            scratch.clear();
            match p.kind {
                PropKind::Lights { .. } => {}
                PropKind::Snowman { look, balls } => snowy_art::snowman(&look, balls, base, h, t, &mut scratch),
                PropKind::Tree { look } => {
                    let g = 1.0 - (1.0 - tr.grow.clamp(0.0, 1.0)).powi(3);
                    snowy_art::tree(&look, base, h, g, t, &mut scratch)
                }
            }
            transform(&mut scratch, base, 1.0 + 0.03 * tr.hover, tr.wobble() + tr.tilt);
            let b = snowy_art::bounds(&scratch);
            self.zones[i] = [b[0] - ZONE_PAD, b[1] - ZONE_PAD, b[2] + ZONE_PAD, b[3] + ZONE_PAD];
            self.inst.extend(scratch.iter().map(snowy_art::to_instance));
        }
        self.update_locks();
        self.yeti_zone = None;
        self.gift_zone = None;
        if let Some(d) = &self.drop {
            if let Some(p) = self.file.props.iter().find(|p| p.id == d.id).copied() {
                if let PropKind::Snowman { look, balls } = p.kind {
                    let f = (d.t / DROP_SECS).clamp(0.0, 1.0);
                    let x = d.from[0] + (p.x * self.width - d.from[0]) * f;
                    let y = d.from[1] + (self.ground - d.from[1]) * f * f;
                    scratch.clear();
                    snowy_art::snowman(&look, balls, [x, y], self.snowman_h(&p), t, &mut scratch);
                    self.inst.extend(scratch.iter().map(snowy_art::to_instance));
                }
            }
        }
        if let Some(g) = &self.gift {
            let h = GIFT_H * sc;
            let sway = if g.stage == GStage::Falling { (g.t * 1.4 + g.seed).sin() * 14.0 * sc } else { 0.0 };
            let base = [g.x + sway, g.y];
            let open = if g.stage == GStage::Opening { (g.t / OPEN_SECS).clamp(0.0, 1.0) } else { 0.0 };
            scratch.clear();
            snowy_art::gift(base, h, open, t, &mut scratch);
            let b = snowy_art::bounds(&scratch);
            if g.stage == GStage::Idle {
                self.gift_zone = Some([b[0] - ZONE_PAD, b[1] - ZONE_PAD, b[2] + ZONE_PAD, b[3] + ZONE_PAD]);
            }
            self.inst.extend(scratch.iter().map(snowy_art::to_instance));
            let fade = match g.stage {
                GStage::Falling => 1.0,
                GStage::Idle => (1.0 - g.t / 0.5).clamp(0.0, 1.0),
                GStage::Opening => 0.0,
            };
            if fade > 0.0 {
                scratch.clear();
                snowy_art::gift_chute(base, h, t, &mut scratch);
                for pr in &mut scratch {
                    pr.color[3] *= fade;
                    if g.stage == GStage::Idle {
                        pr.pos[1] += (1.0 - fade) * 20.0 * sc;
                    }
                }
                self.inst.extend(scratch.iter().map(snowy_art::to_instance));
            }
        }
        if let Some(y) = &self.yeti {
            let yh = YETI_H * sc;
            let jump = if y.stage == YStage::Flee {
                let j = y.t / 0.4;
                if j < 1.0 {
                    (j * PI).sin() * 0.3 * yh
                } else {
                    (y.t * 22.0).sin().abs() * 0.06 * yh
                }
            } else {
                0.0
            };
            let base = [y.x, self.ground - jump];
            if let Some(p) = y.carry.and_then(|id| self.file.props.iter().find(|p| p.id == id)).copied() {
                if let PropKind::Snowman { look, balls } = p.kind {
                    let (b, k) = self.carried_base(y, p.x * self.width);
                    scratch.clear();
                    snowy_art::snowman(&look, balls, b, self.snowman_h(&p) * k, t, &mut scratch);
                    self.inst.extend(scratch.iter().map(snowy_art::to_instance));
                }
            }
            let carrying = y.carry.is_some() && (y.stage != YStage::Lift || y.t > 0.35 * LIFT_SECS);
            let pose = snowy_art::YetiPose { walk_phase: y.phase, carrying, scared: y.stage == YStage::Flee };
            scratch.clear();
            snowy_art::yeti(base, yh, pose, y.dir, t, &mut scratch);
            let b = snowy_art::bounds(&scratch);
            self.yeti_zone = Some([b[0] - ZONE_PAD, b[1] - ZONE_PAD, b[2] + ZONE_PAD, b[3] + ZONE_PAD]);
            self.inst.extend(scratch.iter().map(snowy_art::to_instance));
        }
        if let Some(pl) = &self.plough {
            scratch.clear();
            snowy_art::plough([pl.x, self.ground], PLOUGH_H * sc, pl.dir, pl.t, &mut scratch);
            self.inst.extend(scratch.iter().map(snowy_art::to_instance));
        }
        for c in &self.confetti {
            let a = (c.life / 0.4).clamp(0.0, 1.0);
            let sq = 0.35 + 0.65 * (c.life * 9.0 + c.flutter).sin().abs();
            let (kind, size) = if c.star {
                (snowy_art::PrimKind::Star, [c.size, c.size])
            } else {
                (snowy_art::PrimKind::Ellipse, [c.size, c.size * sq])
            };
            let pr = Prim { kind, pos: c.pos, size, rot: c.rot, color: [c.color[0], c.color[1], c.color[2], a], round: 0.0 };
            self.inst.push(snowy_art::to_instance(&pr));
        }
        self.scratch = scratch;
    }
}

fn transform(prims: &mut [Prim], base: [f32; 2], s: f32, ang: f32) {
    if s == 1.0 && ang == 0.0 {
        return;
    }
    let (sn, cs) = ang.sin_cos();
    for p in prims {
        let dx = (p.pos[0] - base[0]) * s;
        let dy = (p.pos[1] - base[1]) * s;
        p.pos = [base[0] + cs * dx - sn * dy, base[1] + sn * dx + cs * dy];
        p.size = [p.size[0] * s, p.size[1] * s];
        p.rot += ang;
    }
}

impl Effect for Layer {
    fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.rebuild();
    }

    /// Uses `scene.pointer` for hover/drag/click (pointer.down is true while the mouse grab
    /// holds a press inside one of our grab zones). Surfaces are ignored (props live on the
    /// desktop, behind windows).
    fn update(&mut self, dt: f32, _env: &Env, scene: &Scene) {
        let dt = dt.clamp(0.0, 0.1);
        self.time += dt;
        self.since_save += dt;
        let ptr = scene.pointer;
        let down = ptr.is_some_and(|p| p.down);

        // Press / drag / release.
        if down && !self.prev_down {
            // Only presses on the bare desktop: a click into a window in front of a prop
            // belongs to that window.
            if let Some(p) = ptr.filter(|p| p.over_desktop) {
                let inside = |z: Option<[f32; 4]>| z.is_some_and(|z| p.pos[0] >= z[0] && p.pos[0] <= z[2] && p.pos[1] >= z[1] && p.pos[1] <= z[3]);
                if inside(self.yeti_zone) {
                    self.scare_yeti();
                } else if inside(self.gift_zone) {
                    if let Some(g) = &mut self.gift {
                        g.stage = GStage::Opening;
                        g.t = 0.0;
                    }
                } else if let Some((i, part)) = self.hit_part(p.pos) {
                    if self.locked.get(i).copied().unwrap_or(false) {
                        // The press belongs to the desktop (icons stay usable); only explain
                        // when it didn't land on an icon itself.
                        let on_icon = self.icons.iter().any(|z| p.pos[0] >= z[0] && p.pos[0] < z[2] && p.pos[1] >= z[1] && p.pos[1] < z[3]);
                        if !on_icon {
                            self.actions.push(Action::LockedHint { pos: p.pos });
                        }
                    } else {
                        let pr = self.file.props[i];
                        let orig = match pr.kind {
                            PropKind::Lights { a, b, .. } => [a, b],
                            _ => [[0.0; 2]; 2],
                        };
                        let linked = match part {
                            Part::EndA => self.joined_ends(pr.id, orig[0]),
                            Part::EndB => self.joined_ends(pr.id, orig[1]),
                            Part::Move => Vec::new(),
                        };
                        self.press = Some(Press { part, orig, id: pr.id, start: p.pos, t: 0.0, dragging: false, off: pr.x * self.width - p.pos[0], linked });
                    }
                }
            }
        } else if down {
            if let (Some(p), Some(pr)) = (ptr, self.press.as_mut()) {
                pr.t += dt;
                if !pr.dragging && (p.pos[0] - pr.start[0]).hypot(p.pos[1] - pr.start[1]) > DRAG_PX {
                    pr.dragging = true;
                }
                if pr.dragging {
                    let x = ((p.pos[0] + pr.off) / self.width.max(1.0)).clamp(0.02, 0.98);
                    let id = pr.id;
                    let (part, orig) = (pr.part, pr.orig);
                    let d = [(p.pos[0] - pr.start[0]) / self.width.max(1.0), (p.pos[1] - pr.start[1]) / self.height.max(1.0)];
                    let linked = std::mem::take(&mut pr.linked);
                    let is_l = self.file.props.iter().find(|q| q.id == id).is_some_and(is_lights);
                    if is_l {
                        self.drag_lights(id, part, orig, d, &linked);
                        self.dirty = true;
                    } else if let Some(prop) = self.file.props.iter_mut().find(|q| q.id == id) {
                        prop.x = x;
                        self.dirty = true;
                    }
                    if let Some(pr) = self.press.as_mut() {
                        pr.linked = linked;
                    }
                    if is_l {
                        // string lights do not tilt while dragged
                    } else if let Some(tr) = self.trans.get_mut(&id) {
                        let target = (p.vel[0] / 1500.0).clamp(-0.2, 0.2);
                        tr.tilt += (target - tr.tilt) * (dt * 10.0).min(1.0);
                    }
                }
            }
        } else if self.prev_down {
            if let Some(pr) = self.press.take() {
                if pr.dragging {
                    self.save_now();
                } else if pr.t < CLICK_SECS {
                    self.actions.push(Action::OpenEditor { prop_id: pr.id });
                }
            }
        }
        self.prev_down = down;
        if self.dirty && self.since_save >= SAVE_DEBOUNCE {
            self.save_now();
        }

        // Hover target.
        let hover_id = match (&self.press, ptr) {
            (Some(pr), _) if pr.dragging => Some(pr.id),
            (_, Some(p)) if p.over_desktop => self
                .hit_part(p.pos)
                .filter(|&(i, _)| !self.locked.get(i).copied().unwrap_or(false))
                .map(|(i, _)| self.file.props[i].id),
            _ => None,
        };
        let dragging_id = self.press.as_ref().filter(|p| p.dragging).map(|p| p.id);

        for p in &self.file.props {
            let Some(tr) = self.trans.get_mut(&p.id) else { continue };
            if matches!(p.kind, PropKind::Tree { .. }) && tr.grow < 1.0 {
                tr.grow = (tr.grow + dt / GROW_SECS).min(1.0);
            }
            if tr.sway_t >= 0.0 {
                tr.sway_t += dt;
                if tr.sway_t >= tr.sway_dur {
                    tr.sway_t = -1.0;
                }
            } else {
                tr.next_sway -= dt;
                if tr.next_sway <= 0.0 {
                    tr.sway_t = 0.0;
                    if is_lights(p) {
                        tr.sway_dur = 2.5 + fastrand::f32();
                        tr.next_sway = 8.0 + fastrand::f32() * 8.0;
                    } else {
                        tr.sway_dur = 1.0 + fastrand::f32() * 0.5;
                        tr.next_sway = 6.0 + fastrand::f32() * 8.0;
                    }
                }
            }
            // In edit mode every prop shows a faint highlight (lights show their handles).
            let target = if hover_id == Some(p.id) {
                1.0
            } else if self.edit_mode {
                0.45
            } else {
                0.0
            };
            tr.hover += (target - tr.hover) * (dt * 12.0).min(1.0);
            if dragging_id != Some(p.id) {
                tr.tilt -= tr.tilt * (dt * 8.0).min(1.0);
            }
        }

        let dragging = self.press.as_ref().filter(|p| p.dragging).map(|p| p.id);
        self.update_yeti(dt, dragging);
        self.update_drop(dt);
        self.update_gift(dt);
        self.update_confetti(dt);

        // Plough.
        let sc = self.sc();
        let half = self.plough_half();
        let width = self.width;
        if let Some(pl) = &mut self.plough {
            let prev = pl.x + pl.dir * half;
            pl.x += pl.dir * PLOUGH_SPEED * sc * dt;
            pl.t += dt;
            let front = pl.x + pl.dir * half;
            let x0 = prev.min(front).max(0.0);
            let x1 = prev.max(front).min(width);
            if x1 > x0 {
                self.actions.push(Action::ClearGround { x0, x1 });
            }
            let done = if pl.dir > 0.0 { pl.x - half > width } else { pl.x + half < 0.0 };
            if done {
                self.plough = None;
            }
        }

        self.rebuild();
    }

    fn instances(&self) -> &[Instance] {
        &self.inst
    }

    fn shader(&self) -> &'static str {
        snowy_art::SHADER
    }

    /// Bounding boxes of all props (the plough is deliberately not a grab zone).
    fn grab_zones(&self, out: &mut Vec<[f32; 4]>) {
        let free = |i: usize| !self.locked.get(i).copied().unwrap_or(false);
        out.extend(self.zones.iter().enumerate().filter(|(i, z)| z[2] > z[0] && free(*i)).map(|(_, z)| *z));
        out.extend(self.lzones.iter().filter(|(i, _, _)| free(*i)).map(|(_, _, z)| *z));
        out.extend(self.yeti_zone);
        out.extend(self.gift_zone);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snowy_core::Pointer;
    use std::time::Duration;

    fn mon() -> MonitorInfo {
        MonitorInfo { id: "TEST".into(), rect: (0, 0, 1920, 1080), work_rect: (0, 0, 1920, 1040), primary: true, dpi_scale: 1.0 }
    }

    fn layer(name: &str) -> (Layer, PathBuf) {
        let d = std::env::temp_dir().join(format!("snowy-props-test-{}-{name}", fastrand::u64(..)));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("props.json");
        let mut l = Layer::with_store(&mon(), 1920.0, 1080.0, 1040.0, p.clone());
        l.day_override = Some(0);
        (l, p)
    }

    fn run(l: &mut Layer, secs: f32) {
        for _ in 0..(secs / 0.05) as usize {
            idle(l, 0.05);
        }
    }

    fn click(l: &mut Layer, c: [f32; 2]) {
        step(l, 0.016, c, false);
        step(l, 0.016, c, true);
        step(l, 0.016, c, false);
    }

    fn zone_center(z: [f32; 4]) -> [f32; 2] {
        [(z[0] + z[2]) / 2.0, (z[1] + z[3]) / 2.0]
    }

    fn finite(l: &Layer) -> bool {
        l.instances().iter().all(|i| i.pos.iter().chain(&i.color).chain(&i.param).all(|v| v.is_finite()) && i.size.is_finite() && i.rot.is_finite())
    }

    #[test]
    fn civil_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
        assert_eq!(days_from_civil(2026, 11, 18), 20775);
    }

    #[test]
    fn yeti_without_snowmen_does_nothing() {
        let (mut l, _) = layer("yeti-none");
        l.add_tree();
        l.call_yeti();
        assert!(l.yeti.is_none());
        run(&mut l, 5.0);
        assert!(l.yeti.is_none());
    }

    #[test]
    fn yeti_takes_the_snowman() {
        let (mut l, _) = layer("yeti-take");
        l.add_snowman();
        l.add_tree();
        l.call_yeti();
        assert!(l.yeti.is_some());
        let mut z = Vec::new();
idle(&mut l, 0.05);
        l.grab_zones(&mut z);
        assert_eq!(z.len(), 3, "snowman, tree and yeti");
        for _ in 0..4000 {
            idle(&mut l, 0.05);
            assert!(finite(&l));
            if l.yeti.is_none() {
                break;
            }
        }
        assert!(l.yeti.is_none());
        assert!(!l.file.props.iter().any(is_snowman));
        assert_eq!(l.file.props.len(), 1);
        assert_eq!(l.store.load().props.len(), 1);
    }

    #[test]
    fn clicking_the_yeti_saves_the_snowman() {
        let (mut l, _) = layer("yeti-scare");
        l.add_snowman();
        let x0 = l.file.props[0].x;
        let id = l.file.props[0].id;
        l.call_yeti();
        for _ in 0..4000 {
            idle(&mut l, 0.05);
            if l.yeti.as_ref().is_some_and(|y| y.carry.is_some() && y.stage == YStage::Leave) {
                break;
            }
        }
        assert!(l.yeti.as_ref().is_some_and(|y| y.carry.is_some()));
        idle(&mut l, 0.05);
        let c = zone_center(l.yeti_zone.unwrap());
        click(&mut l, c);
        assert!(l.yeti.as_ref().is_some_and(|y| y.stage == YStage::Flee));
        for _ in 0..400 {
            idle(&mut l, 0.05);
            assert!(finite(&l));
        }
        assert!(l.yeti.is_none());
        assert!(l.drop.is_none());
        assert_eq!(l.file.props.len(), 1);
        assert_eq!(l.file.props[0].id, id);
        assert_eq!(l.file.props[0].x, x0);
        assert_eq!(l.store.load().props[0].x, x0);
        assert!(l.zones[0][2] > l.zones[0][0]);
    }

    #[test]
    fn scaring_while_walking_in() {
        let (mut l, _) = layer("yeti-early");
        l.add_snowman();
        l.call_yeti();
        run(&mut l, 1.0);
        let c = zone_center(l.yeti_zone.unwrap());
        click(&mut l, c);
        run(&mut l, 20.0);
        assert!(l.yeti.is_none());
        assert_eq!(l.file.props.len(), 1);
    }

    #[test]
    fn gift_once_per_day_on_primary_only() {
        let (mut l, p) = layer("gift");
        l.day_override = Some(500);
        run(&mut l, 9.0);
        assert!(l.gift.is_none());
        run(&mut l, 2.0);
        assert!(l.gift.is_some());
        assert_eq!(l.store.load().last_gift_day, 500);
        // a restart on the same day does not give another one
        let mut l2 = Layer::with_store(&mon(), 1920.0, 1080.0, 1040.0, p.clone());
        l2.day_override = Some(500);
        run(&mut l2, 30.0);
        assert!(l2.gift.is_none());
        // next day, while running: after the first gift is gone
        run(&mut l, 30.0);
        assert!(l.gift.as_ref().is_some_and(|g| g.stage == GStage::Idle));
        assert!(finite(&l));
        let mut z = Vec::new();
        l.grab_zones(&mut z);
        assert_eq!(z.len(), 1);
        let c = zone_center(z[0]);
        click(&mut l, c);
        run(&mut l, 0.5);
        assert_eq!(l.file.props.len(), 0);
        run(&mut l, 0.4);
        assert!(l.gift.is_none());
        assert_eq!(l.file.props.len(), 1);
        assert!(matches!(l.file.props[0].kind, PropKind::Snowman { balls: 2..=3, .. }));
        assert!(!l.confetti.is_empty());
        assert!(finite(&l));
        run(&mut l, 5.0);
        assert!(l.confetti.is_empty());
        assert_eq!(l.store.load().props.len(), 1);
        l.day_override = Some(501);
        run(&mut l, 2.0);
        assert!(l.gift.is_some());
    }

    #[test]
    fn no_gift_on_secondary() {
        let d = std::env::temp_dir().join(format!("snowy-props-test-{}-sec", fastrand::u64(..)));
        std::fs::create_dir_all(&d).unwrap();
        let m = MonitorInfo { primary: false, ..mon() };
        let mut l = Layer::with_store(&m, 1920.0, 1080.0, 1040.0, d.join("props.json"));
        l.day_override = Some(700);
        run(&mut l, 40.0);
        assert!(l.gift.is_none());
        assert_eq!(l.file.last_gift_day, 0);
    }

    const ENV: Env = Env { wind: 0.0, intensity: 1.0, speed: 1.0, time: 0.0 };

    fn step(l: &mut Layer, dt: f32, pos: [f32; 2], down: bool) {
        let s = Scene {
            surfaces: &[],
            ground: None,
            pointer: Some(Pointer { pos, vel: [0.0; 2], down, over_desktop: true }),
            snowballs: false,
        };
        l.update(dt, &ENV, &s);
    }

    fn idle(l: &mut Layer, dt: f32) {
        let s = Scene { surfaces: &[], ground: None, pointer: None, snowballs: false };
        l.update(dt, &ENV, &s);
    }

    fn center(l: &Layer, i: usize) -> [f32; 2] {
        let z = l.zones[i];
        [(z[0] + z[2]) / 2.0, (z[1] + z[3]) / 2.0]
    }

    #[test]
    fn add_persists_and_avoids_overlap() {
        let (mut l, p) = layer("add");
        l.add_snowman();
        l.add_tree();
        l.add_snowman();
        assert_eq!(l.file.props.len(), 3);
        assert_eq!(l.store.load(), l.file);
        assert!(p.exists());
        for (i, a) in l.file.props.iter().enumerate() {
            assert!((0.05..=0.95).contains(&a.x));
            for b in &l.file.props[i + 1..] {
                let min = (l.prop_width(a) + l.prop_width(b)) * 0.5;
                assert!((a.x - b.x).abs() * 1920.0 >= min - 1.0, "overlap {a:?} {b:?}");
            }
        }
        assert!(matches!(l.file.props[1].kind, PropKind::Tree { look } if look.lights == 1 && look.ornaments == 3 && look.topper == 1));
    }

    #[test]
    fn thirteenth_replaces_oldest() {
        let (mut l, _) = layer("cap");
        for _ in 0..12 {
            l.add_snowman();
        }
        let first = l.file.props[0].id;
        l.add_snowman();
        assert_eq!(l.file.props.len(), 12);
        assert!(l.file.props.iter().all(|p| p.id != first));
        l.add_tree();
        assert_eq!(l.file.props.len(), 12);
    }

    #[test]
    fn click_opens_editor() {
        let (mut l, _) = layer("click");
        l.add_snowman();
        let id = l.file.props[0].id;
        let c = center(&l, 0);
        step(&mut l, 0.016, c, false);
        step(&mut l, 0.016, c, true);
        step(&mut l, 0.1, c, true);
        step(&mut l, 0.016, c, false);
        let mut out = Vec::new();
        l.take_actions(&mut out);
        assert_eq!(out, vec![Action::OpenEditor { prop_id: id }]);
    }

    #[test]
    fn drag_moves_and_saves() {
        let (mut l, _) = layer("drag");
        l.add_snowman();
        let x0 = l.file.props[0].x;
        let c = center(&l, 0);
        step(&mut l, 0.016, c, false);
        step(&mut l, 0.016, c, true);
        for i in 1..=10 {
            step(&mut l, 0.016, [c[0] + 20.0 * i as f32, c[1]], true);
        }
        step(&mut l, 0.016, [c[0] + 200.0, c[1]], false);
        let mut out = Vec::new();
        l.take_actions(&mut out);
        assert!(out.is_empty());
        let x1 = l.file.props[0].x;
        assert!((x1 - x0 - 200.0 / 1920.0).abs() < 0.002, "{x0} -> {x1}");
        assert_eq!(l.store.load().props[0].x, x1);
    }

    #[test]
    fn down_outside_does_nothing() {
        let (mut l, _) = layer("outside");
        l.add_snowman();
        let before = l.file.clone();
        step(&mut l, 0.016, [10.0, 10.0], false);
        step(&mut l, 0.016, [10.0, 10.0], true);
        step(&mut l, 0.016, [10.0, 10.0], false);
        let mut out = Vec::new();
        l.take_actions(&mut out);
        assert!(out.is_empty());
        assert_eq!(l.file, before);
    }

    #[test]
    fn grab_zones_cover_props() {
        let (mut l, _) = layer("zones");
        l.add_snowman();
        l.add_tree();
        let mut z = Vec::new();
        l.grab_zones(&mut z);
        assert_eq!(z.len(), 2);
        for (i, p) in l.file.props.iter().enumerate() {
            let x = p.x * 1920.0;
            assert!(z[i][0] < x && x < z[i][2] && z[i][3] >= 1040.0);
        }
    }

    #[test]
    fn snowman_built_clamps_scale() {
        let (mut l, _) = layer("built");
        l.handle(WorldEvent::SnowmanBuilt { x: 960.0, balls: 2, height: 1000.0 });
        l.handle(WorldEvent::SnowmanBuilt { x: 480.0, balls: 3, height: 10.0 });
        assert_eq!(l.file.props[0].scale, 1.6);
        assert_eq!(l.file.props[1].scale, 0.6);
        assert!((l.file.props[1].x - 0.25).abs() < 1e-5);
        assert!(matches!(l.file.props[0].kind, PropKind::Snowman { balls: 2, .. }));
        assert_eq!(l.store.load().props.len(), 2);
    }

    #[test]
    fn plough_clears_whole_width_and_finishes() {
        let (mut l, _) = layer("plough");
        assert!(l.is_empty());
        for run in 0..2 {
            l.send_plough();
            assert!(!l.is_empty());
            let mut n = 0;
            while !l.is_empty() && n < 5000 {
                idle(&mut l, 0.016);
                assert!(l.inst.iter().all(|i| i.pos[0].is_finite()));
                n += 1;
            }
            assert!(l.is_empty(), "run {run} did not finish");
            let mut acts = Vec::new();
            l.take_actions(&mut acts);
            let mut ranges: Vec<(f32, f32)> = acts
                .iter()
                .map(|a| match a {
                    Action::ClearGround { x0, x1 } => (*x0, *x1),
                    other => panic!("unexpected {other:?}"),
                })
                .collect();
            ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            assert!(ranges[0].0 <= 0.5);
            let mut reach = ranges[0].1;
            for r in &ranges[1..] {
                assert!(r.0 <= reach + 0.01, "gap");
                reach = reach.max(r.1);
            }
            assert!(reach >= 1919.5);
        }
    }

    #[test]
    fn plough_is_not_grab_zone() {
        let (mut l, _) = layer("ploughzone");
        l.send_plough();
        idle(&mut l, 0.016);
        let mut z = Vec::new();
        l.grab_zones(&mut z);
        assert!(z.is_empty());
        assert!(!l.instances().is_empty());
    }

    #[test]
    fn reload_picks_up_external_changes() {
        let (mut l, p) = layer("reload");
        l.add_snowman();
        l.add_tree();
        l.poll_reload();
        assert_eq!(l.file.props.len(), 2);
        let keep = l.file.props[0].id;
        let mut f = l.file.clone();
        f.props.truncate(1);
        if let PropKind::Snowman { look, .. } = &mut f.props[0].kind {
            look.hat = 3;
        }
        std::fs::write(&p, serde_json::to_string(&f).unwrap()).unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
        drop(file);
        l.poll_reload();
        assert_eq!(l.file.props.len(), 1);
        assert_eq!(l.file.props[0].id, keep);
        assert!(matches!(l.file.props[0].kind, PropKind::Snowman { look, .. } if look.hat == 3));
        assert_eq!(l.trans.len(), 1);
    }

    #[test]
    fn instances_finite_and_empty() {
        let (mut l, _) = layer("inst");
        idle(&mut l, 0.016);
        assert!(l.instances().is_empty());
        l.add_snowman();
        l.add_tree();
        for _ in 0..200 {
            idle(&mut l, 0.05);
        }
        assert!(!l.instances().is_empty());
        for i in l.instances() {
            assert!(i.pos.iter().chain(&i.color).chain(&i.param).all(|v| v.is_finite()));
            assert!(i.size.is_finite() && i.rot.is_finite());
        }
    }

    fn lights_ends(l: &Layer) -> ([f32; 2], [f32; 2]) {
        match l.file.props.last().unwrap().kind {
            PropKind::Lights { a, b, .. } => (a, b),
            _ => panic!("not lights"),
        }
    }

    fn drag(l: &mut Layer, from: [f32; 2], by: [f32; 2]) {
        step(l, 0.016, from, false);
        step(l, 0.016, from, true);
        for i in 1..=10 {
            let f = i as f32 / 10.0;
            step(l, 0.016, [from[0] + by[0] * f, from[1] + by[1] * f], true);
        }
        step(l, 0.016, [from[0] + by[0], from[1] + by[1]], false);
    }

    #[test]
    fn add_lights_persists_and_reloads() {
        let (mut l, p) = layer("lights-add");
        l.add_lights();
        assert_eq!(l.file.props.len(), 1);
        let (a, b) = lights_ends(&l);
        assert_eq!((a, b), ([0.3, 0.25], [0.7, 0.25]));
        assert_eq!(l.store.load(), l.file);
        idle(&mut l, 0.05);
        assert!(!l.instances().is_empty());
        assert!(finite(&l));
        let l2 = Layer::with_store(&mon(), 1920.0, 1080.0, 1040.0, p);
        assert_eq!(l2.file, l.file);
        assert!(!l2.instances().is_empty());
        // counts toward the cap, replacing the oldest string
        for _ in 0..14 {
            l.add_lights();
        }
        assert_eq!(l.file.props.len(), 12);
    }

    #[test]
    fn dragging_an_end_moves_only_that_end() {
        let (mut l, _) = layer("lights-end");
        l.add_lights();
        let (a0, b0) = lights_ends(&l);
        drag(&mut l, [a0[0] * 1920.0, a0[1] * 1080.0], [-100.0, 200.0]);
        let (a1, b1) = lights_ends(&l);
        assert_eq!(b1, b0);
        assert!((a1[0] - (a0[0] - 100.0 / 1920.0)).abs() < 0.002 && (a1[1] - (a0[1] + 200.0 / 1080.0)).abs() < 0.002, "{a1:?}");
        let (_, b2) = {
            let st = l.store.load();
            match st.props[0].kind {
                PropKind::Lights { a, b, .. } => (a, b),
                _ => panic!(),
            }
        };
        assert_eq!(b2, b0);
        // the other end
        drag(&mut l, [b0[0] * 1920.0, b0[1] * 1080.0], [50.0, 0.0]);
        let (a2, b3) = lights_ends(&l);
        assert_eq!(a2, a1);
        assert!(b3[0] > b0[0]);
    }

    #[test]
    fn dragging_the_middle_moves_both() {
        let (mut l, _) = layer("lights-mid");
        l.add_lights();
        let (a0, b0) = lights_ends(&l);
        let (ap, bp, sag) = l.lights_px(a0, b0);
        let mid = Layer::wire_at(ap, bp, sag, 0.5);
        drag(&mut l, mid, [120.0, 90.0]);
        let (a1, b1) = lights_ends(&l);
        for k in 0..2 {
            let (dx, dy) = (120.0 / 1920.0, 90.0 / 1080.0);
            assert!((a1[k] - a0[k] - if k == 0 { dx } else { dy }).abs() < 0.002);
            assert!((b1[k] - b0[k] - if k == 0 { dx } else { dy }).abs() < 0.002);
        }
        assert_eq!(l.store.load().props[0].kind, l.file.props[0].kind);
        // cannot be dragged off screen
        let (ap, bp, sag) = l.lights_px(a1, b1);
        let mid = Layer::wire_at(ap, bp, sag, 0.5);
        drag(&mut l, mid, [5000.0, 0.0]);
        let (a2, b2) = lights_ends(&l);
        assert!(a2.iter().chain(&b2).all(|v| (0.0..=1.0).contains(v)));
        assert!((b2[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn clicking_lights_opens_editor() {
        let (mut l, _) = layer("lights-click");
        l.add_lights();
        let id = l.file.props[0].id;
        let (a, b) = lights_ends(&l);
        let (ap, bp, sag) = l.lights_px(a, b);
        for c in [Layer::wire_at(ap, bp, sag, 0.5), ap, bp] {
            click(&mut l, c);
            let mut out = Vec::new();
            l.take_actions(&mut out);
            assert_eq!(out, vec![Action::OpenEditor { prop_id: id }]);
        }
        assert_eq!(lights_ends(&l), (a, b));
    }

    #[test]
    fn lights_zones_leave_the_empty_area_free() {
        let (mut l, _) = layer("lights-zones");
        l.add_lights();
        let (_, b) = lights_ends(&l);
        // make it a steep diagonal: the box between the ends is mostly empty
        if let PropKind::Lights { a, b: bb, .. } = &mut l.file.props[0].kind {
            *a = [0.1, 0.1];
            *bb = [0.9, 0.8];
        }
        let _ = b;
        l.rebuild();
        let mut z = Vec::new();
        l.grab_zones(&mut z);
        assert!(z.len() >= 4);
        let inside = |p: [f32; 2]| z.iter().any(|z| p[0] >= z[0] && p[0] <= z[2] && p[1] >= z[1] && p[1] <= z[3]);
        // top-right and bottom-left corners of the ends' bounding rectangle
        assert!(!inside([0.85 * 1920.0, 0.15 * 1080.0]));
        assert!(!inside([0.15 * 1920.0, 0.75 * 1080.0]));
        assert!(inside([0.1 * 1920.0, 0.1 * 1080.0]));
        assert!(inside([0.9 * 1920.0, 0.8 * 1080.0]));
        // nothing is grabbed far away, and (0,0) is not a phantom zone
        assert!(!inside([2.0, 2.0]));
        step(&mut l, 0.016, [2.0, 2.0], false);
        step(&mut l, 0.016, [2.0, 2.0], true);
        step(&mut l, 0.016, [2.0, 2.0], false);
        let mut out = Vec::new();
        l.take_actions(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn lights_do_not_block_snowman_spot() {
        let (mut l, _) = layer("lights-spot");
        l.add_lights();
        l.add_snowman();
        assert!((l.file.props[1].x - 0.5).abs() < 0.01);
        assert!(finite(&l));
    }

    #[test]
    fn press_into_a_window_in_front_is_ignored() {
        let (mut l, _) = layer("window-front");
        l.add_snowman();
        let c = center(&l, 0);
        let at = |l: &mut Layer, down| {
            let s = Scene {
                surfaces: &[],
                ground: None,
                pointer: Some(Pointer { pos: c, vel: [0.0; 2], down, over_desktop: false }),
                snowballs: false,
            };
            l.update(0.016, &ENV, &s);
        };
        at(&mut l, false);
        at(&mut l, true);
        at(&mut l, false);
        let mut out = Vec::new();
        l.take_actions(&mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn props_next_to_icons_lock_until_edit_mode() {
        let (mut l, _) = layer("icons");
        l.add_snowman();
        let z = l.zones[0];
        // An icon just left of the snowman.
        l.set_desktop_icons(&[[z[0] - 80.0, z[1], z[0] - 2.0, z[1] + 70.0]]);
        assert!(l.has_locked());
        let mut zones = Vec::new();
        l.grab_zones(&mut zones);
        assert!(zones.is_empty(), "locked props must not swallow desktop clicks");
        let c = center(&l, 0);
        click(&mut l, c);
        let mut out = Vec::new();
        l.take_actions(&mut out);
        assert_eq!(out, vec![Action::LockedHint { pos: c }]);

        // Pressing on the icon itself gives no hint (the icon gets the click).
        l.set_desktop_icons(&[[c[0] - 5.0, c[1] - 5.0, c[0] + 5.0, c[1] + 5.0]]);
        click(&mut l, c);
        l.take_actions(&mut out);
        assert_eq!(out.len(), 1);

        l.set_edit_mode(true);
        assert!(!l.has_locked());
        let x0 = l.file.props[0].x;
        drag_x(&mut l, c, 150.0);
        assert!(l.file.props[0].x > x0);
        l.set_edit_mode(false);
        l.set_desktop_icons(&[]);
        assert!(!l.has_locked());
    }

    fn drag_x(l: &mut Layer, from: [f32; 2], dx: f32) {
        drag(l, from, [dx, 0.0]);
    }

    fn set_lights(l: &mut Layer, idx: usize, na: [f32; 2], nb: [f32; 2]) {
        if let PropKind::Lights { a, b, .. } = &mut l.file.props[idx].kind {
            *a = na;
            *b = nb;
        }
        l.rebuild();
    }

    fn ends(l: &Layer, idx: usize) -> ([f32; 2], [f32; 2]) {
        match l.file.props[idx].kind {
            PropKind::Lights { a, b, .. } => (a, b),
            _ => panic!("not lights"),
        }
    }

    #[test]
    fn light_ends_snap_together_and_move_as_a_joint() {
        let (mut l, _) = layer("lights-snap");
        l.add_lights();
        l.add_lights();
        set_lights(&mut l, 0, [0.1, 0.2], [0.4, 0.2]);
        set_lights(&mut l, 1, [0.6, 0.5], [0.9, 0.5]);
        // Drag string 1's end A to within a few px of string 0's end B: it snaps exactly.
        let (b0, a1) = (ends(&l, 0).1, ends(&l, 1).0);
        let from = [a1[0] * 1920.0, a1[1] * 1080.0];
        let target = [b0[0] * 1920.0 + 9.0, b0[1] * 1080.0 - 7.0];
        drag(&mut l, from, [target[0] - from[0], target[1] - from[1]]);
        assert_eq!(ends(&l, 1).0, ends(&l, 0).1);

        // Dragging the joint moves both strings' ends.
        let j = ends(&l, 0).1;
        let jp = [j[0] * 1920.0, j[1] * 1080.0];
        drag(&mut l, jp, [0.0, 120.0]);
        let (b0, a1) = (ends(&l, 0).1, ends(&l, 1).0);
        assert_eq!(b0, a1);
        assert!((b0[1] - (j[1] + 120.0 / 1080.0)).abs() < 0.002, "{b0:?}");
        assert_eq!(ends(&l, 0).0, [0.1, 0.2]);
        assert_eq!(ends(&l, 1).1, [0.9, 0.5]);

        // Far ends don't snap.
        let a0 = ends(&l, 0).0;
        drag(&mut l, [a0[0] * 1920.0, a0[1] * 1080.0], [0.0, 300.0]);
        assert_ne!(ends(&l, 0).0, ends(&l, 1).1);
    }

    #[test]
    fn moving_a_whole_string_snaps_its_end_to_a_neighbour() {
        let (mut l, _) = layer("lights-move-snap");
        l.add_lights();
        l.add_lights();
        set_lights(&mut l, 0, [0.1, 0.2], [0.3, 0.2]);
        set_lights(&mut l, 1, [0.5, 0.6], [0.7, 0.6]);
        let (a1, b1) = ends(&l, 1);
        let (ap, bp, sag) = l.lights_px(a1, b1);
        let mid = Layer::wire_at(ap, bp, sag, 0.5);
        // Bring string 1's end A close to string 0's end B (off by ~10 px).
        let b0 = ends(&l, 0).1;
        let by = [(b0[0] - a1[0]) * 1920.0 + 10.0, (b0[1] - a1[1]) * 1080.0 + 6.0];
        drag(&mut l, mid, by);
        let (na, nb) = ends(&l, 1);
        assert!((na[0] - b0[0]).abs() < 1e-5 && (na[1] - b0[1]).abs() < 1e-5, "{na:?} vs {b0:?}");
        // Shape kept.
        assert!(((nb[0] - na[0]) - (b1[0] - a1[0])).abs() < 1e-5);
    }
}
