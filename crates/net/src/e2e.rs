//! End-to-end encryption between two friends. Both sides turn their ed25519 identity keys into
//! X25519 keys and agree on a shared secret; messages are sealed with ChaCha20-Poly1305 under
//! a key derived from it. The server only passes the sealed bytes along and can't read or
//! forge them; only the two friends can.

use anyhow::{anyhow, bail, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

const LABEL: &[u8] = b"snowy-e2e-v1";

/// The symmetric key two friends share. `peer_key` is the friend's base64 public key.
fn shared_key(me: &SigningKey, peer_key: &str) -> Result<Key> {
    let peer = B64.decode(peer_key.as_bytes()).ok().and_then(|b| <[u8; 32]>::try_from(b).ok()).ok_or_else(|| anyhow!("bad friend key"))?;
    let peer = VerifyingKey::from_bytes(&peer).map_err(|_| anyhow!("bad friend key"))?;
    let secret = peer.to_montgomery() * me.to_scalar();
    if secret.as_bytes().iter().all(|b| *b == 0) {
        bail!("weak friend key");
    }
    // Bind the key to both public keys (sorted, so both sides get the same).
    let (a, b) = (me.verifying_key().to_bytes(), peer.to_bytes());
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut h = Sha256::new();
    h.update(LABEL);
    h.update(secret.as_bytes());
    h.update(lo);
    h.update(hi);
    Ok(*Key::from_slice(&h.finalize()))
}

/// Seals `plain` for the friend with public key `peer_key`: base64(nonce || ciphertext).
pub fn seal(me: &SigningKey, peer_key: &str, plain: &[u8]) -> Result<String> {
    let cipher = ChaCha20Poly1305::new(&shared_key(me, peer_key)?);
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).map_err(|e| anyhow!("no randomness: {e}"))?;
    let ct = cipher.encrypt(Nonce::from_slice(&nonce), plain).map_err(|_| anyhow!("encryption failed"))?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    Ok(B64.encode(out))
}

/// Opens a message from the friend with public key `peer_key`. Fails on anything forged,
/// changed or meant for someone else.
pub fn open(me: &SigningKey, peer_key: &str, sealed: &str) -> Result<Vec<u8>> {
    let raw = B64.decode(sealed.as_bytes()).map_err(|_| anyhow!("not base64"))?;
    if raw.len() < 12 + 16 {
        bail!("too short");
    }
    let cipher = ChaCha20Poly1305::new(&shared_key(me, peer_key)?);
    cipher.decrypt(Nonce::from_slice(&raw[..12]), &raw[12..]).map_err(|_| anyhow!("can't open: not from this friend or changed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public(k: &SigningKey) -> String {
        B64.encode(k.verifying_key().to_bytes())
    }

    #[test]
    fn friends_can_read_each_other_and_nobody_else_can() {
        let (a, b, eve) = (key(1), key(2), key(3));
        let sealed = seal(&a, &public(&b), b"snowball").unwrap();
        assert_eq!(open(&b, &public(&a), &sealed).unwrap(), b"snowball");
        // Someone else, or a wrong sender key, can't open it.
        assert!(open(&eve, &public(&a), &sealed).is_err());
        assert!(open(&b, &public(&eve), &sealed).is_err());
        // Tampering is caught.
        let mut raw = B64.decode(&sealed).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 1;
        assert!(open(&b, &public(&a), &B64.encode(raw)).is_err());
        // Same message twice looks different (fresh nonce).
        assert_ne!(seal(&a, &public(&b), b"x").unwrap(), seal(&a, &public(&b), b"x").unwrap());
        assert!(open(&b, &public(&a), "AAAA").is_err());
        assert!(seal(&a, "not a key", b"x").is_err());
    }
}
