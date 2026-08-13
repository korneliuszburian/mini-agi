//! Append-only entry store — port of `PoC` `memory/canonical` layout:
//!
//! ```text
//! memory/canonical/entries/<YYYY-MM-DD>/<YYYY-MM-DD>-NNN.md
//! ```
//!
//! Entry numbering is per-day and monotonic (max existing + 1). Written
//! entries are never modified (append-only contract, ADR-0002).

use std::path::{Path, PathBuf};

/// Relative path from repo root to the canonical entries directory.
pub const ENTRIES_REL: &str = "memory/canonical/entries";

/// A canonical entry file location plus its date/sequence identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryFile {
    /// Absolute-or-root-relative filesystem path to the entry markdown file.
    pub path: PathBuf,
    /// Entry date, `YYYY-MM-DD` (part of the filename).
    pub date: String,
    /// Per-day sequence number (part of the filename).
    pub seq: u32,
}

/// Compute the next entry file for `today` under `root/memory/canonical/entries`.
///
/// Scans existing `NNN.md` files in the day directory; an empty day starts
/// at sequence 1. The day directory is NOT created.
///
/// The sequence is computed in `u64`: a day dir containing the maximum
/// `u32` sequence must neither panic (debug overflow) nor wrap to a name
/// that already exists (release: the caller's write would overwrite a
/// canonical entry, breaking the append-only contract). The returned
/// path is guaranteed not to exist.
#[must_use]
pub fn next_entry(root: &Path, today: &str) -> EntryFile {
    let day_dir = root.join(ENTRIES_REL).join(today);
    let mut max: u64 = 0;
    if day_dir.is_dir() {
        for dir_entry in std::fs::read_dir(&day_dir).ok().into_iter().flatten() {
            let Ok(de) = dir_entry else { continue };
            let name = de.file_name().to_string_lossy().into_owned();
            if let Some(seq) = name
                .strip_prefix(&format!("{today}-"))
                .and_then(|rest| rest.strip_suffix(".md"))
                .and_then(|s| s.parse::<u64>().ok())
            {
                max = max.max(seq);
            }
        }
    }
    let next = max + 1;
    EntryFile {
        path: day_dir.join(format!("{today}-{next:03}.md")),
        date: today.to_string(),
        seq: u32::try_from(next).unwrap_or(u32::MAX),
    }
}

/// Extract every backtick-delimited 16-hex-char id from text (one per line).
#[must_use]
/// Extract fact ids from a canonical entry — header lines only.
///
/// A body that merely QUOTES another fact's id must not enter the known
/// set (that would make `consolidate` skip / `signoff` refuse a fact
/// whose own id is quoted elsewhere).
pub fn extract_fact_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("## F-") else {
            continue;
        };
        let Some(start) = rest.find('`') else {
            continue;
        };
        let rest = &rest[start + 1..];
        if let Some(end) = rest.find('`') {
            let cand = &rest[..end];
            if cand.len() == 16 && cand.chars().all(|c| c.is_ascii_hexdigit()) {
                ids.push(cand.to_string());
            }
        }
    }
    ids
}

/// A fact header is `## F-<digits>` followed by either a backticked
/// 16-hex id with NOTHING (trimmed) after the closing backtick, or no
/// backtick pair at all (the id-less block contract: its body is
/// dropped, no half-labeled facts leak). A body line that merely QUOTES
/// a header ("## F-007 `id` style headers...") carries trailing content
/// and is BODY, not a header — otherwise the reference truncates the
/// enclosing fact and spawns a phantom fact with the quoted id
/// (EXP-014: real information loss in the memory pipeline).
fn is_fact_header(line: &str) -> bool {
    // Column-0 ONLY: an escaped body (` ## F-000 \`id\``) must NOT be
    // re-read as a header after trim_start (that spawned phantom
    // id-less facts and broke the digest invariant).
    let Some(rest) = line.strip_prefix("## F-") else {
        return false;
    };
    let digits: usize = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let after = &rest[digits..];
    let Some(open) = after.find('`') else {
        // Id-less header: `## F-2 (id lost)` — still a header whose
        // body is dropped (frozen contract).
        return true;
    };
    let close_rest = &after[open + 1..];
    let Some(close) = close_rest.find('`') else {
        return false;
    };
    let cand = &close_rest[..close];
    cand.len() == 16
        && cand.chars().all(|c| c.is_ascii_hexdigit())
        && close_rest[close + 1..].trim().is_empty()
}

/// Parse `(fact_body, fact_id)` pairs from an entry file — blocks headed by
/// `## F-NNN` plus a backticked 16-hex id.
///
/// Bodies are the trimmed joined lines between headers; multi-line bodies are
/// joined with single spaces.
#[must_use]
pub fn parse_canonical_facts(text: &str) -> Vec<(String, String)> {
    let mut facts = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_body: Vec<String> = Vec::new();

    for line in text.lines() {
        let is_fact = is_fact_header(line);
        if is_fact {
            let id = extract_fact_ids(line).into_iter().next();
            if let Some(prev) = current_id.take() {
                let body = current_body.join(" ").trim().to_string();
                facts.push((body, prev));
            }
            current_id = id;
            current_body.clear();
        } else if current_id.is_some() {
            current_body.push(line.trim().to_string());
        }
    }
    if let Some(prev) = current_id {
        facts.push((current_body.join(" ").trim().to_string(), prev));
    }
    facts
}

#[cfg(test)]
mod store_tests {
    use super::*;

    #[test]
    fn escaped_header_bodies_are_not_headers() {
        // The space-prefixed escape must NOT be re-read as a header after
        // trim_start (that spawned phantom id-less facts).
        assert!(
            !is_fact_header(" ## F-000 `deadbeefdeadbeef`"),
            "escaped body is not a header"
        );
        assert!(
            is_fact_header("## F-000 `deadbeefdeadbeef`"),
            "a real header is"
        );
    }

    #[test]
    fn fact_ids_come_only_from_headers_not_bodies() {
        let text = "# Canonical entry\n\n## F-000 `aabbccddeeff0011`\n\nthis body references `1122334455667788` in prose\n";
        let ids = extract_fact_ids(text);
        assert_eq!(
            ids,
            vec!["aabbccddeeff0011".to_string()],
            "only the header id is known"
        );
    }
}
