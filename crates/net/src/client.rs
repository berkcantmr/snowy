//! The connection to the friends server, on its own thread: logs in with the identity key,
//! keeps `FriendsState` current, sends commands, pings, and reconnects with backoff.

use crate::identity::friend_code;
use crate::state::{Action, ActionKind, Command, Friend, FriendsState, SceneMsg, Status};
use std::collections::HashMap;
use anyhow::{anyhow, bail, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde::Deserialize;
use std::net::TcpStream;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// What a client signs to log in (server/src/proto.rs).
const AUTH_PREFIX: &[u8] = b"snowy-hub-auth-v1:";
const PING_EVERY: Duration = Duration::from_secs(30);
const SILENT_LIMIT: Duration = Duration::from_secs(90);

enum Ctl {
    Cmd(Command),
    Stop,
}

/// Handle to the connection thread. Dropping it disconnects.
pub struct Hub {
    state: Arc<Mutex<FriendsState>>,
    tx: Sender<Ctl>,
    /// Actions friends did on this desktop, decrypted, not yet handled.
    inbox: Arc<Mutex<Vec<(String, Action)>>>,
    /// Shared desktop messages, decrypted, not yet handled.
    scene: Arc<Mutex<Vec<(String, SceneMsg)>>>,
}

/// Inside every relayed message (after decryption): an action or a shared desktop message.
#[derive(serde::Serialize, Deserialize)]
struct Envelope {
    v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    a: Option<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s: Option<SceneMsg>,
}

impl Hub {
    /// Connects to `url` (wss://…/ws) as `key` and stays connected until dropped.
    pub fn start(key: SigningKey, url: String, ver: String) -> Hub {
        let code = friend_code(&key.verifying_key().to_bytes());
        let state = Arc::new(Mutex::new(FriendsState { status: Status::Connecting, code, rev: 1, ..Default::default() }));
        let (tx, rx) = channel();
        let (st, inbox, scene) = (state.clone(), Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
        let (ib, sc) = (inbox.clone(), scene.clone());
        let _ = std::thread::Builder::new().name("snowy-hub".into()).spawn(move || run(key, url, ver, st, ib, sc, rx));
        Hub { state, tx, inbox, scene }
    }

    /// Takes the shared desktop messages friends sent since the last call.
    pub fn take_scene(&self) -> Vec<(String, SceneMsg)> {
        self.scene.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
    }

    /// Takes the actions friends sent since the last call: (friend code, action).
    pub fn take_incoming(&self) -> Vec<(String, Action)> {
        self.inbox.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
    }

    pub fn state(&self) -> FriendsState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(Ctl::Cmd(cmd));
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.tx.send(Ctl::Stop);
    }
}

fn update(st: &Mutex<FriendsState>, f: impl FnOnce(&mut FriendsState)) {
    if let Ok(mut s) = st.lock() {
        f(&mut s);
        s.rev += 1;
    }
}

type Queue<T> = Arc<Mutex<Vec<(String, T)>>>;

fn run(key: SigningKey, url: String, ver: String, st: Arc<Mutex<FriendsState>>, inbox: Queue<Action>, scene: Queue<SceneMsg>, rx: Receiver<Ctl>) {
    let mut backoff = Duration::from_secs(2);
    let mut sent: HashMap<(String, ActionKind), Instant> = HashMap::new();
    loop {
        update(&st, |s| s.status = Status::Connecting);
        let mut welcomed = false;
        let mut io = Io { st: &st, inbox: &inbox, scene: &scene, rx: &rx, sent: &mut sent };
        match session(&key, &url, &ver, &mut io, &mut welcomed) {
            Ok(()) => break,
            Err(e) => log::info!("friends server: {e}"),
        }
        if welcomed {
            backoff = Duration::from_secs(2);
        }
        update(&st, |s| {
            s.status = Status::Retrying;
            for f in &mut s.friends {
                f.online = false;
            }
        });
        // Wait before retrying; commands meanwhile can't be sent.
        let until = Instant::now() + backoff;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(Ctl::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Ctl::Cmd(_)) => update(&st, |s| s.last_error = Some("offline".into())),
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
    update(&st, |s| s.status = Status::Off);
}

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

fn set_timeout(ws: &mut Ws, t: Duration) {
    let _ = match ws.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(Some(t)),
        MaybeTlsStream::Rustls(s) => s.sock.set_read_timeout(Some(t)),
        _ => Ok(()),
    };
}

fn send(ws: &mut Ws, v: serde_json::Value) -> Result<()> {
    ws.send(Message::Text(v.to_string().into()))?;
    Ok(())
}

/// Next text message, or None when the read timed out.
fn read(ws: &mut Ws) -> Result<Option<In>> {
    match ws.read() {
        Ok(Message::Text(t)) => Ok(serde_json::from_str(&t).ok()),
        Ok(Message::Close(_)) => bail!("closed by the server"),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn expect(ws: &mut Ws, what: &str, f: impl Fn(&In) -> bool) -> Result<In> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Some(m) = read(ws)? {
            if f(&m) {
                return Ok(m);
            }
            if let In::Error { code } = m {
                bail!("login refused: {code}");
            }
        }
    }
    Err(anyhow!("no {what} from the server"))
}

/// What a session reads from and writes to.
struct Io<'a> {
    st: &'a Mutex<FriendsState>,
    inbox: &'a Mutex<Vec<(String, Action)>>,
    scene: &'a Mutex<Vec<(String, SceneMsg)>>,
    rx: &'a Receiver<Ctl>,
    /// Last time each action went to each friend (sender-side rate limit).
    sent: &'a mut HashMap<(String, ActionKind), Instant>,
}

