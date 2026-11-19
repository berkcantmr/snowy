//! This installation's identity: an ed25519 key made on first use and kept in
//! `%APPDATA%\Snowy\identity.key`. Its friend code is derived from the public key the same way
//! the server does it (see server/src/code.rs).

use anyhow::{bail, Result};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::path::Path;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Loads the key from `path`, or makes and saves a new one.
pub fn load_or_create(path: &Path) -> Result<SigningKey> {
    if let Ok(bytes) = std::fs::read(path) {
        let Ok(seed) = <[u8; 32]>::try_from(bytes.as_slice()) else {
            bail!("{} is damaged", path.display());
        };
        return Ok(SigningKey::from_bytes(&seed));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| anyhow::anyhow!("no randomness for a new key: {e}"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write to a temporary file first so a crash can't leave half a key.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, seed)?;
    std::fs::rename(&tmp, path)?;
    Ok(SigningKey::from_bytes(&seed))
}

/// Friend code of a public key, e.g. `7KQ2-M9XD`.
pub fn friend_code(public: &[u8; 32]) -> String {
    let h = Sha256::digest(public);
    let mut v: u64 = 0;
    for b in &h[..5] {
        v = (v << 8) | *b as u64;
    }
    let mut s = String::with_capacity(9);
    for i in (0..8).rev() {
        s.push(ALPHABET[((v >> (i * 5)) & 31) as usize] as char);
        if i == 4 {
            s.push('-');
        }
    }
    s
}

/// Cleans up a typed code (case, spaces, dashes, O->0, I/L->1). None unless 8 valid symbols.
pub fn normalize_code(input: &str) -> Option<String> {
    let mut out = String::with_capacity(9);
    for c in input.chars() {
        let c = match c.to_ascii_uppercase() {
            ' ' | '-' | '_' => continue,
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) {
            return None;
        }
        if out.len() == 4 {
            out.push('-');
        }
        out.push(c);
        if out.len() > 9 {
            return None;
        }
    }
    (out.len() == 9).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_codes_as_the_server() {
        // server/src/code.rs uses the same derivation; keep these in sync.
        let a = friend_code(&[7u8; 32]);
        // Same vector in server/src/code.rs and crates/net/src/identity.rs.
        assert_eq!(a, "9ER6-Z3JE");
        assert_eq!(a.len(), 9);
        assert_eq!(normalize_code(&a.to_lowercase()).as_deref(), Some(a.as_str()));
        assert_eq!(normalize_code("abcd efgh").as_deref(), Some("ABCD-EFGH"));
        assert_eq!(normalize_code("ABCU-EFGH"), None);
    }

    #[test]
    fn key_is_made_once_and_kept() {
        let dir = std::env::temp_dir().join(format!("snowy-net-id-{}", std::process::id()));
        let p = dir.join("identity.key");
        let _ = std::fs::remove_file(&p);
        let a = load_or_create(&p).unwrap();
        let b = load_or_create(&p).unwrap();
        assert_eq!(a.to_bytes(), b.to_bytes());
        std::fs::write(&p, b"short").unwrap();
        assert!(load_or_create(&p).is_err(), "a damaged key is not silently replaced");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
