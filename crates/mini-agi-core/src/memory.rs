//! Memory engine — port of `PoC` `scripts/consolidate.py` + `scripts/derive.py`.
//!
//! Consolidation: episodic buffer -> canonical facts (append-only, deduped,
//! contested-wording queue + signoff). Derivation: canonical -> context brief,
//! per-domain fragments (regenerated, never hand-edited).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
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
    /// A supersede write targeted a preserved (load-bearing) fact id.
    #[error("cannot supersede preserved id {0} — preservation is a stronger contract (ADR-0010)")]
    PreservedId(String),
    /// A derive snapshot/replay name contained path separators/traversal.
    #[error("invalid snapshot name '{0}' — use plain alphanumeric/-_ (no path separators)")]
    InvalidSnapshotName(String),
    /// A derive snapshot file existed but did not parse as JSON.
    #[error("corrupted snapshot '{0}': not valid JSON")]
    CorruptedSnapshot(String),
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

/// Parse `- supersedes: <id, ...>` lineage from a canonical entry's
/// frontmatter (D3 soft-delete lineage: the superseded fact stays on
/// disk, the entry records who replaced it).
fn entry_supersedes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("- supersedes:") else {
            continue;
        };
        for id in rest.split(',') {
            let id = id.trim();
            if id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit()) {
                out.push(id.to_string());
            }
        }
    }
    out
}

/// `(superseding_id, superseded_ids)` edges across all canonical entries.
#[must_use]
pub fn supersede_edges(root: &Path) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for entry in canonical_entries(root) {
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        let superseded = entry_supersedes(&text);
        if superseded.is_empty() {
            continue;
        }
        let ids: Vec<String> = parse_canonical_facts(&text)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        for id in ids {
            out.push((id, superseded.clone()));
        }
    }
    out
}

/// All soft-deleted fact ids (superseded, per the lineage frontmatter).
#[must_use]
pub fn superseded_ids(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for (_, superseded) in supersede_edges(root) {
        out.extend(superseded);
    }
    out
}

/// Detect supersede cycles (a supersedes b and b supersedes a) — broken
/// lineage. Deterministic DFS; one representative cycle per loop.
#[must_use]
pub fn supersede_cycles(edges: &[(String, Vec<String>)]) -> Vec<Vec<String>> {
    use std::collections::HashMap;
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, bs) in edges {
        for b in bs {
            adj.entry(a).or_default().push(b);
        }
    }
    let mut nodes: Vec<&str> = adj.keys().copied().collect();
    nodes.sort_unstable();
    let mut cycles = Vec::new();
    for &start in &nodes {
        // DFS along supersede edges from `start`; if we return to a node
        // on the current path, we found a cycle.
        let mut path: Vec<&str> = Vec::new();
        let mut on_path: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut visited: std::collections::HashSet<&str> = std::collections::HashSet::new();
        if dfs_cycle(&adj, start, &mut path, &mut on_path, &mut visited) {
            // The path contains the cycle tail; extract it.
            if let Some(pos) = path.iter().position(|n| *n == start) {
                let mut cyc: Vec<String> = path[pos..].iter().map(|s| (*s).to_string()).collect();
                cyc.push(start.to_string());
                cycles.push(cyc);
            }
        }
    }
    // Dedup by sorted node set so each cycle is reported once.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    cycles.retain(|c| {
        let mut key = c.clone();
        key.sort();
        key.dedup();
        seen.insert(key.join(","))
    });
    cycles
}

fn dfs_cycle<'a>(
    adj: &std::collections::HashMap<&'a str, Vec<&'a str>>,
    node: &'a str,
    path: &mut Vec<&'a str>,
    on_path: &mut std::collections::HashSet<&'a str>,
    visited: &mut std::collections::HashSet<&'a str>,
) -> bool {
    if on_path.contains(node) {
        return true;
    }
    if visited.contains(node) {
        return false;
    }
    path.push(node);
    on_path.insert(node);
    for &next in adj.get(node).into_iter().flatten() {
        if dfs_cycle(adj, next, path, on_path, visited) {
            return true;
        }
    }
    on_path.remove(node);
    visited.insert(node);
    path.pop();
    false
}

