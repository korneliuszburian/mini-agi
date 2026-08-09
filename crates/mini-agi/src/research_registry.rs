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

/// Record a question as asked (upsert by slug; existing row keeps its
/// question text but its status resets to `Asked`).
pub fn record_asked(root: &Path, question: &str) -> std::io::Result<RegistryEntry> {
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
}
