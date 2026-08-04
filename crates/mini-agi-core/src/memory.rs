//! Memory engine — port of `PoC` `scripts/consolidate.py` + `scripts/derive.py`.
//!
//! Consolidation: episodic buffer -> canonical facts (append-only, deduped,
//! contested-wording queue + signoff). Derivation: canonical -> context brief,
//! per-domain fragments, `CLAUDE.md` shim (regenerated, never hand-edited).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::hash::{fact_id, source_sha256};
use crate::store::{EntryFile, parse_canonical_facts};

/// Relative path of the canonical memory directory.
pub const CANONICAL_REL: &str = "memory/canonical";
/// Relative path of the canonical entries directory.
pub const ENTRIES_REL: &str = "memory/canonical/entries";
/// Relative path of the derived views directory.
pub const DERIVED_REL: &str = "memory/derived";
/// Relative path of the per-domain fragments directory.
pub const PER_DOMAIN_REL: &str = "memory/derived/per-domain";
/// Relative path of the review (contested) queue directory.
pub const REVIEW_REL: &str = "memory/review";
/// Named derive snapshots (production-readiness F.1).
pub const SNAPSHOTS_REL: &str = "memory/derived/snapshots";

/// Derived brief size cap in bytes (context budget; `PoC`: 8192).
pub const MAX_BRIEF_BYTES: usize = 8192;

/// Memory engine errors (deterministic, exit-code mapped by the CLI).
#[derive(Debug, Error)]
pub enum MemoryError {
    /// The episodic buffer contained no fact candidates.
    #[error("no facts found in episodic buffer")]
    NoFacts,
    /// Filesystem operation failed.
    #[error("entry write failed: {0}")]
    Io(#[from] std::io::Error),
    /// The fact being signed off already exists in canonical memory.
    #[error("fact already known")]
    FactKnown,
    /// The contested queue has no fact at the requested index.
    #[error("contested fact index not found")]
    IndexNotFound,
    /// A named derive snapshot does not exist.
    #[error("derive snapshot not found: {0}")]
    SnapshotMissing(String),
    /// Signoff was called with a missing queue or a non-positive index.
    #[error("signoff requires an existing queue file and positive fact index")]
    BadSignoff,
    /// Derivation ran before any canonical facts existed.
    #[error("no canonical facts yet — run ingest first")]
    NoCanonical,
}

/// Options for a consolidation run.
#[derive(Debug, Clone, Default)]
pub struct ConsolidateOptions {
    /// Domain assigned to new facts.
    pub domain: String,
    /// Route wording-variants of known facts to the review queue instead of canonical.
    pub require_signoff: bool,
    /// Report without writing anything (no directories created).
    pub dry_run: bool,
}

/// Outcome of a consolidation run (mirrors `PoC` stdout semantics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsolidateOutcome {
    /// Facts written (or planned) to canonical.
    pub new_facts: usize,
    /// Facts skipped as duplicates or queued.
    pub skipped: usize,
    /// Entry written (None for dry-run / empty consolidation).
    pub entry: Option<EntryFile>,
}

/// Extract fact candidates from an episodic buffer: `FACT:` lines and
/// bullets (`- ` / `* `) with payload of at least 8 chars.
///
/// Mirrors `PoC` `extract_candidates`.
#[must_use]
pub fn extract_candidates(text: &str) -> Vec<String> {
    let mut facts = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.to_ascii_lowercase().starts_with("fact:") {
            let payload = trimmed["fact:".len()..].trim();
            if !payload.is_empty() {
                facts.push(payload.to_string());
            }
        } else if let Some(payload) = trimmed
            .strip_prefix('-')
            .or_else(|| trimmed.strip_prefix('*'))
        {
            let payload = payload.trim();
            if payload.len() >= 8 {
                facts.push(payload.to_string());
            }
        }
    }
    facts
}

