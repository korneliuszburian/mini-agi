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
pub fn extract_fact_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        if let Some(start) = line.find('`') {
            let rest = &line[start + 1..];
            if let Some(end) = rest.find('`') {
                let cand = &rest[..end];
                if cand.len() == 16 && cand.chars().all(|c| c.is_ascii_hexdigit()) {
                    ids.push(cand.to_string());
                }
            }
        }
    }
    ids
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
        let is_fact = line.trim_start().starts_with("## F-");
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
mod tests {
    use super::*;

    #[test]
    fn next_entry_sequence_is_per_day_and_incremental() {
        let tmp = std::env::temp_dir().join(format!("mag-store-test-{}", std::process::id()));
        let day = "2026-08-02";
        let e1 = next_entry(&tmp, day);
        assert_eq!(
            e1.path.file_name().unwrap(),
            format!("{day}-001.md").as_str()
        );
        std::fs::create_dir_all(e1.path.parent().unwrap()).unwrap();
        std::fs::write(&e1.path, "# x\n").unwrap();
        let e2 = next_entry(&tmp, day);
        assert_eq!(e2.seq, 2);
        let other = next_entry(&tmp, "2026-07-31");
        assert_eq!(other.seq, 1, "different day restarts at 1");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_fact_ids_parses_backtick_ids() {
        let text = "## F-001 `0123456789abcdef`\n\nbody\n";
        assert_eq!(extract_fact_ids(text), vec!["0123456789abcdef"]);
    }

    #[test]
    fn parse_canonical_facts_recovers_bodies() {
        let text = "# entry\n\n## F-001 `1111111111111111`\n\nfirst fact\n\n## F-002 `2222222222222222`\n\nsecond fact\n";
        let facts = parse_canonical_facts(text);
        assert_eq!(facts.len(), 2);
        assert_eq!(
            facts[0],
            ("first fact".to_string(), "1111111111111111".to_string())
        );
        assert_eq!(
            facts[1],
            ("second fact".to_string(), "2222222222222222".to_string())
        );
    }

    #[test]
    fn next_entry_ignores_non_sequence_files_and_handles_gaps() {
        let tmp = std::env::temp_dir().join(format!("mag-store2-{}", std::process::id()));
        let day = "2026-08-02";
        let day_dir = tmp.join("memory/canonical/entries").join(day);
        std::fs::create_dir_all(&day_dir).unwrap();
        // Gap + junk files: only NNN.md names count, the sequence is
        // max+1, so a deleted middle file does not get reused.
        std::fs::write(day_dir.join(format!("{day}-001.md")), "a").unwrap();
        std::fs::write(day_dir.join(format!("{day}-003.md")), "c").unwrap();
        std::fs::write(day_dir.join(format!("{day}-00X.md")), "junk").unwrap();
        std::fs::write(day_dir.join("001.md"), "wrong prefix").unwrap();
        std::fs::write(day_dir.join("notes.md"), "junk").unwrap();
        std::fs::write(day_dir.join("2026-07-31-009.md"), "other day").unwrap();
        let e = next_entry(&tmp, day);
        assert_eq!(e.seq, 4, "gap at 002 is not reused, junk is ignored");
        assert_eq!(
            e.path.file_name().unwrap(),
            format!("{day}-004.md").as_str()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_fact_ids_only_accepts_first_16hex_per_line() {
        let text = "line with `0123456789abcdef` and `2222222222222222`\n\
                    `short` not an id\n\
                    `gggggggggggggggg` not hex\n\
                    no backticks here\n";
        assert_eq!(
            extract_fact_ids(text),
            vec!["0123456789abcdef"],
            "only the first well-formed id per line counts"
        );
    }

    #[test]
    fn parse_canonical_facts_skips_blocks_without_id() {
        // A header without a backticked 16-hex id is not a fact block:
        // its body is dropped entirely — no half-labeled facts leak
        // into canonical.
        let text = "## F-1 `0123456789abcdef`\n\nlabeled body\n\n\
                    ## F-2 (id lost)\n\nunlabeled body\n\n\
                    ## F-3 `ffffffffffffffff`\n\nsecond body\n";
        let facts = parse_canonical_facts(text);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].1, "0123456789abcdef");
        assert_eq!(facts[0].0, "labeled body");
        assert_eq!(facts[1].1, "ffffffffffffffff");
        assert_eq!(facts[1].0, "second body");
    }

    #[test]
    fn next_entry_never_returns_an_existing_path_on_sequence_overflow() {
        // Append-only (ADR-0002): next_entry must never hand out a path
        // that already exists. A day dir containing the maximum u32
        // sequence used to wrap to 0 (debug build: panic; release: the
        // 000 file returned again — the caller's write would overwrite
        // a canonical entry). The sequence must stay monotonic in u64.
        let tmp = std::env::temp_dir().join(format!("mag-store-of-{}", std::process::id()));
        let day = "2026-08-02";
        let day_dir = tmp.join("memory/canonical/entries").join(day);
        std::fs::create_dir_all(&day_dir).unwrap();
        std::fs::write(day_dir.join(format!("{day}-4294967295.md")), "max").unwrap();
        std::fs::write(day_dir.join(format!("{day}-000.md")), "zero").unwrap();
        let e = next_entry(&tmp, day);
        assert!(
            !e.path.exists(),
            "next_entry returned an existing path {} — append-only would be violated",
            e.path.display()
        );
        assert!(e.seq > 0, "sequence must stay monotonic, got {}", e.seq);
        // Once the returned entry is written, the next call advances
        // (the name is never reused).
        std::fs::write(&e.path, "# written\n").unwrap();
        let e2 = next_entry(&tmp, day);
        assert_ne!(e2.path, e.path, "the next entry must advance the name");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