/// Exact-duplicate scan: identical flat fact bodies carrying DIFFERENT
/// ids (the dedup gate's finding; the fix is a supersede, never an edit).
#[must_use]
pub fn exact_duplicates(root: &Path) -> Vec<(String, String)> {
    let mut by_body: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (id, _, flat) in read_all_facts(root) {
        by_body.entry(flat).or_default().push(id);
    }
    let mut out = Vec::new();
    for (_, ids) in by_body {
        for pair in ids.windows(2) {
            out.push((pair[0].clone(), pair[1].clone()));
        }
    }
    out
}

/// Deterministic findings used by `mem verify` and read-only supervision.
/// Empty means the scan observed no exact duplicates, broken lineage, or
/// preservation violations. This function performs no writes.
#[must_use]
pub fn integrity_findings(root: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    for (a, b) in exact_duplicates(root) {
        findings.push(format!(
            "exact duplicate bodies: {a} == {b} (supersede one)"
        ));
    }
    let known = existing_fact_ids(root);
    let preserved = preserved_ids(root);
    let edges = supersede_edges(root);
    for (superseding, superseded) in &edges {
        for id in superseded {
            if !known.contains(id) {
                findings.push(format!(
                    "supersede ref to unknown id {id} (from {superseding})"
                ));
            } else if preserved.contains(id) {
                // Preservation is a stronger contract than supersede:
                // a load-bearing id must not be soft-deleted by a
                // lineage write (A-MEM supersede-never).
                findings.push(format!(
                    "supersede ref targets preserved id {id} (from {superseding}) — preserved facts must not be superseded"
                ));
            }
        }
    }
    // A supersede CYCLE (a supersedes b and b supersedes a) is broken
    // lineage — both ends are soft-deleted and unreachable.
    for cycle in supersede_cycles(&edges) {
        findings.push(format!(
            "supersede cycle: {} — lineage must be a DAG (break the cycle with a fresh entry)",
            cycle.join(" -> ")
        ));
    }
    for id in preserved_ids(root) {
        if !known.contains(&id) {
            findings.push(format!("preserved id {id} does not exist in canonical"));
        }
    }
    findings
}

/// Preserved fact ids from `memory/canonical/preserved.md`.
///
/// One 16-hex id per line, `#` comments. Load-bearing facts exempt from
/// merge/supersede; a consolidation colliding with one routes to the
/// human queue (directed consolidation, D3).
#[must_use]
pub fn preserved_ids(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("memory/canonical/preserved.md")) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| l.len() == 16 && l.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_string)
        .collect()
}

/// Append one or more fact ids to the preservation list.
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the list cannot be written.
pub fn preserve_ids(root: &Path, ids: &[String]) -> Result<PathBuf, MemoryError> {
    let list = root.join("memory/canonical/preserved.md");
    if let Some(parent) = list.parent() {
        fs::create_dir_all(parent)?;
    }
    // Idempotent: skip ids already on the list so repeated `preserve`
    // calls do not accumulate duplicate lines.
    let existing = preserved_ids(root);
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&list)?;
    for id in ids {
        if existing.contains(id) {
            continue;
        }
        writeln!(f, "{id}")?;
    }
    Ok(list)
}

/// Remove fact ids from the preservation list — the counterpart to
/// `preserve_ids` (supersede of a preserved id is refused, so a wrongly
/// preserved id must be un-preserved before it can be superseded).
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the list cannot be rewritten, or
/// an explicit miss when none of the ids are on the list.
pub fn unpreserve_ids(root: &Path, ids: &[String]) -> Result<usize, MemoryError> {
    let path = root.join("memory/canonical/preserved.md");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| {
            let t = l.trim();
            !ids.iter().any(|id| t == id)
        })
        .collect();
    let removed = text.lines().filter(|l| {
        let t = l.trim();
        ids.iter().any(|id| t == id)
    });
    let removed_count = removed.count();
    if removed_count == 0 {
        return Err(MemoryError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("none of the ids are preserved: {ids:?}"),
        )));
    }
    let parent = path.parent().ok_or_else(|| {
        MemoryError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no parent dir",
        ))
    })?;
    fs::create_dir_all(parent)?;
    if kept.is_empty() {
        // Removing the last preserved id: drop the file entirely rather
        // than leave a newline-only stub (review F3).
        fs::remove_file(&path)?;
    } else {
        fs::write(&path, format!("{}\n", kept.join("\n")))?;
    }
    Ok(removed_count)
}