/// All canonical entry files under `root/memory/canonical/entries` (sorted).
#[must_use]
pub fn canonical_entries(root: &Path) -> Vec<PathBuf> {
    let entries_root = root.join(ENTRIES_REL);
    let mut out = Vec::new();
    let Ok(days) = fs::read_dir(&entries_root) else {
        return out;
    };
    for day in days.flatten() {
        let Ok(meta) = day.file_type() else { continue };
        if !meta.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(day.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "md") {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    out
}

/// `(fact_id, domain, body)` triples from canonical entries (`PoC` `read_facts`).
///
/// Body is whitespace-flattened exactly like `" ".join(m.group(2).split())`.
#[must_use]
pub fn read_facts(root: &Path) -> Vec<(String, String, String)> {
    let mut facts = Vec::new();
    for entry in canonical_entries(root) {
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        let mut domain = "general".to_string();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("- domain:") {
                domain = rest.trim().to_string();
                break;
            }
        }
        for (body, id) in parse_canonical_facts(&text) {
            let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
            facts.push((id, domain.clone(), flat));
        }
    }
    facts
}

/// `(fact_body, fact_id)` pairs from all canonical entries.
///
/// Mirrors `PoC` `parsed_canonical_facts`.
#[must_use]
pub fn canonical_facts(root: &Path) -> Vec<(String, String)> {
    let mut facts = Vec::new();
    for entry in canonical_entries(root) {
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        facts.extend(parse_canonical_facts(&text));
    }
    facts
}

/// All known 16-hex fact ids across canonical entries.
#[must_use]
pub fn existing_fact_ids(root: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    for entry in canonical_entries(root) {
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        ids.extend(crate::store::extract_fact_ids(&text));
    }
    ids
}

/// Domain/keyword retrieval over canonical facts (hardening audit C.7).
///
/// `domain` filters on the entry's declared domain (exact);
/// `keyword` filters on the flattened fact body (case-insensitive
/// substring). At least one filter must be given. Returns
/// `(fact_id, domain, flat_body)` triples.
#[must_use]
pub fn query_facts(
    root: &Path,
    domain: Option<&str>,
    keyword: Option<&str>,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (id, fact_domain, body) in read_facts(root) {
        let domain_matches = domain.is_none_or(|d| fact_domain == d);
        let keyword_matches =
            keyword.is_none_or(|k| body.to_lowercase().contains(&k.to_lowercase()));
        if domain_matches && keyword_matches {
            out.push((id, fact_domain, body));
        }
    }
    out
}

/// UTC now formatted `YYYY-MM-DDTHH:MM:SSZ` (`PoC` strftime contract).
#[must_use]
pub fn utc_now_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d, hh, mm, ss) = civil_from_unix(secs);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// UTC now formatted `YYYY-MM-DD`.
#[must_use]
pub fn utc_now_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d, _, _, _) = civil_from_unix(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Civil calendar from unix seconds (Howard Hinnant's algorithm).
fn civil_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400).cast_signed();
    let rem = secs % 86_400;
    let hh = u32::try_from(rem / 3600).unwrap_or(0);
    let mm = u32::try_from((rem % 3600) / 60).unwrap_or(0);
    let ss = u32::try_from(rem % 60).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(0);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(0);
    (if m <= 2 { y + 1 } else { y }, m, d, hh, mm, ss)
}

/// Write a canonical entry with `## F-NNN` blocks (`PoC` `write_canonical_entry`).
///
/// Returns the written entry file; parent directories are created.
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when a directory or file cannot be created.
pub fn write_canonical_entry(
    root: &Path,
    facts: &[(String, String)],
    source: &str,
    domain: &str,
    kind: &str,
) -> Result<EntryFile, MemoryError> {
    let entry = crate::store::next_entry(root, &utc_now_date());
    if let Some(parent) = entry.path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stamp = utc_now_stamp();
    let stem = entry
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("entry");
    let mut content = format!("# Canonical entry {stem} (consolidated from {source})\n\n");
    let _ = writeln!(
        content,
        "- date: {stamp}\n- source: {source}\n- domain: {domain}\n- kind: {kind}"
    );
    for (i, (fact, digest)) in facts.iter().enumerate() {
        let _ = writeln!(content, "\n## F-{i:03} `{digest}`\n\n{fact}");
    }
    fs::write(&entry.path, content)?;
    Ok(entry)
}

/// Append one contested fact to the review queue (`PoC` `append_contested`).
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the queue cannot be opened or written.
pub fn append_contested(
    root: &Path,
    fact: &str,
    digest: &str,
    source: &str,
    existing_hash: &str,
) -> Result<PathBuf, MemoryError> {
    let queue = root
        .join(REVIEW_REL)
        .join(format!("contested-{}.md", utc_now_date()));
    if let Some(parent) = queue.parent() {
        fs::create_dir_all(parent)?;
    }
    let current = fs::read_to_string(&queue).unwrap_or_default();
    let number = current.lines().filter(|l| l.starts_with("## C-")).count() + 1;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&queue)?;
    writeln!(
        f,
        "## C-{number:03} `{digest}`\n- source: {source}\n- reason: same first 40 chars\n- existing fact hash: {existing_hash}\n\n{fact}\n"
    )?;
    Ok(queue)
}