fn friend_key(st: &Mutex<FriendsState>, code: &str) -> Option<String> {
    st.lock().ok()?.friends.iter().find(|f| f.code == code).map(|f| f.key.clone())
}

/// Encrypts an action for a friend and hands it to the server.
fn send_action(ws: &mut Ws, key: &SigningKey, io: &mut Io, to: String, action: Action) -> Result<()> {
    let Some(peer) = friend_key(io.st, &to) else {
        update(io.st, |s| s.last_error = Some("not_friends".into()));
        return Ok(());
    };
    let k = (to.clone(), action.kind());
    if io.sent.get(&k).is_some_and(|t| t.elapsed() < action.kind().min_gap()) {
        update(io.st, |s| s.last_error = Some("slow_down".into()));
        return Ok(());
    }
    let plain = serde_json::to_vec(&Envelope { v: 1, a: Some(action), s: None })?;
    let data = crate::e2e::seal(key, &peer, &plain)?;
    send(ws, serde_json::json!({ "t": "relay", "to": to, "data": data }))?;
    io.sent.insert(k, Instant::now());
    Ok(())
}

/// Encrypts a shared desktop message for a friend.
fn send_scene(ws: &mut Ws, key: &SigningKey, io: &Io, to: String, msg: SceneMsg) -> Result<()> {
    let Some(peer) = friend_key(io.st, &to) else { return Ok(()) };
    let plain = serde_json::to_vec(&Envelope { v: 1, a: None, s: Some(msg) })?;
    let data = crate::e2e::seal(key, &peer, &plain)?;
    send(ws, serde_json::json!({ "t": "relay", "to": to, "data": data }))
}

fn push<T>(q: &Mutex<Vec<(String, T)>>, from: String, item: T, cap: usize) {
    if let Ok(mut v) = q.lock() {
        if v.len() < cap {
            v.push((from, item));
        }
    }
}

/// A relayed message from a friend: decrypt, check, queue.
fn receive(key: &SigningKey, io: &Io, from: String, data: &str) {
    let Some(peer) = friend_key(io.st, &from) else { return };
    let env = crate::e2e::open(key, &peer, data).ok().and_then(|b| serde_json::from_slice::<Envelope>(&b).ok()).filter(|e| e.v == 1);
    match env {
        Some(Envelope { a: Some(a), .. }) => push(io.inbox, from, a.sanitized(), 64),
        Some(Envelope { s: Some(mut s), .. }) => {
            if let SceneMsg::Upsert { prop } = &mut s {
                snowy_core::props::sanitize_prop(prop);
            }
            push(io.scene, from, s, 512);
        }
        _ => log::info!("friends: dropped a message from {from} that didn't open"),
    }
}

