//! Sessions, friends and presence. Transport-free: a session gets `In` messages and sends
//! `Out` messages through a channel, so this is tested without sockets.

use crate::code;
use crate::db::Db;
use crate::proto::{ErrorCode, Friend, In, Out, AUTH_PREFIX, MAX_RELAY};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use rand::RngCore;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

/// Pending requests one person may have out at a time.
const MAX_PENDING: usize = 50;
/// Messages per second a session may send (burst: twice that).
const RATE: f64 = 20.0;

pub type Tx = UnboundedSender<Out>;

struct Online {
    session: u64,
    tx: Tx,
}

#[derive(Default)]
struct Today {
    day: String,
    seen: HashSet<String>,
    peak: usize,
}

pub struct Hub {
    db: Mutex<Db>,
    online: Mutex<HashMap<String, Online>>,
    today: Mutex<Today>,
    next_session: Mutex<u64>,
}

pub fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

enum State {
    New,
    Challenged { key: [u8; 32], nonce: [u8; 32] },
    In { code: String },
}

/// One connection.
pub struct Session {
    id: u64,
    tx: Tx,
    state: State,
    tokens: f64,
    last: Instant,
    strikes: u32,
}

impl Session {
    /// Logged-in friend code, if any.
    pub fn code(&self) -> Option<&str> {
        match &self.state {
            State::In { code } => Some(code),
            _ => None,
        }
    }

    /// True when the session should be dropped (keeps flooding after being told to slow down).
    pub fn abusive(&self) -> bool {
        self.strikes > 100
    }

    fn send(&self, m: Out) {
        let _ = self.tx.send(m);
    }

    fn err(&self, code: ErrorCode) {
        self.send(Out::Error { code });
    }

    /// Token bucket; false = drop this message.
    fn allow(&mut self) -> bool {
        let t = Instant::now();
        self.tokens = (self.tokens + t.duration_since(self.last).as_secs_f64() * RATE).min(RATE * 2.0);
        self.last = t;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            self.strikes += 1;
            false
        }
    }
}

impl Hub {
    pub fn new(db: Db) -> Hub {
        Hub { db: Mutex::new(db), online: Mutex::new(HashMap::new()), today: Mutex::default(), next_session: Mutex::new(0) }
    }

