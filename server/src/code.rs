//! Friend codes: 8 Crockford base32 characters derived from the public key, shown as
//! `ABCD-EFGH`. Easy to read out loud (no I, L, O, U) and forgiving when typed.

use sha2::{Digest, Sha256};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The friend code of a 32-byte public key.
pub fn from_key(key: &[u8; 32]) -> String {
    let h = Sha256::digest(key);
    // First 40 bits of the hash -> 8 symbols of 5 bits.
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

/// Normalizes a typed code: case, spaces and dashes don't matter, and the look-alikes
/// O->0, I/L->1 are fixed. None unless it is 8 valid symbols.
pub fn normalize(input: &str) -> Option<String> {
    let mut out = String::with_capacity(9);
    for c in input.chars() {
        let c = match c.to_ascii_uppercase() {
            ' ' | '-' | '_' => continue,
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        if !ALPHABET.contains(&(c as u8)) || !c.is_ascii() {
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
    fn codes_are_stable_and_well_formed() {
        let a = from_key(&[7u8; 32]);
        // Same vector in server/src/code.rs and crates/net/src/identity.rs.
        assert_eq!(a, "9ER6-Z3JE");
        assert_eq!(a, from_key(&[7u8; 32]));
        assert_ne!(a, from_key(&[8u8; 32]));
        assert_eq!(a.len(), 9);
        assert_eq!(&a[4..5], "-");
        assert_eq!(normalize(&a).as_deref(), Some(a.as_str()));
    }

    #[test]
    fn typing_is_forgiving() {
        assert_eq!(normalize("abcd efgh").as_deref(), Some("ABCD-EFGH"));
        assert_eq!(normalize("0o1i-lL23").as_deref(), Some("0011-1123"));
        assert_eq!(normalize("ABCD-EFG"), None);
        assert_eq!(normalize("ABCD-EFGHJ"), None);
        assert_eq!(normalize("ABCU-EFGH"), None, "U is not in the alphabet");
        assert_eq!(normalize("ABCÇ-EFGH"), None);
    }
}
