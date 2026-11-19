//! WebSocket messages (JSON, one object per text frame, tagged by `t`).
//!
//! Login: the client sends `hello` with its ed25519 public key, the server answers with a
//! random `challenge`, the client signs `AUTH_PREFIX + nonce` and sends `auth`. From then on
//! the session is that key; its friend code is derived from it.
//!
//! The server never sees names (nicknames stay on each computer) and never reads `relay`
//! payloads: friends encrypt them end to end.

use serde::{Deserialize, Serialize};

/// What a client signs to log in: this prefix followed by the 32 nonce bytes.
pub const AUTH_PREFIX: &[u8] = b"snowy-hub-auth-v1:";

/// Largest relay payload (base64 text) accepted.
pub const MAX_RELAY: usize = 4096;

#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum In {
    /// `key`: base64 of the 32-byte ed25519 public key. `ver`: app version, so the server can
    /// answer older clients differently later (not stored).
    Hello {
        key: String,
        #[allow(dead_code)]
        ver: String,
    },
    /// `sig`: base64 of the 64-byte signature over AUTH_PREFIX + nonce.
    Auth { sig: String },
    /// Ask to be friends (or accept, if they already asked).
    Add { code: String },
    Accept { code: String },
    Decline { code: String },
    Remove { code: String },
    /// Opaque, end-to-end encrypted message for a friend who is online.
    Relay { to: String, data: String },
    Ping,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Friend {
    pub code: String,
    pub online: bool,
    /// Base64 public key, so friends can encrypt to each other and check who they talk to.
    pub key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Out {
    Challenge { nonce: String },
    Welcome { code: String, friends: Vec<Friend>, requests: Vec<String>, sent: Vec<String> },
    /// Someone asked to be your friend.
    Request { code: String },
    /// Your request is waiting for them.
    RequestSent { code: String },
    FriendAdded { friend: Friend },
    FriendRemoved { code: String },
    Presence { code: String, online: bool },
    Relay { from: String, data: String },
    Error { code: ErrorCode },
    Pong,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadMessage,
    NotLoggedIn,
    BadSignature,
    UnknownCode,
    OwnCode,
    NotFriends,
    Offline,
    TooMany,
    TooLarge,
    SlowDown,
}