/// Write a superseding canonical entry (D3): a NEW fact whose frontmatter
/// records the soft-deleted lineage — `- supersedes: <id, ...>`.
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the entry cannot be written.
pub fn write_supersede_entry(
    root: &Path,
    facts: &[(String, String)],
    source: &str,
    domain: &str,
    supersedes: &[String],
) -> Result<EntryFile, MemoryError> {
    // Preservation is a stronger contract than supersede (A-MEM
    // supersede-never): a lineage write must not soft-delete a
    // load-bearing id. Enforced here, not just flagged by mem verify.
    let preserved = preserved_ids(root);
    for id in supersedes {
        if preserved.contains(id) {
            return Err(MemoryError::PreservedId(id.clone()));
        }
    }
    let mut entry = crate::store::next_entry(root, &utc_now_date());
    if let Some(parent) = entry.path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stamp = utc_now_stamp();
    let stem = entry
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("entry");
    let mut content = format!("# Canonical entry {stem} (supersedes from {source})\n\n");
    let source = flat_meta(source);
    let domain = flat_meta(domain);
    let _ = writeln!(
        content,
        "- date: {stamp}\n- source: {source}\n- domain: {domain}\n- kind: supersede"
    );
    let _ = writeln!(content, "- supersedes: {}", supersedes.join(", "));
    for (i, (fact, digest)) in facts.iter().enumerate() {
        let _ = writeln!(content, "\n## F-{i:03} `{digest}`\n\n{fact}");
    }
    // Same create_new TOCTOU guard as write_canonical_entry: a concurrent
    // supersede scan-then-write must not overwrite an existing entry.
    let mut attempt = 0;
    loop {
        let path = entry.path.clone();
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                use std::io::Write as _;
                f.write_all(content.as_bytes())?;
                return Ok(EntryFile {
                    path,
                    date: entry.date.clone(),
                    seq: entry.seq,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                attempt += 1;
                if attempt >= 10 {
                    return Err(MemoryError::Io(e));
                }
                entry = crate::store::next_entry(root, &entry.date);
            }
            Err(e) => return Err(MemoryError::Io(e)),
        }
    }
}

/// Fact ids of enforcement-bound facts (ADR-0010): bodies carrying an
/// `enforced_by:` check. The budgeted selector always ranks them first.
#[must_use]
pub fn enforced_fact_ids(root: &Path) -> Vec<String> {
    read_all_facts(root)
        .into_iter()
        .filter(|(_, _, body)| body.contains("enforced_by"))
        .map(|(id, _, _)| id)
        .collect()
}

/// Budgeted selective retrieval (D3): rank facts by
/// enforced(3) + link-degree(2) + recency, then fill until the char
/// budget. Enforced facts (ADR-0010) always survive when they fit.
#[must_use]
pub fn select_budgeted(
    facts: &[(String, String, String)],
    links: &std::collections::BTreeMap<String, Vec<String>>,
    enforced: &[String],
    budget_chars: usize,
) -> Vec<(String, String, String)> {
    if budget_chars == 0 {
        return Vec::new();
    }
    let total = facts.len();
    let mut scored: Vec<(i64, usize, &(String, String, String))> = facts
        .iter()
        .enumerate()
        .map(|(i, f)| (relevance_score(f, i, total, links, enforced), i, f))
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut out = Vec::new();
    let mut used = 0usize;
    for (_, _, fact) in scored {
        let cost = fact.2.len() + 1;
        if used + cost > budget_chars && !out.is_empty() {
            break;
        }
        used += cost;
        out.push((fact.0.clone(), fact.1.clone(), fact.2.clone()));
    }
    out
}

