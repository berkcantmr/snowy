//! Files the main app and the settings window share (both in `%APPDATA%\Snowy`):
//!
//! - `friends.json`: what the main app knows (written by the app, read by the window).
//! - `friend-names.json`: nicknames, only on this computer (written by the window).
//! - `friends-cmd\*.json`: actions the window asks the app to send, one file each, consumed
//!   and deleted by the app.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Friends are turned off in settings.
    #[default]
    Off,
    Connecting,
    Online,
    /// Lost the connection; trying again shortly.
    Retrying,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Friend {
    pub code: String,
    pub online: bool,
    /// Base64 public key (for end-to-end encryption between friends).
    pub key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FriendsState {
    pub status: Status,
    /// This computer's friend code (known even before the first connection).
    pub code: String,
    pub friends: Vec<Friend>,
    /// People who asked to be friends.
    pub requests: Vec<String>,
    /// Requests this person sent that are still waiting.
    pub sent: Vec<String>,
    /// i18n-able error id of the last failed action (e.g. "unknown_code"), cleared on success.
    pub last_error: Option<String>,
    /// Bumped on every change, so readers can tell.
    pub rev: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Command {
    Add { code: String },
    Accept { code: String },
    Decline { code: String },
    Remove { code: String },
    /// Do something on a friend's desktop (sent end-to-end encrypted).
    Send { to: String, action: Action },
    /// Shared desktop, from the settings window; handled by the app (see `SharedState`).
    ShareInvite { code: String },
    ShareAccept { code: String },
    ShareDecline { code: String },
    ShareStop,
    /// Shared desktop message for a friend (sent by the app itself, end-to-end encrypted).
    Scene { to: String, msg: SceneMsg },
}

impl Command {
    /// True for commands the app handles itself instead of passing to the server.
    pub fn is_share(&self) -> bool {
        matches!(self, Command::ShareInvite { .. } | Command::ShareAccept { .. } | Command::ShareDecline { .. } | Command::ShareStop)
    }
}

/// Shared desktop messages between two friends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SceneMsg {
    /// "Let's share our desktops."
    Invite,
    Accept,
    Decline,
    /// Sharing ends; both go back to their own decorations.
    Stop,
    /// A decoration was added or changed.
    Upsert { prop: snowy_core::props::Prop },
    Remove { id: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareStatus {
    #[default]
    None,
    /// This person invited `with`; waiting for an answer.
    Invited,
    /// `with` invited this person.
    Asked,
    /// Sharing with `with`.
    Active,
}

/// The shared desktop, as the app last wrote it (`shared.json`, read by the settings window).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SharedState {
    pub with: Option<String>,
    pub status: ShareStatus,
}

pub fn shared_path(dir: &Path) -> PathBuf {
    dir.join("shared.json")
}

pub fn load_shared(dir: &Path) -> SharedState {
    std::fs::read(shared_path(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_shared(dir: &Path, s: &SharedState) -> std::io::Result<()> {
    write_json(&shared_path(dir), s)
}

/// Something one friend does on the other's desktop. Only these fixed shapes exist: no text,
/// files or links can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// A snowball flies in from the top; `x` is where, 0..1 across the screen.
    Snowball { x: f32 },
    /// A gift box floats down on a parachute.
    Gift,
}

impl Action {
    pub fn kind(&self) -> ActionKind {
        match self {
            Action::Snowball { .. } => ActionKind::Snowball,
            Action::Gift => ActionKind::Gift,
        }
    }

    /// Clamps values from the wire into range.
    pub fn sanitized(self) -> Action {
        match self {
            Action::Snowball { x } => Action::Snowball { x: if x.is_finite() { x.clamp(0.02, 0.98) } else { 0.5 } },
            Action::Gift => Action::Gift,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionKind {
    Snowball,
    Gift,
}

impl ActionKind {
    /// Least time between two of these from (or to) the same friend.
    pub fn min_gap(self) -> std::time::Duration {
        match self {
            ActionKind::Snowball => std::time::Duration::from_millis(1500),
            ActionKind::Gift => std::time::Duration::from_secs(10 * 60),
        }
    }
}

/// What one friend may do on this desktop. Everything is allowed until turned off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Perms {
    pub snowballs: bool,
    pub gifts: bool,
}

impl Default for Perms {
    fn default() -> Self {
        Perms { snowballs: true, gifts: true }
    }
}

impl Perms {
    pub fn allows(&self, kind: ActionKind) -> bool {
        match kind {
            ActionKind::Snowball => self.snowballs,
            ActionKind::Gift => self.gifts,
        }
    }
}

pub fn perms_path(dir: &Path) -> PathBuf {
    dir.join("friend-perms.json")
}

/// Permissions by friend code (missing = default).
pub fn load_perms(dir: &Path) -> BTreeMap<String, Perms> {
    std::fs::read(perms_path(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_perms(dir: &Path, perms: &BTreeMap<String, Perms>) -> std::io::Result<()> {
    write_json(&perms_path(dir), perms)
}

pub fn state_path(dir: &Path) -> PathBuf {
    dir.join("friends.json")
}

pub fn names_path(dir: &Path) -> PathBuf {
    dir.join("friend-names.json")
}

pub fn cmd_dir(dir: &Path) -> PathBuf {
    dir.join("friends-cmd")
}

/// Writes atomically (temp file + rename) so a reader never sees half a file.
fn write_json<T: Serialize>(path: &Path, v: &T) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v).unwrap_or_default())?;
    std::fs::rename(&tmp, path)
}

pub fn load_state(dir: &Path) -> FriendsState {
    std::fs::read(state_path(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_state(dir: &Path, s: &FriendsState) -> std::io::Result<()> {
    write_json(&state_path(dir), s)
}

/// Nicknames by friend code.
pub fn load_names(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read(names_path(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_names(dir: &Path, names: &BTreeMap<String, String>) -> std::io::Result<()> {
    write_json(&names_path(dir), names)
}

/// Queues a command for the main app.
pub fn send_command(dir: &Path, cmd: &Command) -> std::io::Result<()> {
    let d = cmd_dir(dir);
    std::fs::create_dir_all(&d)?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    write_json(&d.join(format!("{nanos}.json")), cmd)
}

/// Takes all queued commands, oldest first (the files are deleted).
pub fn take_commands(dir: &Path) -> Vec<Command> {
    let Ok(rd) = std::fs::read_dir(cmd_dir(dir)) else { return Vec::new() };
    let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        if let Ok(b) = std::fs::read(&f) {
            if let Ok(c) = serde_json::from_slice(&b) {
                out.push(c);
            }
        }
        let _ = std::fs::remove_file(&f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_commands_parse() {
        let c: Command = serde_json::from_str(r#"{"t":"share_invite","code":"EMFM-SAM6"}"#).unwrap();
        assert_eq!(c, Command::ShareInvite { code: "EMFM-SAM6".into() });
        assert!(c.is_share());
        let c: Command = serde_json::from_str(r#"{"t":"share_stop"}"#).unwrap();
        assert_eq!(c, Command::ShareStop);
    }

    #[test]
    fn commands_round_trip_in_order() {
        let dir = std::env::temp_dir().join(format!("snowy-net-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        send_command(&dir, &Command::Add { code: "AAAA-AAAA".into() }).unwrap();
        send_command(&dir, &Command::Remove { code: "BBBB-BBBB".into() }).unwrap();
        assert_eq!(take_commands(&dir), [Command::Add { code: "AAAA-AAAA".into() }, Command::Remove { code: "BBBB-BBBB".into() }]);
        assert!(take_commands(&dir).is_empty());
        let mut names = BTreeMap::new();
        names.insert("AAAA-AAAA".to_string(), "Ayşe".to_string());
        save_names(&dir, &names).unwrap();
        assert_eq!(load_names(&dir), names);
        let s = FriendsState { status: Status::Online, code: "X".into(), rev: 3, ..Default::default() };
        save_state(&dir, &s).unwrap();
        assert_eq!(load_state(&dir), s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