/// Parse `(digest, payload)` pairs from a contested queue file (`PoC` `queued_facts`).
#[must_use]
pub fn queued_facts(queue: &Path) -> Vec<(String, String)> {
    let Ok(lines) = fs::read_to_string(queue) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    let lines: Vec<&str> = lines.lines().collect();
    for (pos, line) in lines.iter().enumerate() {
        let Some(id) = line
            .strip_prefix("## C-")
            .and_then(|rest| rest.split('`').nth(1))
            .filter(|id| id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit()))
        else {
            continue;
        };
        let payload = lines[pos + 1..]
            .iter()
            .find(|item| !item.is_empty() && !item.starts_with("- "))
            .copied()
            .unwrap_or("")
            .trim()
            .to_string();
        records.push((id.to_string(), payload));
    }
    records
}

/// Consolidate an episodic buffer into canonical facts.
///
/// Mirrors `PoC` `main()`: dedup by fact id, contested-wording routing when
/// `require_signoff`, dry-run reporting without any writes.
///
/// # Errors
///
/// Returns [`MemoryError::NoFacts`] for an empty buffer and
/// [`MemoryError::Io`] for any filesystem failure.
pub fn consolidate(
    root: &Path,
    buffer_text: &str,
    source: &str,
    opts: &ConsolidateOptions,
) -> Result<ConsolidateOutcome, MemoryError> {
    let candidates = extract_candidates(buffer_text);
    if candidates.is_empty() {
        return Err(MemoryError::NoFacts);
    }

    let mut known: Vec<String> = existing_fact_ids(root);
    let canonical = canonical_facts(root);

    let mut new_facts: Vec<(String, String)> = Vec::new();
    let mut skipped = 0usize;

    for fact in candidates {
        let h = fact_id(&fact);
        if known.contains(&h) {
            skipped += 1;
            continue;
        }
        let contested = canonical
            .iter()
            .find(|(old_fact, _)| {
                old_fact.starts_with(&fact[..fact.len().min(40)]) && *old_fact != fact
            })
            .map(|(_, old_hash)| old_hash.clone());
        if opts.require_signoff
            && let Some(old_hash) = contested
        {
            if !opts.dry_run {
                append_contested(root, &fact, &h, source, &old_hash)?;
            }
            skipped += 1;
            continue;
        }
        known.push(h.clone());
        new_facts.push((fact, h));
    }

    if opts.dry_run {
        let entry = crate::store::next_entry(root, &utc_now_date());
        return Ok(ConsolidateOutcome {
            new_facts: new_facts.len(),
            skipped,
            entry: Some(entry),
        });
    }
    if new_facts.is_empty() {
        return Ok(ConsolidateOutcome {
            new_facts: 0,
            skipped,
            entry: None,
        });
    }
    let entry = write_canonical_entry(root, &new_facts, source, &opts.domain, "consolidation")?;
    Ok(ConsolidateOutcome {
        new_facts: new_facts.len(),
        skipped,
        entry: Some(entry),
    })
}

/// Promote ONE contested fact from the queue into canonical (`PoC` `--signoff`).
///
/// # Errors
///
/// Returns [`MemoryError::BadSignoff`] when the queue is missing or the index
/// is not positive, [`MemoryError::IndexNotFound`] for an out-of-range index,
/// and [`MemoryError::FactKnown`] when the fact was already promoted.
pub fn signoff(
    root: &Path,
    queue: &Path,
    index: usize,
    domain: &str,
) -> Result<EntryFile, MemoryError> {
    if !queue.exists() || index < 1 {
        return Err(MemoryError::BadSignoff);
    }
    let blocks = queued_facts(queue);
    let Some((digest, fact)) = blocks.get(index - 1).cloned() else {
        return Err(MemoryError::IndexNotFound);
    };
    if existing_fact_ids(root).contains(&digest) {
        return Err(MemoryError::FactKnown);
    }
    let source = queue
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("queue")
        .to_string();
    write_canonical_entry(root, &[(fact, digest)], &source, domain, "signoff")
}

