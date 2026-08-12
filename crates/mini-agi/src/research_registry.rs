//! Research question registry (D2 autoresearch wiring): a durable
//! `research/registry.json` mapping every asked question to its lifecycle
//! state. The registry is the dedup + freshness layer — asking the same
//! question twice resolves to the existing findings instead of spawning
//! a second worker run (observed: duplicate research files, stale
//! re-research of already-decided topics).

use crate::research::slugify;
use std::path::Path;

/// Lifecycle states of a research question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionStatus {
    /// The worker was dispatched; findings are being produced.
    Asked,
    /// Findings landed at `research/<slug>.md`; not yet distilled.
    Findings,
    /// Distilled + audited (`dream --source`); verdicts staged.
    Distilled,
    /// Verdicts promoted into canonical memory.
    Promoted,
    /// A decision/ticket was derived from the findings.
    Decided,
}

/// One registry row: a research question and its lifecycle state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryEntry {
    /// The exact question as asked.
    pub question: String,
    /// Slugified file stem (the findings file's stem).
    pub slug: String,
    /// Current lifecycle state.
    pub status: QuestionStatus,
    /// ISO-8601 date of the last state change.
    pub updated: String,
}

/// The registry file: `research/registry.json`.
#[must_use]
pub fn registry_path(root: &Path) -> std::path::PathBuf {
    root.join("research").join("registry.json")
}

/// Load the registry; missing or malformed file = empty registry.
#[must_use]
pub fn load_registry(root: &Path) -> Vec<RegistryEntry> {
    let Ok(text) = std::fs::read_to_string(registry_path(root)) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Find the registry row for a question (dedup by slug).
#[must_use]
pub fn find_entry<'a>(entries: &'a [RegistryEntry], slug: &str) -> Option<&'a RegistryEntry> {
    entries.iter().find(|e| e.slug == slug)
}

/// Save the registry (rewrite whole file; entries sorted by slug).
fn save_registry(root: &Path, entries: &[RegistryEntry]) -> std::io::Result<()> {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.slug.cmp(&b.slug));
    let json = serde_json::to_string_pretty(&sorted)?;
    std::fs::write(registry_path(root), format!("{json}\n"))
}