    pub fn db(&self) -> std::sync::MutexGuard<'_, Db> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn online(&self) -> std::sync::MutexGuard<'_, HashMap<String, Online>> {
        self.online.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn online_count(&self) -> usize {
        self.online().len()
    }

    pub fn open(&self, tx: Tx) -> Session {
        let mut n = self.next_session.lock().unwrap_or_else(|e| e.into_inner());
        *n += 1;
        Session { id: *n, tx, state: State::New, tokens: RATE * 2.0, last: Instant::now(), strikes: 0 }
    }

    fn is_online(&self, code: &str) -> bool {
        self.online().contains_key(code)
    }

    fn send_to(&self, code: &str, m: Out) -> bool {
        match self.online().get(code) {
            Some(o) => o.tx.send(m).is_ok(),
            None => false,
        }
    }

    fn friend(&self, code: &str, key: String) -> Friend {
        Friend { code: code.to_string(), online: self.is_online(code), key }
    }

    fn friend_by_code(&self, code: &str) -> Option<Friend> {
        let key = self.db().key_of(code).ok()??;
        Some(self.friend(code, key))
    }

    /// Tell `code`'s online friends whether it is online.
    fn announce(&self, code: &str, online: bool) {
        let friends = self.db().friends_of(code).unwrap_or_default();
        for (f, _) in friends {
            self.send_to(&f, Out::Presence { code: code.to_string(), online });
        }
    }

    fn note_today(&self, code: &str) {
        let online = self.online_count();
        let mut t = self.today.lock().unwrap_or_else(|e| e.into_inner());
        let day = today();
        if t.day != day {
            *t = Today { day: day.clone(), ..Default::default() };
        }
        t.seen.insert(code.to_string());
        t.peak = t.peak.max(online);
        let users = (t.seen.len() as i64).max(self.db().users_seen(&day).unwrap_or(0));
        let _ = self.db().record_hub(&day, users, t.peak as i64);
    }

    pub fn close(&self, s: &Session) {
        let Some(code) = s.code() else { return };
        let removed = {
            let mut on = self.online();
            match on.get(code) {
                Some(o) if o.session == s.id => on.remove(code).is_some(),
                _ => false,
            }
        };
        if removed {
            self.announce(code, false);
        }
    }

    /// A frame that isn't a valid message: answered (rate-limited), never fatal.
    pub fn handle_bad(&self, s: &mut Session) {
        if s.allow() {
            s.err(ErrorCode::BadMessage);
        }
    }

    pub fn handle(&self, s: &mut Session, msg: In) {
        if !s.allow() {
            if s.strikes % 20 == 1 {
                s.err(ErrorCode::SlowDown);
            }
            return;
        }
        match (msg, &s.state) {
            (In::Ping, _) => s.send(Out::Pong),
            (In::Hello { key, ver: _ }, State::New) => {
                let Some(key) = B64.decode(key.as_bytes()).ok().and_then(|k| <[u8; 32]>::try_from(k).ok()) else {
                    return s.err(ErrorCode::BadMessage);
                };
                if VerifyingKey::from_bytes(&key).is_err() {
                    return s.err(ErrorCode::BadMessage);
                }
                let mut nonce = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut nonce);
                s.state = State::Challenged { key, nonce };
                s.send(Out::Challenge { nonce: B64.encode(nonce) });
            }
            (In::Auth { sig }, State::Challenged { key, nonce }) => {
                let (key, nonce) = (*key, *nonce);
                let ok = B64
                    .decode(sig.as_bytes())
                    .ok()
                    .and_then(|b| <[u8; 64]>::try_from(b).ok())
                    .zip(VerifyingKey::from_bytes(&key).ok())
                    .is_some_and(|(sig, vk)| {
                        let mut msg = AUTH_PREFIX.to_vec();
                        msg.extend_from_slice(&nonce);
                        vk.verify(&msg, &Signature::from_bytes(&sig)).is_ok()
                    });
                if !ok {
                    s.state = State::New;
                    return s.err(ErrorCode::BadSignature);
                }
                let code = code::from_key(&key);
                let key_b64 = B64.encode(key);
                if !self.db().login(&key_b64, &code, &today(), now()).unwrap_or(false) {
                    s.state = State::New;
                    return s.err(ErrorCode::BadSignature);
                }
                // A newer connection of the same person replaces the old one.
                self.online().insert(code.clone(), Online { session: s.id, tx: s.tx.clone() });
                s.state = State::In { code: code.clone() };
                self.note_today(&code);
                let (friends, requests, sent) = {
                    let db = self.db();
                    (db.friends_of(&code).unwrap_or_default(), db.incoming(&code).unwrap_or_default(), db.outgoing(&code).unwrap_or_default())
                };
                let friends = friends.into_iter().map(|(c, k)| self.friend(&c, k)).collect();
                s.send(Out::Welcome { code: code.clone(), friends, requests, sent });
                self.announce(&code, true);
            }
            (In::Hello { .. } | In::Auth { .. }, _) => s.err(ErrorCode::BadMessage),
            (_, State::New | State::Challenged { .. }) => s.err(ErrorCode::NotLoggedIn),
            (msg, State::In { code: me }) => {
                let me = me.clone();
                self.handle_in(s, &me, msg);
            }
        }
    }

