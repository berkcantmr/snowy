//! Shared desktop: two friends see the same decorations on their primary screens.
//!
//! Sharing works on the primary monitor's props file. When it starts, that file is backed up
//! (`props-solo-<monitor>.json`) and both sides send all their decorations, so each ends up
//! with both sets. After that every change to the file (drag, editor, add, remove) is sent
//! as it happens, and friends' changes are written into the file, which the props layer
//! reloads. When sharing ends, each side gets its own decorations back.

use snowy_core::props::{Prop, PropsFile};
use snowy_net::state::{self as net, Command, SceneMsg, ShareStatus, SharedState};
use std::path::{Path, PathBuf};

/// Most decorations in a shared desktop.
const MAX_SHARED: usize = 40;

/// Something worth telling the user about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Asked(String),
    Started(String),
    Stopped(String),
}

pub struct SharedDesktop {
    dir: PathBuf,
    state: SharedState,
    /// The props as last sent/received, to find what changed.
    last: Vec<Prop>,
    friend_online: bool,
}

/// Messages to send: (friend code, message).
pub type Out = Vec<(String, SceneMsg)>;

impl SharedDesktop {
    pub fn load(dir: &Path) -> SharedDesktop {
        SharedDesktop { dir: dir.to_path_buf(), state: net::load_shared(dir), last: Vec::new(), friend_online: false }
    }

    pub fn state(&self) -> &SharedState {
        &self.state
    }

    fn save(&self) {
        if let Err(e) = net::save_shared(&self.dir, &self.state) {
            log::warn!("shared desktop: writing state: {e}");
        }
    }

    fn set(&mut self, with: Option<String>, status: ShareStatus) {
        self.state = SharedState { with, status };
        self.save();
    }

    /// The monitor's props file in this data dir (same name as `PropsFile::path`).
    fn props_path(&self, monitor: &str) -> PathBuf {
        let clean: String = monitor.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        self.dir.join(format!("props-{clean}.json"))
    }

    fn load_props(&self, monitor: &str) -> PropsFile {
        let mut f: PropsFile = std::fs::read(self.props_path(monitor)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        for p in &mut f.props {
            snowy_core::props::sanitize_prop(p);
        }
        f
    }

    fn save_props(&self, monitor: &str, f: &PropsFile) -> std::io::Result<()> {
        let path = self.props_path(monitor);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(f).unwrap_or_default())?;
        std::fs::rename(tmp, path)
    }

    fn backup_path(&self, monitor: &str) -> PathBuf {
        let clean: String = monitor.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        self.dir.join(format!("props-solo-{clean}.json"))
    }

    fn with_is(&self, code: &str) -> bool {
        self.state.with.as_deref() == Some(code)
    }

    /// Commands from the settings window.
    pub fn command(&mut self, cmd: &Command, monitor: &str, out: &mut Out) -> Option<Notice> {
        match cmd {
            Command::ShareInvite { code } => {
                if self.state.status == ShareStatus::Active {
                    return None;
                }
                if self.state.status == ShareStatus::Asked && self.with_is(code) {
                    // They asked first: inviting back accepts.
                    return self.command(&Command::ShareAccept { code: code.clone() }, monitor, out);
                }
                self.set(Some(code.clone()), ShareStatus::Invited);
                out.push((code.clone(), SceneMsg::Invite));
                None
            }
            Command::ShareAccept { code } => {
                if self.state.status != ShareStatus::Asked || !self.with_is(code) {
                    return None;
                }
                out.push((code.clone(), SceneMsg::Accept));
                self.start(code, monitor, out);
                Some(Notice::Started(code.clone()))
            }
            Command::ShareDecline { code } => {
                if self.with_is(code) && matches!(self.state.status, ShareStatus::Asked | ShareStatus::Invited) {
                    out.push((code.clone(), SceneMsg::Decline));
                    self.set(None, ShareStatus::None);
                }
                None
            }
            Command::ShareStop => {
                let code = self.state.with.clone()?;
                out.push((code.clone(), if self.state.status == ShareStatus::Active { SceneMsg::Stop } else { SceneMsg::Decline }));
                self.end(monitor);
                None
            }
            _ => None,
        }
    }

