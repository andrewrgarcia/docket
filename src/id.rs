//! Card identifiers.
//!
//! An id is eight hex characters, generated once when the card is created and
//! then never touched. It is *not* a hash of the card's contents: a content
//! hash changes every time you edit, which makes it useless as a name. What it
//! hashes is the moment of creation, so it stays put for the life of the card
//! while the name, status and text all change around it.
//!
//! Eight characters is 4.3 billion values. Cards number in the dozens, so a
//! collision is a curiosity rather than a risk — and `generate` checks against
//! the ids already in the store anyway.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub const LENGTH: usize = 8;

/// FNV-1a, 64-bit. Chosen because it is six lines and needs no dependency;
/// nothing here is security-sensitive.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A fresh id, distinct from every id in `taken`.
pub fn generate(seed: &str, taken: &BTreeSet<String>) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    for salt in 0..1_000u64 {
        let material = format!("{seed}:{nanos}:{}:{salt}", std::process::id());
        let id = format(fnv1a(material.as_bytes()));
        if !taken.contains(&id) {
            return id;
        }
    }
    // A thousand collisions in a row means the hash is broken, not unlucky.
    format(fnv1a(format!("{seed}:{nanos}:fallback").as_bytes()))
}

/// A fixed id for a piece of text: the same text always gives the same id.
/// For things identified by name rather than created with an id, like books.
pub fn of_text(text: &str) -> String {
    format(fnv1a(text.as_bytes()))
}

fn format(hash: u64) -> String {
    format!("{hash:016x}")[..LENGTH].to_string()
}

/// Whether a string could be an id, or the start of one. Used to tell an id
/// prefix from a name prefix when resolving what the user typed.
pub fn looks_like(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= LENGTH
        && text.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_eight_lowercase_hex_characters() {
        let id = generate("moxi", &BTreeSet::new());
        assert_eq!(id.len(), LENGTH);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn a_taken_id_is_never_handed_out_twice() {
        let mut taken = BTreeSet::new();
        let first = generate("moxi", &taken);
        taken.insert(first.clone());
        assert_ne!(generate("moxi", &taken), first);
    }

    #[test]
    fn the_same_name_gets_different_ids() {
        let a = generate("moxi", &BTreeSet::new());
        let b = generate("moxi", &BTreeSet::new());
        // Different nanosecond, different id. Identity is per card, not per name.
        assert_ne!(a, b);
    }

    #[test]
    fn recognises_id_shaped_strings() {
        assert!(looks_like("a4"));
        assert!(looks_like("a43b21c0"));
        assert!(!looks_like(""));
        assert!(!looks_like("a43b21c0f"), "too long");
        assert!(!looks_like("moxi"), "x is not hex");
        assert!(!looks_like("A43B"), "ids are lowercase");
    }

    #[test]
    fn a_hex_shaped_name_is_still_id_shaped() {
        // "abed" is a real word and valid hex; resolution tries names first,
        // so this ambiguity is resolved by order, not by this function.
        assert!(looks_like("abed"));
    }
}