    fn handle_in(&self, s: &mut Session, me: &str, msg: In) {
        let target = |c: &str| code::normalize(c);
        match msg {
            In::Add { code } => {
                let Some(other) = target(&code) else { return s.err(ErrorCode::UnknownCode) };
                if other == me {
                    return s.err(ErrorCode::OwnCode);
                }
                let db = self.db();
                if db.key_of(&other).ok().flatten().is_none() {
                    drop(db);
                    return s.err(ErrorCode::UnknownCode);
                }
                if db.are_friends(me, &other).unwrap_or(false) {
                    drop(db);
                    if let Some(f) = self.friend_by_code(&other) {
                        s.send(Out::FriendAdded { friend: f });
                    }
                    return;
                }
                if db.has_request(&other, me).unwrap_or(false) {
                    // They asked first: this is an accept.
                    let _ = db.make_friends(me, &other);
                    drop(db);
                    self.befriended(s, me, &other);
                    return;
                }
                if db.outgoing(me).map(|v| v.len()).unwrap_or(0) >= MAX_PENDING {
                    drop(db);
                    return s.err(ErrorCode::TooMany);
                }
                let _ = db.add_request(me, &other, now());
                drop(db);
                s.send(Out::RequestSent { code: other.clone() });
                self.send_to(&other, Out::Request { code: me.to_string() });
            }
            In::Accept { code } => {
                let Some(other) = target(&code) else { return s.err(ErrorCode::UnknownCode) };
                let db = self.db();
                if !db.has_request(&other, me).unwrap_or(false) {
                    drop(db);
                    return s.err(ErrorCode::UnknownCode);
                }
                let _ = db.make_friends(me, &other);
                drop(db);
                self.befriended(s, me, &other);
            }
            In::Decline { code } => {
                if let Some(other) = target(&code) {
                    let _ = self.db().drop_request(&other, me);
                    // Also withdraws one's own request.
                    let _ = self.db().drop_request(me, &other);
                }
            }
            In::Remove { code } => {
                let Some(other) = target(&code) else { return s.err(ErrorCode::UnknownCode) };
                let _ = self.db().unfriend(me, &other);
                s.send(Out::FriendRemoved { code: other.clone() });
                self.send_to(&other, Out::FriendRemoved { code: me.to_string() });
            }
            In::Relay { to, data } => {
                if data.len() > MAX_RELAY {
                    return s.err(ErrorCode::TooLarge);
                }
                let Some(other) = target(&to) else { return s.err(ErrorCode::UnknownCode) };
                if !self.db().are_friends(me, &other).unwrap_or(false) {
                    return s.err(ErrorCode::NotFriends);
                }
                if !self.send_to(&other, Out::Relay { from: me.to_string(), data }) {
                    s.err(ErrorCode::Offline);
                }
            }
            In::Hello { .. } | In::Auth { .. } | In::Ping => unreachable!("handled by the caller"),
        }
    }