    /// A message from a friend.
    pub fn message(&mut self, from: &str, msg: SceneMsg, monitor: &str, out: &mut Out) -> Option<Notice> {
        let from_partner = self.with_is(from);
        match msg {
            SceneMsg::Invite => match self.state.status {
                ShareStatus::None => {
                    self.set(Some(from.to_string()), ShareStatus::Asked);
                    Some(Notice::Asked(from.to_string()))
                }
                // Both invited each other at once: that's a yes from both.
                ShareStatus::Invited if from_partner => {
                    out.push((from.to_string(), SceneMsg::Accept));
                    self.start(from, monitor, out);
                    Some(Notice::Started(from.to_string()))
                }
                ShareStatus::Active if from_partner => {
                    // They lost track (reinstalled?): send everything again.
                    self.resend(from, monitor, out);
                    None
                }
                _ => {
                    out.push((from.to_string(), SceneMsg::Decline));
                    None
                }
            },
            SceneMsg::Accept if from_partner && self.state.status == ShareStatus::Invited => {
                self.start(from, monitor, out);
                Some(Notice::Started(from.to_string()))
            }
            SceneMsg::Decline if from_partner && self.state.status != ShareStatus::Active => {
                self.set(None, ShareStatus::None);
                None
            }
            SceneMsg::Stop if from_partner => {
                let was_active = self.state.status == ShareStatus::Active;
                self.end(monitor);
                was_active.then(|| Notice::Stopped(from.to_string()))
            }
            SceneMsg::Upsert { prop } if from_partner && self.state.status == ShareStatus::Active => {
                let mut f = self.load_props(monitor);
                match f.props.iter().position(|p| p.id == prop.id) {
                    Some(i) => f.props[i] = prop,
                    None if f.props.len() < MAX_SHARED => f.props.push(prop),
                    None => return None,
                }
                self.write(monitor, f);
                None
            }
            SceneMsg::Remove { id } if from_partner && self.state.status == ShareStatus::Active => {
                let mut f = self.load_props(monitor);
                f.props.retain(|p| p.id != id);
                self.write(monitor, f);
                None
            }
            _ => None,
        }
    }

    fn write(&mut self, monitor: &str, f: PropsFile) {
        if let Err(e) = self.save_props(monitor, &f) {
            log::warn!("shared desktop: writing props: {e}");
        }
        self.last = f.props;
    }

    /// Sharing begins: back up this person's decorations and send them all.
    fn start(&mut self, with: &str, monitor: &str, out: &mut Out) {
        let backup = self.backup_path(monitor);
        if !backup.exists() {
            let own = self.load_props(monitor);
            if let Err(e) = std::fs::write(&backup, serde_json::to_vec_pretty(&own).unwrap_or_default()) {
                log::warn!("shared desktop: backup: {e}");
            }
        }
        self.set(Some(with.to_string()), ShareStatus::Active);
        self.friend_online = true;
        self.resend(with, monitor, out);
        log::info!("shared desktop with {with}: on");
    }

    fn resend(&mut self, with: &str, monitor: &str, out: &mut Out) {
        let f = self.load_props(monitor);
        for p in f.props.iter().take(MAX_SHARED) {
            out.push((with.to_string(), SceneMsg::Upsert { prop: *p }));
        }
        self.last = f.props;
    }

    /// Sharing ends: put this person's own decorations back.
    fn end(&mut self, monitor: &str) {
        if self.state.status == ShareStatus::Active {
            let backup = self.backup_path(monitor);
            let own: Option<PropsFile> = std::fs::read(&backup).ok().and_then(|b| serde_json::from_slice(&b).ok());
            if let Some(mut own) = own {
                // Keep today's gift bookkeeping.
                own.last_gift_day = own.last_gift_day.max(self.load_props(monitor).last_gift_day);
                if self.save_props(monitor, &own).is_ok() {
                    let _ = std::fs::remove_file(&backup);
                }
            }
            log::info!("shared desktop: off");
        }
        self.last.clear();
        self.set(None, ShareStatus::None);
    }