/// All facts INCLUDING superseded (lineage reads; the derived views use
/// [`read_facts`], which soft-excludes them).
#[must_use]
pub fn read_all_facts(root: &Path) -> Vec<(String, String, String)> {
    read_facts_impl(root, false)
}

/// `(fact_id, domain, body)` triples from canonical entries (`PoC` `read_facts`).
///
/// Body is whitespace-flattened exactly like `" ".join(m.group(2).split())`.
/// Soft-deleted facts (superseded, D3) are excluded from the VIEW.
#[must_use]
pub fn read_facts(root: &Path) -> Vec<(String, String, String)> {
    read_facts_impl(root, true)
}

fn read_facts_impl(root: &Path, exclude_superseded: bool) -> Vec<(String, String, String)> {
    let excluded: Vec<String> = if exclude_superseded {
        superseded_ids(root)
    } else {
        Vec::new()
    };
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
            if exclude_superseded && excluded.contains(&id) {
                continue;
            }
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
    let all = read_facts(root);
    let links = fact_links(&all);
    let enforced = enforced_fact_ids(root);
    let mut out = Vec::new();
    for (id, fact_domain, body) in all {
        let domain_matches = domain.is_none_or(|d| fact_domain == d);
        let keyword_matches =
            keyword.is_none_or(|k| body.to_lowercase().contains(&k.to_lowercase()));
        if domain_matches && keyword_matches {
            out.push((id, fact_domain, body));
        }
    }
    // Relevance-ranked (cycle-33 memory-evolution): the same enforced +
    // link-degree + recency signal `select_budgeted` uses, WITHOUT a
    // budget — so `mem query` and the MCP `memory_query` return the most
    // load-bearing facts first instead of id-sorted.
    ranked_facts(&out, &links, &enforced)
}

/// Shared relevance score for a fact at index `i` of `facts` (cycle-33
/// review F1 / dedup): enforced (3) + link-degree (2, capped) + recency.
/// `select_budgeted` and `ranked_facts` must agree on what matters, so
/// the scoring lives in ONE place — a drift in one would silently
/// diverge the query and budgeted contexts.
fn relevance_score(
    fact: &(String, String, String),
    index: usize,
    total: usize,
    links: &std::collections::BTreeMap<String, Vec<String>>,
    enforced: &[String],
) -> i64 {
    let mut score = 0i64;
    if enforced.iter().any(|e| e == &fact.0) {
        score += 3;
    }
    let degree = i64::try_from(links.get(&fact.0).map_or(0, |v| v.len().min(2))).unwrap_or(0);
    score += degree;
    let recency = i64::try_from(total.saturating_sub(index)).unwrap_or(0);
    score * 100_000 + recency
}

/// Relevance order over `facts` (enforced, link-degree, recency;
/// descending). Budget-free twin of `select_budgeted` — same scoring,
/// so a query and a budgeted context agree on what matters.
///
/// Deterministic (index tie-break).
#[must_use]
pub fn ranked_facts(
    facts: &[(String, String, String)],
    links: &std::collections::BTreeMap<String, Vec<String>>,
    enforced: &[String],
) -> Vec<(String, String, String)> {
    let total = facts.len();
    let mut scored: Vec<(i64, usize, &(String, String, String))> = facts
        .iter()
        .enumerate()
        .map(|(i, f)| (relevance_score(f, i, total, links, enforced), i, f))
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored.into_iter().map(|(_, _, f)| f.clone()).collect()
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
    let mut entry = crate::store::next_entry(root, &utc_now_date());
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
    let source = flat_meta(source);
    let domain = flat_meta(domain);
    let _ = writeln!(
        content,
        "- date: {stamp}\n- source: {source}\n- domain: {domain}\n- kind: {kind}"
    );
    for (i, (fact, digest)) in facts.iter().enumerate() {
        let _ = writeln!(content, "\n## F-{i:03} `{digest}`\n\n{fact}");
    }
    // TOCTOU guard: next_entry is scan-then-write; a concurrent writer
    // can claim the same sequence. create_new makes the collision FAIL
    // (never overwrite an existing entry — append-only contract,
    // ADR-0002) and retries with the next free sequence.
    let mut attempt = 0;
    loop {
        let path = entry.path.clone();
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                use std::io::Write as _;
                f.write_all(content.as_bytes())?;
                return Ok(EntryFile {
                    path,
                    date: entry.date.clone(),
                    seq: entry.seq,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                attempt += 1;
                if attempt >= 10 {
                    return Err(MemoryError::Io(e));
                }
                entry = crate::store::next_entry(root, &entry.date);
            }
            Err(e) => return Err(MemoryError::Io(e)),
        }
    }
}

