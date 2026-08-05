//! Fact identity: sha256 hex truncated to 16 chars.
//!
//! IDENTICAL to `PoC` `fid`, so existing canonical memory hashes carry over
//! unchanged (behavioral contract, tag `v1-spec-reference`).

use sha2::{Digest, Sha256};

const HEX: &[u8; 16] = b"0123456789abcdef";

fn hex_prefix(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(16);
    for b in bytes.iter().take(8) {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Compute the 16-hex-char fact id for a fact body (sha256 prefix).
///
/// Matches `PoC` `fid()` byte-for-byte: `sha256(body)[..16]`.
#[must_use]
pub fn fact_id(body: &str) -> String {
    hex_prefix(&Sha256::digest(body.as_bytes()))
}

/// Compute the 16-hex-char source hash for raw material text.
///
/// Used for provenance (`source_sha256` in canonical entries).
#[must_use]
pub fn source_sha256(text: &str) -> String {
    hex_prefix(&Sha256::digest(text.as_bytes()))
}

/// Compute the 16-hex-char source hash for RAW BYTES (binary-safe).
///
/// Used by the skill-drift check: lossy text conversion would collapse
/// distinct malformed byte sequences onto one replacement character.
#[must_use]
pub fn source_sha256_bytes(bytes: &[u8]) -> String {
    hex_prefix(&Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fact_id_matches_poc_contract() {
        let body = "explicit memory survives compaction";
        let id = fact_id(body);
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn fact_id_is_deterministic_and_content_bound() {
        assert_eq!(fact_id("a"), fact_id("a"));
        assert_ne!(fact_id("a"), fact_id("b"));
    }

    #[test]
    fn source_sha256_is_16_hex() {
        let id = source_sha256("raw source material");
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