/// Provenance header for every derived artifact (`PoC` `provenance_block`).
#[must_use]
pub fn provenance_block(root: &Path) -> String {
    let entries = canonical_entries(root);
    let joined = entries
        .iter()
        .map(|p| fs::read_to_string(p).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# PROVENANCE\n# canonical_sha256: {}\n# canonical_entries: {}\n# derived_at: regenerated deterministically by mini-agi derive\n# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive\n\n",
        source_sha256(&joined),
        entries.len()
    )
}

/// Render the derived context brief (`PoC` `render_brief`).
#[must_use]
/// Keyword index of a fact body: words of 5+ chars, lowercased
/// (fact-linking pass, Phase 8 slice 6 — derived views only).
fn keywords(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.len() >= 5)
        .collect()
}

/// Cross-fact links (Phase 8 slice 6): facts sharing >= 2 keywords.
/// Deterministic, computed in DERIVED views only — canonical facts stay
/// append-only (A-MEM 2502.12110, supersede-never semantics).
#[must_use]
pub fn fact_links(
    facts: &[(String, String, String)],
) -> std::collections::BTreeMap<String, Vec<String>> {
    let index: Vec<(String, std::collections::HashSet<String>)> = facts
        .iter()
        .map(|(fid, _, text)| (fid.clone(), keywords(text)))
        .collect();
    let mut links: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for i in 0..index.len() {
        for j in (i + 1)..index.len() {
            let shared = index[i].1.intersection(&index[j].1).count();
            if shared >= 2 {
                links
                    .entry(index[i].0.clone())
                    .or_default()
                    .push(index[j].0.clone());
                links
                    .entry(index[j].0.clone())
                    .or_default()
                    .push(index[i].0.clone());
            }
        }
    }
    links
}

/// Render the context brief with importance-ordered facts + links
/// (Phase 8 slice 6). Linked facts come first — importance learned
/// from cross-referencing, not hand-assigned.
#[must_use]
pub fn render_brief(root: &Path, facts: &[(String, String, String)]) -> String {
    let links = fact_links(facts);
    let mut ordered: Vec<(String, String, String, usize)> = facts
        .iter()
        .map(|(fid, domain, text)| {
            let importance = links.get(fid).map_or(0, Vec::len);
            (fid.clone(), domain.clone(), text.clone(), importance)
        })
        .collect();
    ordered.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));
    let mut out = provenance_block(root);
    out.push_str("# CONTEXT BRIEF (derived)\n\nRead this before starting any session. Canonical wins over this file.\n\n");
    for (fid, domain, text, importance) in &ordered {
        let _ = writeln!(out, "- `{fid}` [{domain}] {text}");
        if *importance > 0 {
            let _ = writeln!(out, "  links: {}", links[fid].join(", "));
        }
    }
    out
}

/// Render per-domain `AGENTS.md` fragments (`PoC` `render_domain_agents`).
#[must_use]
pub fn render_domain_agents(
    root: &Path,
    facts: &[(String, String, String)],
) -> BTreeMap<String, String> {
    let mut domains: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (fid, domain, text) in facts {
        domains
            .entry(domain.clone())
            .or_default()
            .push(format!("- `{fid}` {text}"));
    }
    domains
        .into_iter()
        .map(|(domain, lines)| {
            let name: String = domain
                .to_ascii_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .trim_matches('-')
                .to_string();
            let mut content = provenance_block(root);
            let _ = writeln!(
                content,
                "# Domain: {domain} (derived from canonical memory)\n\nApplies when working on this domain. Canonical memory wins on conflict."
            );
            content.push_str(&lines.join("\n"));
            content.push('\n');
            (name, content)
        })
        .collect()
}

/// Render the `CLAUDE.md` import shim (`PoC` `render_claude_shim`, ADR-0009).
#[must_use]
pub fn render_claude_shim(root: &Path) -> String {
    let mut out = provenance_block(root);
    out.push_str(
        "# CLAUDE.md — generated import-shim (do not hand-edit; mini-agi derive)\n\n\
         This repo's canonical agent instructions live in AGENTS.md.\n\
         Context brief: memory/derived/context-brief.md (regenerated by derive).\n\
         Deterministic gates: `scripts/verify.sh` (fmt, clippy, tests,\n\
         eval gate, checkpoint, provenance, stats, budget).\n",
    );
    out
}