    /// Every few hundred ms: send this side's changes; resend everything when the friend
    /// comes back online.
    pub fn tick(&mut self, monitor: &str, friend_online: impl Fn(&str) -> bool, out: &mut Out) {
        if self.state.status != ShareStatus::Active {
            return;
        }
        let Some(with) = self.state.with.clone() else { return };
        let online = friend_online(&with);
        let came_back = online && !self.friend_online;
        self.friend_online = online;
        if !online {
            return;
        }
        if came_back {
            self.resend(&with, monitor, out);
            return;
        }
        let cur = self.load_props(monitor).props;
        if cur == self.last {
            return;
        }
        for p in &cur {
            if !self.last.contains(p) {
                out.push((with.clone(), SceneMsg::Upsert { prop: *p }));
            }
        }
        for p in &self.last {
            if !cur.iter().any(|q| q.id == p.id) {
                out.push((with.clone(), SceneMsg::Remove { id: p.id }));
            }
        }
        self.last = cur;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snowy_core::props::{PropKind, SnowmanLook};

    fn snowman(id: u64, x: f32) -> Prop {
        Prop { id, kind: PropKind::Snowman { look: SnowmanLook::default(), balls: 3 }, x, scale: 1.0 }
    }

    /// Two people, each with its own data dir, talking through `out`.
    struct Side {
        sd: SharedDesktop,
        monitor: String,
        dir: PathBuf,
    }

    impl Side {
        fn new(name: &str) -> Side {
            let dir = std::env::temp_dir().join(format!("snowy-shared-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Side { sd: SharedDesktop::load(&dir), monitor: "DISPLAY1".into(), dir }
        }
        fn props(&self) -> Vec<Prop> {
            self.sd.load_props(&self.monitor).props
        }
        fn set_props(&self, props: Vec<Prop>) {
            self.sd.save_props(&self.monitor, &PropsFile { props, last_gift_day: 0 }).unwrap();
        }
    }

    impl Drop for Side {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn deliver(out: Out, to: &mut Side, from: &str) -> Out {
        let mut back = Vec::new();
        for (_, m) in out {
            to.sd.message(from, m, &to.monitor.clone(), &mut back);
        }
        back
    }

    #[test]
    fn invite_merge_sync_and_restore() {
        let (mut a, mut b) = (Side::new("a"), Side::new("b"));
        a.set_props(vec![snowman(1, 0.2)]);
        b.set_props(vec![snowman(2, 0.8)]);
        // A invites, B accepts.
        let mut out = Vec::new();
        a.sd.command(&Command::ShareInvite { code: "B".into() }, &a.monitor.clone(), &mut out);
        assert_eq!(a.sd.state().status, ShareStatus::Invited);
        let back = deliver(out, &mut b, "A");
        assert!(back.is_empty());
        assert_eq!(b.sd.state().status, ShareStatus::Asked);
        let mut out = Vec::new();
        let n = b.sd.command(&Command::ShareAccept { code: "A".into() }, &b.monitor.clone(), &mut out);
        assert_eq!(n, Some(Notice::Started("A".into())));
        let back = deliver(out, &mut a, "B");
        deliver(back, &mut b, "A");
        // Both now have both snowmen.
        let ids = |s: &Side| {
            let mut v: Vec<u64> = s.props().iter().map(|p| p.id).collect();
            v.sort();
            v
        };
        assert_eq!(ids(&a), [1, 2]);
        assert_eq!(ids(&b), [1, 2]);
        // A moves B's snowman; B gets it.
        a.set_props(vec![snowman(1, 0.2), snowman(2, 0.5)]);
        let mut out = Vec::new();
        a.sd.tick(&a.monitor.clone(), |_| true, &mut out);
        assert_eq!(out.len(), 1);
        deliver(out, &mut b, "A");
        assert!(b.props().iter().any(|p| p.id == 2 && (p.x - 0.5).abs() < 1e-6));
        // B removes A's snowman; A loses it too.
        b.set_props(vec![snowman(2, 0.5)]);
        let mut out = Vec::new();
        b.sd.tick(&b.monitor.clone(), |_| true, &mut out);
        deliver(out, &mut a, "B");
        assert_eq!(ids(&a), [2]);
        // Nothing echoes back.
        let mut out = Vec::new();
        a.sd.tick(&a.monitor.clone(), |_| true, &mut out);
        assert!(out.is_empty(), "{out:?}");
        // A stops: both get their own decorations back.
        let mut out = Vec::new();
        a.sd.command(&Command::ShareStop, &a.monitor.clone(), &mut out);
        assert_eq!(a.props(), vec![snowman(1, 0.2)]);
        let n = deliver(out, &mut b, "A");
        assert!(n.is_empty());
        assert_eq!(b.props(), vec![snowman(2, 0.8)]);
        assert_eq!(b.sd.state().status, ShareStatus::None);
    }

    #[test]
    fn strangers_and_second_invites_are_turned_down() {
        let mut a = Side::new("c");
        let mut out = Vec::new();
        a.sd.message("X", SceneMsg::Invite, &a.monitor.clone(), &mut out);
        assert_eq!(a.sd.state().status, ShareStatus::Asked);
        // A second friend asking meanwhile is declined; X's request stays.
        a.sd.message("Y", SceneMsg::Invite, &a.monitor.clone(), &mut out);
        assert_eq!(out, [("Y".to_string(), SceneMsg::Decline)]);
        // Someone else can't push decorations in.
        a.sd.message("Y", SceneMsg::Upsert { prop: snowman(9, 0.5) }, &a.monitor.clone(), &mut out);
        assert!(a.props().is_empty());
    }

    #[test]
    fn offline_friend_gets_everything_again_on_return() {
        let (mut a, mut b) = (Side::new("d"), Side::new("e"));
        a.set_props(vec![snowman(1, 0.3)]);
        let mut out = Vec::new();
        a.sd.command(&Command::ShareInvite { code: "B".into() }, &a.monitor.clone(), &mut out);
        let _ = deliver(out, &mut b, "A");
        let mut out = Vec::new();
        b.sd.command(&Command::ShareAccept { code: "A".into() }, &b.monitor.clone(), &mut out);
        let back = deliver(out, &mut a, "B");
        deliver(back, &mut b, "A");
        let mut out = Vec::new();
        a.sd.tick(&a.monitor.clone(), |_| false, &mut out);
        a.set_props(vec![snowman(1, 0.6)]);
        a.sd.tick(&a.monitor.clone(), |_| false, &mut out);
        assert!(out.is_empty(), "nothing sent while they're away");
        a.sd.tick(&a.monitor.clone(), |_| true, &mut out);
        deliver(out, &mut b, "A");
        assert!(b.props().iter().any(|p| p.id == 1 && (p.x - 0.6).abs() < 1e-6));
    }
}