/// Append one contested fact to the review queue (`PoC` `append_contested`).
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the queue cannot be opened or written.
pub fn append_contested(
    root: &Path,
    fact: &str,
    _digest: &str,
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
    // Single-line payload: a multi-line fact would truncate on the
    // queue parse and promote an id that no longer hashes its body.
    let fact_flat = fact.replace('\n', " ");
    // The stored digest MUST hash the stored (flattened) payload, or the
    // signoff digest check rejects the entry forever (dead HITL state).
    let digest = fact_id(&fact_flat);
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&queue)?;
    let source = flat_meta(source);
    writeln!(
        f,
        "## C-{number:03} `{digest}`\n- source: {source}\n- reason: same first 40 chars\n- existing fact hash: {existing_hash}\n\n{fact_flat}\n"
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
                // Char-boundary-safe prefix: a multibyte fact prefix must
                // not panic on a non-char boundary slice.
                let take = fact
                    .char_indices()
                    .nth(fact.chars().count().min(40))
                    .map_or(fact.len(), |(i, _)| i);
                old_fact.starts_with(&fact[..take]) && *old_fact != fact
            })
            .map(|(_, old_hash)| old_hash.clone());
        // Directed consolidation (D3): a candidate colliding with a
        // PRESERVED fact is never silently merged or superseded — it
        // routes to the human queue unconditionally (preservation is a
        // stronger contract than the general contested path). The
        // preserved ids resolve to their canonical BODIES; the candidate
        // prefix is matched against them (same char-boundary-safe
        // prefix rule as the contested check).
        let preserved: Vec<(&str, &str)> = canonical
            .iter()
            .filter(|(_, id)| preserved_ids(root).contains(id))
            .map(|(body, id)| (body.as_str(), id.as_str()))
            .collect();
        let preserved_collision = (!preserved.is_empty())
            .then(|| {
                let take = fact
                    .char_indices()
                    .nth(fact.chars().count().min(40))
                    .map_or(fact.len(), |(i, _)| i);
                preserved
                    .iter()
                    .find(|(body, _)| {
                        body.starts_with(&fact[..take]) || fact[..take].starts_with(*body)
                    })
                    .map(|(_, id)| (*id).to_string())
            })
            .flatten();
        if let Some(preserved) = preserved_collision {
            if !opts.dry_run {
                append_contested(root, &fact, &h, source, &preserved)?;
            }
            skipped += 1;
            continue;
        }
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
    if index < 1 {
        return Err(MemoryError::BadSignoff);
    }
    // The queue path is UNTRUSTED caller input (MCP): constrain it to the
    // review queue directory — a signoff must never read an arbitrary
    // file the process can reach (path traversal).
    let queue = if queue.is_absolute() {
        queue.to_path_buf()
    } else {
        root.join(queue)
    };
    let review_root = root.join(REVIEW_REL);
    let review_canon = review_root
        .canonicalize()
        .map_err(|_| MemoryError::BadSignoff)?;
    let queue_canon = queue.canonicalize().map_err(|_| MemoryError::BadSignoff)?;
    if !queue_canon.starts_with(&review_canon) {
        return Err(MemoryError::BadSignoff);
    }
    if !queue.exists() {
        return Err(MemoryError::BadSignoff);
    }
    // Read the CANONICAL path (not the raw caller string): this NARROWS
    // the symlink-swap window between containment and read to a
    // concurrent local writer racing the kernel (not a remote-caller
    // escape) — it does not fully close the TOCTOU.
    let blocks = queued_facts(&queue_canon);
    let Some((digest, fact)) = blocks.get(index - 1).cloned() else {
        return Err(MemoryError::IndexNotFound);
    };
    // The queued digest must actually hash the queued body — a caller
    // cannot promote under an arbitrary id (fact ids = sha256[:16]).
    if crate::hash::fact_id(&fact) != digest {
        return Err(MemoryError::BadSignoff);
    }
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