/// Before the registry is rewritten, a malformed existing file must be
/// preserved aside — the registry is the dedup layer, and silent
/// overwrite would destroy every recorded question (a duplicate
/// research wave follows). The corrupt bytes survive as
/// `registry.json.corrupt-<stamp>`; a missing or empty file is nothing
/// to preserve.
fn preserve_corrupt_registry(root: &Path) {
    let path = registry_path(root);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    if text.trim().is_empty() || serde_json::from_str::<Vec<RegistryEntry>>(&text).is_ok() {
        return;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let backup = path.with_file_name(format!("registry.json.corrupt-{stamp}"));
    let _ = std::fs::rename(&path, &backup);
}

/// Record a question as asked (upsert by slug; the row is re-worded to
/// the latest question text and its status resets to `Asked`, while its
/// `updated` date keeps the FIRST ask — freshness bumps happen at
/// `advance_status`).
pub fn record_asked(root: &Path, question: &str) -> std::io::Result<RegistryEntry> {
    preserve_corrupt_registry(root);
    let slug = slugify(question);
    let mut entries = load_registry(root);
    let entry = match find_entry(&entries, &slug) {
        Some(existing) => {
            let updated = existing.updated.clone();
            RegistryEntry {
                question: question.to_string(),
                slug,
                status: QuestionStatus::Asked,
                updated,
            }
        }
        None => RegistryEntry {
            question: question.to_string(),
            slug,
            status: QuestionStatus::Asked,
            updated: today_iso(),
        },
    };
    // Replace the row (dedup) and save.
    entries.retain(|e| e.slug != entry.slug);
    entries.push(entry.clone());
    save_registry(root, &entries)?;
    Ok(entry)
}

/// Advance a question's lifecycle state (idempotent upsert; `updated`
/// changes only when the status actually changes).
pub fn advance_status(root: &Path, slug: &str, status: QuestionStatus) -> std::io::Result<()> {
    preserve_corrupt_registry(root);
    let mut entries = load_registry(root);
    match find_entry(&entries, slug) {
        Some(existing) if existing.status == status => Ok(()),
        Some(existing) => {
            let row = RegistryEntry {
                question: existing.question.clone(),
                slug: slug.to_string(),
                status,
                updated: today_iso(),
            };
            entries.retain(|e| e.slug != slug);
            entries.push(row);
            save_registry(root, &entries)
        }
        None => {
            entries.push(RegistryEntry {
                question: slug.to_string(),
                slug: slug.to_string(),
                status,
                updated: today_iso(),
            });
            save_registry(root, &entries)
        }
    }
}

/// Today's date in ISO-8601 (`YYYY-MM-DD`), UTC.
fn today_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = secs / 86_400;
    // Civil-from-days (Howard Hinnant's algorithm); days is non-negative
    // so the era branch is trivial but kept for correctness.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "mag-research-registry-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn registry_roundtrips_and_dedups() {
        let root = tmpdir("roundtrip");
        std::fs::create_dir_all(root.join("research")).unwrap();
        assert!(load_registry(&root).is_empty(), "missing file = empty");
        let e1 = record_asked(&root, "What is X?").unwrap();
        assert_eq!(e1.slug, "what-is-x");
        assert_eq!(
            find_entry(&load_registry(&root), "what-is-x")
                .unwrap()
                .status,
            QuestionStatus::Asked
        );
        // Same question again: still one row, same slug.
        let e2 = record_asked(&root, "What is X?").unwrap();
        assert_eq!(e1.slug, e2.slug);
        assert_eq!(load_registry(&root).len(), 1, "dedup keeps one row");
        // Advance lifecycle.
        advance_status(&root, "what-is-x", QuestionStatus::Promoted).unwrap();
        let rows = load_registry(&root);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, QuestionStatus::Promoted);
        // Idempotent advance keeps the status (no panic).
        advance_status(&root, "what-is-x", QuestionStatus::Promoted).unwrap();
        assert_eq!(load_registry(&root)[0].status, QuestionStatus::Promoted);
    }

    #[test]
    fn today_iso_is_dated() {
        let s = today_iso();
        assert_eq!(s.len(), 10);
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert!(s.parse::<u64>().is_ok() || s.chars().all(|c| c.is_ascii_digit() || c == '-'));
    }

    #[test]
    fn reask_with_different_wording_upserts_and_keeps_first_ask_date() {
        let root = tmpdir("reask");
        std::fs::create_dir_all(root.join("research")).unwrap();
        let first = record_asked(&root, "What is X?").unwrap();
        let second = record_asked(&root, "What is X!!!").unwrap();
        let rows = load_registry(&root);
        assert_eq!(rows.len(), 1, "same slug dedups to one row");
        assert_eq!(rows[0].question, "What is X!!!", "latest wording wins");
        assert_eq!(second.slug, "what-is-x");
        assert_eq!(
            rows[0].updated, first.updated,
            "re-ask keeps the first-ask date"
        );
        assert_eq!(
            rows[0].status,
            QuestionStatus::Asked,
            "status resets to Asked"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn advance_status_creates_row_for_unknown_slug() {
        let root = tmpdir("unknown");
        std::fs::create_dir_all(root.join("research")).unwrap();
        advance_status(&root, "never-asked", QuestionStatus::Findings).unwrap();
        let rows = load_registry(&root);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].slug, "never-asked");
        assert_eq!(
            rows[0].question, "never-asked",
            "slug doubles as question text"
        );
        assert_eq!(rows[0].status, QuestionStatus::Findings);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_registry_file_loads_as_empty() {
        let root = tmpdir("malformed");
        std::fs::create_dir_all(root.join("research")).unwrap();
        std::fs::write(root.join("research/registry.json"), "{not json!!").unwrap();
        assert!(
            load_registry(&root).is_empty(),
            "malformed = empty, never panic"
        );
        // A corrupt registry must not block a fresh ask (rewrite path).
        record_asked(&root, "What is Y?").unwrap();
        assert_eq!(load_registry(&root).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_registry_is_preserved_aside_never_destroyed() {
        // The registry is the dedup layer: silently overwriting a
        // malformed file destroys every recorded question (and the
        // next ask spawns a duplicate research wave). Corruption must
        // preserve the original aside, like install backups.
        let root = tmpdir("corrupt-preserve");
        std::fs::create_dir_all(root.join("research")).unwrap();
        std::fs::write(root.join("research/registry.json"), "{not json!!").unwrap();
        record_asked(&root, "What is Y?").unwrap();
        // The registry is usable again (malformed = empty, never panic).
        assert_eq!(load_registry(&root).len(), 1);
        // ...but the corrupt original was preserved, not destroyed.
        let preserved: Vec<_> = std::fs::read_dir(root.join("research"))
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("registry.json.corrupt-")
            })
            .collect();
        assert!(
            !preserved.is_empty(),
            "corrupt registry must be preserved aside"
        );
        let kept = std::fs::read_to_string(preserved[0].path()).unwrap();
        assert!(kept.contains("{not json!!"), "original corrupt bytes kept");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn saved_registry_is_sorted_by_slug() {
        let root = tmpdir("sorted");
        std::fs::create_dir_all(root.join("research")).unwrap();
        record_asked(&root, "Zebra question?").unwrap();
        record_asked(&root, "Apple question?").unwrap();
        let text = std::fs::read_to_string(root.join("research/registry.json")).unwrap();
        let apples = text.find("\"apple-question\"").unwrap();
        let zebras = text.find("\"zebra-question\"").unwrap();
        assert!(apples < zebras, "rows persist sorted by slug");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reask_after_promotion_resets_status_keeps_first_date() {
        let root = tmpdir("reask2");
        std::fs::create_dir_all(root.join("research")).unwrap();
        let first = record_asked(&root, "What is X?").unwrap();
        advance_status(&root, "what-is-x", QuestionStatus::Promoted).unwrap();
        assert_eq!(load_registry(&root)[0].status, QuestionStatus::Promoted);
        // Asking the same question again AFTER promotion: lifecycle
        // resets to Asked (the question is live again), but the
        // first-ask date survives — freshness is not rewritten.
        record_asked(&root, "What is X?").unwrap();
        let rows = load_registry(&root);
        assert_eq!(rows.len(), 1, "still one row");
        assert_eq!(rows[0].status, QuestionStatus::Asked);
        assert_eq!(
            rows[0].updated, first.updated,
            "re-ask keeps the first-ask date even after promotion"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