    fn befriended(&self, s: &Session, me: &str, other: &str) {
        if let Some(f) = self.friend_by_code(other) {
            s.send(Out::FriendAdded { friend: f });
        }
        if let Some(f) = self.friend_by_code(me) {
            self.send_to(other, Out::FriendAdded { friend: f });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    struct Client {
        s: Session,
        rx: UnboundedReceiver<Out>,
        code: String,
    }

    impl Client {
        fn drain(&mut self) -> Vec<Out> {
            let mut v = Vec::new();
            while let Ok(m) = self.rx.try_recv() {
                v.push(m);
            }
            v
        }
    }

    fn login(hub: &Hub, seed: u8) -> Client {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let (tx, mut rx) = unbounded_channel();
        let mut s = hub.open(tx);
        hub.handle(&mut s, In::Hello { key: B64.encode(sk.verifying_key().to_bytes()), ver: "0.1.6".into() });
        let Some(Out::Challenge { nonce }) = rx.try_recv().ok() else { panic!("no challenge") };
        let mut msg = AUTH_PREFIX.to_vec();
        msg.extend_from_slice(&B64.decode(nonce).unwrap());
        hub.handle(&mut s, In::Auth { sig: B64.encode(sk.sign(&msg).to_bytes()) });
        let Some(Out::Welcome { code, .. }) = rx.try_recv().ok() else { panic!("no welcome") };
        Client { s, rx, code }
    }

    #[test]
    fn wrong_signature_is_refused() {
        let hub = Hub::new(Db::memory().unwrap());
        let sk = SigningKey::from_bytes(&[1; 32]);
        let (tx, mut rx) = unbounded_channel();
        let mut s = hub.open(tx);
        hub.handle(&mut s, In::Hello { key: B64.encode(sk.verifying_key().to_bytes()), ver: "x".into() });
        let _ = rx.try_recv();
        hub.handle(&mut s, In::Auth { sig: B64.encode([0u8; 64]) });
        assert_eq!(rx.try_recv().ok(), Some(Out::Error { code: ErrorCode::BadSignature }));
        hub.handle(&mut s, In::Add { code: "AAAA-AAAA".into() });
        assert_eq!(rx.try_recv().ok(), Some(Out::Error { code: ErrorCode::NotLoggedIn }));
        assert_eq!(hub.online_count(), 0);
    }

    #[test]
    fn friends_presence_and_relay() {
        let hub = Hub::new(Db::memory().unwrap());
        let mut a = login(&hub, 1);
        let mut b = login(&hub, 2);
        // Relay needs friendship.
        hub.handle(&mut a.s, In::Relay { to: b.code.clone(), data: "x".into() });
        assert_eq!(a.drain(), [Out::Error { code: ErrorCode::NotFriends }]);
        // A asks, B is told, B accepts by adding back.
        hub.handle(&mut a.s, In::Add { code: b.code.to_lowercase().replace('-', " ") });
        assert_eq!(a.drain(), [Out::RequestSent { code: b.code.clone() }]);
        assert_eq!(b.drain(), [Out::Request { code: a.code.clone() }]);
        hub.handle(&mut b.s, In::Add { code: a.code.clone() });
        assert!(matches!(&b.drain()[..], [Out::FriendAdded { friend }] if friend.code == a.code && friend.online));
        assert!(matches!(&a.drain()[..], [Out::FriendAdded { friend }] if friend.code == b.code && friend.online));
        // Relay works now; payload passes through untouched.
        hub.handle(&mut a.s, In::Relay { to: b.code.clone(), data: "secret".into() });
        assert_eq!(b.drain(), [Out::Relay { from: a.code.clone(), data: "secret".into() }]);
        // B leaves: A sees it offline, relays fail.
        hub.close(&b.s);
        assert_eq!(a.drain(), [Out::Presence { code: b.code.clone(), online: false }]);
        hub.handle(&mut a.s, In::Relay { to: b.code.clone(), data: "x".into() });
        assert_eq!(a.drain(), [Out::Error { code: ErrorCode::Offline }]);
        // B comes back and sees A online in the welcome; A is told B is back.
        let b2 = login(&hub, 2);
        assert_eq!(a.drain(), [Out::Presence { code: b2.code.clone(), online: true }]);
        // Unfriend.
        hub.handle(&mut a.s, In::Remove { code: b2.code.clone() });
        assert_eq!(a.drain(), [Out::FriendRemoved { code: b2.code.clone() }]);
        let mut b2 = b2;
        assert_eq!(b2.drain(), [Out::FriendRemoved { code: a.code.clone() }]);
        assert!(!hub.db().are_friends(&a.code, &b2.code).unwrap());
    }

    #[test]
    fn unknown_and_own_codes() {
        let hub = Hub::new(Db::memory().unwrap());
        let mut a = login(&hub, 1);
        hub.handle(&mut a.s, In::Add { code: "ZZZZ-ZZZZ".into() });
        hub.handle(&mut a.s, In::Add { code: a.code.clone() });
        hub.handle(&mut a.s, In::Add { code: "not a code".into() });
        assert_eq!(
            a.drain(),
            [Out::Error { code: ErrorCode::UnknownCode }, Out::Error { code: ErrorCode::OwnCode }, Out::Error { code: ErrorCode::UnknownCode }]
        );
    }

    #[test]
    fn flooding_is_throttled() {
        let hub = Hub::new(Db::memory().unwrap());
        let mut a = login(&hub, 1);
        for _ in 0..200 {
            hub.handle(&mut a.s, In::Ping);
        }
        let out = a.drain();
        let pongs = out.iter().filter(|m| **m == Out::Pong).count();
        assert!(pongs < 60, "{pongs}");
        assert!(out.contains(&Out::Error { code: ErrorCode::SlowDown }));
    }

    #[test]
    fn daily_users_are_counted() {
        let hub = Hub::new(Db::memory().unwrap());
        let _a = login(&hub, 1);
        let _b = login(&hub, 2);
        let d = &hub.db().days(1).unwrap()[0];
        assert_eq!((d.hub_users, d.peak_online), (2, 2));
    }
}