/// Flatten newlines/control chars out of a caller-supplied STRING used in
/// canonical frontmatter (`source`, `domain`): a value like
/// `general\n- supersedes: <id>` would inject a lifecycle line.
#[must_use]
fn flat_meta(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
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

/// Domain stop words for the fact-linking pass. These are words too
/// frequent across the memory store to carry linking signal (measured:
/// "memory" appears in 130 facts, "model" 121, "accuracy" 104 — linking
/// on them made ~99% of facts pairwise-linked, drowning the brief).
/// Filtering them restores the meaning of `shared >= 4`: two facts link
/// only through genuinely specific shared terms.
const LINK_STOP_WORDS: &[&str] = &[
    "memory",
    "model",
    "models",
    "accuracy",
    "measured",
    "failure",
    "agent",
    "agents",
    "source",
    "context",
    "tokens",
    "token",
    "arxiv",
    "across",
    "state",
    "while",
    "every",
    "without",
    "schema",
    "output",
    "verified",
    "openai",
    "human",
    "repair",
    "reasoning",
    "system",
    "error",
    "retry",
    "claude",
    "feedback",
    "result",
    "results",
    "paper",
    "study",
    "rate",
    "rates",
    "using",
    "based",
    "reported",
    "report",
    "according",
    "though",
    "likely",
    "rather",
    "there",
    "these",
    "their",
    "other",
    "often",
    "also",
    "both",
    "each",
    "into",
    "from",
    "with",
    "than",
    "then",
    "that",
    "this",
    "have",
    "was",
    "has",
    "its",
    "are",
    "may",
    "can",
    "the",
    "and",
    "for",
    "not",
    "but",
    "which",
    "when",
    "between",
    "within",
    "under",
    "over",
    "about",
    "against",
    "after",
];

/// Keyword index of a fact body: words of 5+ chars, lowercased, minus
/// the domain stop words (fact-linking pass, Phase 8 slice 6 — derived
/// views only).
fn keywords(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.len() >= 5 && !LINK_STOP_WORDS.contains(&w.as_str()))
        .collect()
}