/// Regenerate all derived views from canonical memory (`PoC` `main`).
///
/// Returns `(fact_count, fragment_count)`.
///
/// # Errors
///
/// Returns [`MemoryError::NoCanonical`] when no facts exist and
/// [`MemoryError::Io`] for filesystem failures.
pub fn derive(root: &Path, brief_only: bool) -> Result<(usize, usize), MemoryError> {
    let facts = read_facts(root);
    if facts.is_empty() {
        return Err(MemoryError::NoCanonical);
    }
    fs::create_dir_all(root.join(DERIVED_REL))?;
    fs::write(
        root.join(DERIVED_REL).join("context-brief.md"),
        render_brief(root, &facts),
    )?;
    fs::write(root.join("CLAUDE.md"), render_claude_shim(root))?;

    let fragments = if brief_only {
        BTreeMap::new()
    } else {
        let fragments = render_domain_agents(root, &facts);
        let per_domain = root.join(PER_DOMAIN_REL);
        fs::create_dir_all(&per_domain)?;
        for (name, content) in &fragments {
            fs::write(per_domain.join(format!("AGENTS.{name}.md")), content)?;
        }
        fragments
    };
    Ok((facts.len(), fragments.len()))
}

/// Snapshot the derived views (production-readiness F.1).
///
/// Records the canonical fingerprint + the brief hash under a named
/// file, so a later `replay` can prove the views are a deterministic
/// materialization.
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the snapshot cannot be written.
pub fn snapshot(root: &Path, name: &str) -> Result<String, MemoryError> {
    let fingerprint = canonical_fingerprint(root);
    let brief =
        fs::read_to_string(root.join(DERIVED_REL).join("context-brief.md")).unwrap_or_default();
    let doc = serde_json::json!({
        "name": name,
        "at": utc_now_stamp(),
        "canonical_sha256": fingerprint,
        "brief_sha256": source_sha256(&brief),
    });
    let dir = root.join(SNAPSHOTS_REL);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_string_pretty(&doc).unwrap_or_default(),
    )?;
    Ok(format!("snapshot {name}: canonical {fingerprint}"))
}

/// Replay a named snapshot (production-readiness F.1).
///
/// Regenerates the derived views deterministically, then verifies the
/// canonical fingerprint and the brief hash match the snapshot — the
/// deterministic materialization proof.
///
/// # Errors
///
/// Returns [`MemoryError::SnapshotMissing`] when the named snapshot does
/// not exist, or [`MemoryError::NoCanonical`] when there are no facts.
pub fn replay(root: &Path, name: &str) -> Result<String, MemoryError> {
    let path = root.join(SNAPSHOTS_REL).join(format!("{name}.json"));
    let text = fs::read_to_string(&path).map_err(|_| MemoryError::SnapshotMissing(name.into()))?;
    let doc: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| MemoryError::Io(std::io::Error::other(e)))?;
    let snap_canon = doc["canonical_sha256"].as_str().unwrap_or("").to_string();
    let snap_brief = doc["brief_sha256"].as_str().unwrap_or("").to_string();
    derive(root, false)?;
    let now_canon = canonical_fingerprint(root);
    let brief =
        fs::read_to_string(root.join(DERIVED_REL).join("context-brief.md")).unwrap_or_default();
    let now_brief = source_sha256(&brief);
    let verdict = if now_canon == snap_canon && now_brief == snap_brief {
        "MATCH — derived views are a deterministic materialization of the snapshot".to_string()
    } else if now_canon != snap_canon {
        format!("DIVERGENT — canonical changed since the snapshot ({snap_canon} != {now_canon})")
    } else {
        "DIVERGENT — derived views changed since the snapshot (brief hash differs)".to_string()
    };
    Ok(format!("replay {name}: {verdict}"))
}