/// One connection. Ok(()) only when told to stop.
fn session(key: &SigningKey, url: &str, ver: &str, io: &mut Io, welcomed: &mut bool) -> Result<()> {
    let (st, rx) = (io.st, io.rx);
    let (mut ws, _) = tungstenite::connect(url)?;
    set_timeout(&mut ws, Duration::from_millis(300));
    send(&mut ws, serde_json::json!({ "t": "hello", "key": B64.encode(key.verifying_key().to_bytes()), "ver": ver }))?;
    let In::Challenge { nonce } = expect(&mut ws, "challenge", |m| matches!(m, In::Challenge { .. }))? else { unreachable!() };
    let mut msg = AUTH_PREFIX.to_vec();
    msg.extend_from_slice(&B64.decode(nonce.as_bytes())?);
    send(&mut ws, serde_json::json!({ "t": "auth", "sig": B64.encode(key.sign(&msg).to_bytes()) }))?;
    let welcome = expect(&mut ws, "welcome", |m| matches!(m, In::Welcome { .. }))?;
    *welcomed = true;
    update(st, |s| {
        apply(s, welcome);
        s.status = Status::Online;
    });
    log::info!("friends server: connected");

    let (mut last_ping, mut last_heard) = (Instant::now(), Instant::now());
    loop {
        loop {
            match rx.try_recv() {
                Ok(Ctl::Cmd(Command::Send { to, action })) => {
                    update(st, |s| s.last_error = None);
                    send_action(&mut ws, key, io, to, action)?;
                }
                Ok(Ctl::Cmd(Command::Scene { to, msg })) => send_scene(&mut ws, key, io, to, msg)?,
                Ok(Ctl::Cmd(c)) if c.is_share() => {}
                Ok(Ctl::Cmd(c)) => {
                    update(st, |s| s.last_error = None);
                    send(&mut ws, serde_json::to_value(&c)?)?;
                }
                Ok(Ctl::Stop) | Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    return Ok(());
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if last_ping.elapsed() >= PING_EVERY {
            send(&mut ws, serde_json::json!({ "t": "ping" }))?;
            last_ping = Instant::now();
        }
        if let Some(m) = read(&mut ws)? {
            last_heard = Instant::now();
            if let In::Relay { from, data } = m {
                receive(key, io, from, &data);
                continue;
            }
            update(st, |s| apply(s, m));
        } else if last_heard.elapsed() > SILENT_LIMIT {
            bail!("the server went quiet");
        }
    }
}

/// Server messages (server/src/proto.rs `Out`).
#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum In {
    Challenge { nonce: String },
    Welcome { code: String, friends: Vec<Friend>, requests: Vec<String>, sent: Vec<String> },
    Request { code: String },
    RequestSent { code: String },
    FriendAdded { friend: Friend },
    FriendRemoved { code: String },
    Presence { code: String, online: bool },
    Relay { from: String, data: String },
    Error { code: String },
    Pong,
}

fn apply(s: &mut FriendsState, m: In) {
    let push = |v: &mut Vec<String>, c: String| {
        if !v.contains(&c) {
            v.push(c);
        }
    };
    match m {
        In::Welcome { code, friends, requests, sent } => {
            s.code = code;
            s.friends = friends;
            s.requests = requests;
            s.sent = sent;
        }
        In::Request { code } => push(&mut s.requests, code),
        In::RequestSent { code } => push(&mut s.sent, code),
        In::FriendAdded { friend } => {
            s.requests.retain(|c| *c != friend.code);
            s.sent.retain(|c| *c != friend.code);
            match s.friends.iter_mut().find(|f| f.code == friend.code) {
                Some(f) => *f = friend,
                None => s.friends.push(friend),
            }
        }
        In::FriendRemoved { code } => {
            s.friends.retain(|f| f.code != code);
            s.requests.retain(|c| *c != code);
            s.sent.retain(|c| *c != code);
        }
        In::Presence { code, online } => {
            if let Some(f) = s.friends.iter_mut().find(|f| f.code == code) {
                f.online = online;
            }
        }
        In::Error { code } if code != "slow_down" => s.last_error = Some(code),
        In::Error { .. } | In::Challenge { .. } | In::Relay { .. } | In::Pong => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_follows_server_messages() {
        let mut s = FriendsState::default();
        let f = |code: &str, online| Friend { code: code.into(), online, key: "k".into() };
        apply(&mut s, In::Welcome { code: "ME00-0000".into(), friends: vec![f("AAAA-AAAA", true)], requests: vec!["BBBB-BBBB".into()], sent: vec![] });
        apply(&mut s, In::RequestSent { code: "CCCC-CCCC".into() });
        apply(&mut s, In::FriendAdded { friend: f("BBBB-BBBB", true) });
        apply(&mut s, In::Presence { code: "AAAA-AAAA".into(), online: false });
        apply(&mut s, In::Error { code: "unknown_code".into() });
        assert_eq!(s.friends, [f("AAAA-AAAA", false), f("BBBB-BBBB", true)]);
        assert!(s.requests.is_empty());
        assert_eq!(s.sent, ["CCCC-CCCC"]);
        assert_eq!(s.last_error.as_deref(), Some("unknown_code"));
        apply(&mut s, In::FriendRemoved { code: "AAAA-AAAA".into() });
        assert_eq!(s.friends.len(), 1);
    }

    /// Talks to the real server. Run with `cargo test -p snowy-net -- --ignored live`.
    #[test]
    #[ignore]
    fn live_two_clients_become_friends() {
        let url = std::env::var("SNOWY_HUB").unwrap_or_else(|_| crate::HUB_URL.to_string());
        let mk = |seed: u8| {
            let mut s = [0u8; 32];
            getrandom::getrandom(&mut s).unwrap();
            s[0] = seed;
            SigningKey::from_bytes(&s)
        };
        let a = Hub::start(mk(1), url.clone(), "test".into());
        let b = Hub::start(mk(2), url, "test".into());
        let wait = |h: &Hub, f: &dyn Fn(&FriendsState) -> bool| {
            let t = Instant::now();
            while t.elapsed() < Duration::from_secs(15) {
                if f(&h.state()) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            panic!("timed out: {:?}", h.state());
        };
        wait(&a, &|s| s.status == Status::Online);
        wait(&b, &|s| s.status == Status::Online);
        a.send(Command::Add { code: b.state().code });
        wait(&b, &|s| !s.requests.is_empty());
        b.send(Command::Accept { code: a.state().code });
        wait(&a, &|s| s.friends.iter().any(|f| f.online));
        wait(&b, &|s| s.friends.iter().any(|f| f.online));
        // An encrypted snowball goes through the server and opens on the other side.
        a.send(Command::Send { to: b.state().code, action: Action::Snowball { x: 0.25 } });
        let t = Instant::now();
        let mut got = Vec::new();
        while got.is_empty() && t.elapsed() < Duration::from_secs(10) {
            got = b.take_incoming();
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(got, [(a.state().code, Action::Snowball { x: 0.25 })]);
        // A second one right away is held back by the sender's rate limit.
        a.send(Command::Send { to: b.state().code, action: Action::Snowball { x: 0.5 } });
        wait(&a, &|s| s.last_error.as_deref() == Some("slow_down"));
        drop(b);
        wait(&a, &|s| s.friends.iter().all(|f| !f.online));
        // Clean up the test friendship.
        let other = a.state().friends[0].code.clone();
        a.send(Command::Remove { code: other });
        wait(&a, &|s| s.friends.is_empty());
    }
}