/// Cross-fact links (Phase 8 slice 6): facts sharing >= 4 keywords
/// after domain stop-word filtering. Deterministic, DERIVED views only.
///
/// Naive `>= 2` on a one-topic corpus linked ~99% of facts (avg 36
/// links/fact); `>= 4` cuts that ~8x (17,605 -> 2,273 edges) while
/// keeping genuinely related facts linked. Canonical stays append-only
/// (A-MEM 2502.12110, supersede-never semantics).
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
            if shared >= 4 {
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

/// Reserved bytes for the truncation notice (TICKET-13): the notice is
/// emitted only when facts are dropped, so the fill loop reserves its
/// worst-case length to keep the brief strictly <= [`MAX_BRIEF_BYTES`].
const BRIEF_NOTICE_RESERVE: usize = 192;

/// Render the context brief with importance-ordered facts + links
/// (Phase 8 slice 6). Linked facts come first — importance learned
/// from cross-referencing, not hand-assigned.
///
/// The brief is a BUDGETED working set (TICKET-13): facts are ranked by
/// enforced(3) + link-degree(2) + recency via [`ranked_facts`] and
/// filled until [`MAX_BRIEF_BYTES`]; truncated facts stay available in
/// canonical (append-only source of truth). Returns `(output, facts
/// written)`.
#[must_use]
pub fn render_brief(root: &Path, facts: &[(String, String, String)]) -> (String, usize) {
    let links = fact_links(facts);
    let enforced: Vec<String> = facts
        .iter()
        .filter(|(_, _, body)| body.contains("enforced_by"))
        .map(|(id, _, _)| id.clone())
        .collect();
    let total = facts.len();
    let mut out = provenance_block(root);
    let header = "# CONTEXT BRIEF (derived)\n\nRead this before starting any session. Canonical wins over this file.\n\n";
    let budget = MAX_BRIEF_BYTES.saturating_sub(out.len() + header.len() + BRIEF_NOTICE_RESERVE);
    let mut body = String::new();
    let mut written = 0usize;
    for (fid, domain, text) in ranked_facts(facts, &links, &enforced) {
        let mut line = format!("- `{fid}` [{domain}] {text}\n");
        let importance = links.get(&fid).map_or(0, Vec::len);
        if importance > 0 {
            let _ = writeln!(line, "  links: {}", links[&fid].join(", "));
        }
        if body.len() + line.len() > budget {
            break;
        }
        body.push_str(&line);
        written += 1;
    }
    out.push_str(header);
    out.push_str(&body);
    if written < total {
        let _ = writeln!(
            out,
            "\n# truncated: {} of {total} facts fit the {MAX_BRIEF_BYTES}B brief cap — full listing: memory/canonical/ (append-only)",
            total - written
        );
    }
    (out, written)
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

/// Regenerate all derived views from canonical memory (`PoC` `main`).
///
/// Returns `(facts_in_brief, facts_total, fragment_count)`.
///
/// # Errors
///
/// Returns [`MemoryError::NoCanonical`] when no facts exist and
/// [`MemoryError::Io`] for filesystem failures.
pub fn derive(root: &Path, brief_only: bool) -> Result<(usize, usize, usize), MemoryError> {
    let facts = read_facts(root);
    if facts.is_empty() {
        return Err(MemoryError::NoCanonical);
    }
    fs::create_dir_all(root.join(DERIVED_REL))?;
    let (brief, in_brief) = render_brief(root, &facts);
    fs::write(root.join(DERIVED_REL).join("context-brief.md"), brief)?;

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
    Ok((in_brief, facts.len(), fragments.len()))
}

/// Derive snapshot names are file names under `SNAPSHOTS_REL`; only plain
/// alphanumeric plus `-`/`_`/`.` are allowed (no separators or
/// traversal), and the length is capped well under the FS filename limit
/// so an over-long name fails with a clean validation error instead of an
/// opaque `File name too long` IO error.
fn valid_snapshot_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Snapshot the derived views (production-readiness F.1).
///
/// Records the canonical fingerprint + the brief hash under a named
/// file, so a later `replay` can prove the views are a deterministic
/// materialization.
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the snapshot cannot be written, or
/// [`MemoryError::InvalidSnapshotName`] when the name is not path-safe.
pub fn snapshot(root: &Path, name: &str) -> Result<String, MemoryError> {
    // Path-safety: the name becomes a file name under SNAPSHOTS_REL, so
    // it must not contain separators or traversal that would escape the
    // snapshots dir (e.g. "../evil" landed in derived/).
    if !valid_snapshot_name(name) {
        return Err(MemoryError::InvalidSnapshotName(name.to_string()));
    }
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
    if !valid_snapshot_name(name) {
        return Err(MemoryError::InvalidSnapshotName(name.to_string()));
    }
    let path = root.join(SNAPSHOTS_REL).join(format!("{name}.json"));
    let text = fs::read_to_string(&path).map_err(|_| MemoryError::SnapshotMissing(name.into()))?;
    let doc: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| MemoryError::CorruptedSnapshot(name.to_string()))?;
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