/// Current canonical fingerprint for the provenance gate (`PoC` `sha256(joined)`).
#[must_use]
pub fn canonical_fingerprint(root: &Path) -> String {
    let entries = canonical_entries(root);
    let joined = entries
        .iter()
        .map(|p| fs::read_to_string(p).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    source_sha256(&joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_stamp_matches_poc_format() {
        let stamp = utc_now_stamp();
        assert_eq!(stamp.len(), 20);
        assert!(stamp.ends_with('Z'));
        assert!(stamp.contains('T'));
        let date = utc_now_date();
        assert_eq!(date.len(), 10);
        assert!(stamp.starts_with(&date));
    }

    #[test]
    fn query_filters_by_domain_and_keyword() {
        let root = std::env::temp_dir().join(format!("mag-memq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let entry_dir = root.join("memory/canonical/entries").join(utc_now_date());
        std::fs::create_dir_all(&entry_dir).unwrap();
        let text = "# Canonical entry test-001\n\n- domain: eval\n\n## F-000 `1111111111111111`\n\ncomposite 0.9 on the rerun\n\n## F-001 `2222222222222222`\n\nmemory discipline held\n";
        std::fs::write(entry_dir.join("test-001.md"), text).unwrap();
        // Domain filter.
        let eval = query_facts(&root, Some("eval"), None);
        assert_eq!(eval.len(), 2);
        let strategy = query_facts(&root, Some("strategy"), None);
        assert!(strategy.is_empty());
        // Keyword filter (case-insensitive substring).
        let comp = query_facts(&root, None, Some("COMPOSITE"));
        assert_eq!(comp.len(), 1);
        assert_eq!(comp[0].0, "1111111111111111");
        // Domain + keyword together.
        let both = query_facts(&root, Some("eval"), Some("memory"));
        assert_eq!(both.len(), 1);
        // At least one filter: read_facts with none given matches all.
        assert_eq!(query_facts(&root, None, None).len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn snapshot_then_replay_detects_canonical_divergence() {
        // Production-readiness F.1: a snapshot is the deterministic-
        // materialization reference; a canonical change after the
        // snapshot makes a replay DIVERGENT.
        let root = std::env::temp_dir().join(format!("mag-snap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let entry_dir = root.join("memory/canonical/entries").join(utc_now_date());
        std::fs::create_dir_all(&entry_dir).unwrap();
        std::fs::write(
            entry_dir.join("snap-001.md"),
            "# Canonical entry snap-001\n\n- domain: eval\n\n## F-000 `aaaaaaaaaaaaaaaa`\n\nfact one\n",
        )
        .unwrap();
        derive(&root, false).unwrap();
        let snap = snapshot(&root, "pre").unwrap();
        assert!(snap.contains("canonical"), "{snap}");
        // Replay now matches.
        let r1 = replay(&root, "pre").unwrap();
        assert!(r1.contains("MATCH"), "{r1}");
        // Add a canonical fact, re-derive, replay -> DIVERGENT.
        std::fs::write(
            entry_dir.join("snap-002.md"),
            "# Canonical entry snap-002\n\n- domain: eval\n\n## F-001 `bbbbbbbbbbbbbbbb`\n\nfact two\n",
        )
        .unwrap();
        derive(&root, false).unwrap();
        let r2 = replay(&root, "pre").unwrap();
        assert!(r2.contains("DIVERGENT"), "{r2}");
        assert!(r2.contains("canonical changed"), "{r2}");
        // Missing snapshot -> SnapshotMissing.
        let err = replay(&root, "nope").unwrap_err();
        assert!(matches!(err, MemoryError::SnapshotMissing(_)));
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod fact_link_tests {
    use super::*;
    use std::env;

    #[test]
    fn links_facts_sharing_two_keywords() {
        let facts = vec![
            (
                "aaa".into(),
                "strategy".into(),
                "failure register prevents repeated failing actions across runs".into(),
            ),
            (
                "bbb".into(),
                "strategy".into(),
                "the register records failing actions before every rerun".into(),
            ),
            (
                "ccc".into(),
                "eval".into(),
                "composite scoring measures outcome trajectory tool use".into(),
            ),
        ];
        let links = fact_links(&facts);
        assert_eq!(
            links["aaa"],
            vec!["bbb"],
            "shared: failure/register/actions/rerun"
        );
        assert!(!links.contains_key("ccc"), "no shared keywords");
    }

    #[test]
    fn brief_orders_by_importance_and_lists_links() {
        let root = env::temp_dir().join(format!("mag-links-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let facts = vec![
            (
                "aaa".into(),
                "s".into(),
                "failure register prevents repeated failing actions across runs".into(),
            ),
            (
                "bbb".into(),
                "s".into(),
                "the register records failing actions before every rerun".into(),
            ),
            (
                "ccc".into(),
                "e".into(),
                "composite scoring measures outcome trajectory tool use".into(),
            ),
        ];
        let brief = render_brief(&root, &facts);
        let pos_a = brief.find("`aaa`").unwrap();
        let pos_b = brief.find("`bbb`").unwrap();
        let pos_c = brief.find("`ccc`").unwrap();
        assert!(
            pos_a < pos_c && pos_b < pos_c,
            "linked facts come before isolated ones"
        );
        assert!(brief.contains("links: bbb"));
        let _ = fs::remove_dir_all(&root);
    }
}
