//! Dream-loop (D2): episodic material -> staged facts -> audited
//! verdicts -> canonical promotion.
//!
//! The distiller is a cheap model, the auditor a strong one; the worker
//! invocations live in the binary crate while this module owns the pure
//! logic: prompts, JSON parsing, staging layout, verdict application,
//! and the ADR-0010 human-signoff routing for enforcement-bound facts.

use std::path::Path;

/// Staging root under a repo root.
pub const STAGING_REL: &str = "memory/staging";

/// One staged candidate fact.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StagedFact {
    /// The candidate fact's durable statement.
    pub body: String,
    /// Memory domain the distiller assigned.
    pub domain: String,
}

impl Eq for StagedFact {}

/// Concatenate the `text` parts of an opencode `--format json` stream.
///
/// The raw stream carries `parts:[...]` arrays in every event — a naive
/// `find('[')` would match those, not the model's answer. The answer
/// lives in the `type: text` events; the parsers below run the
/// balanced-array scan on the concatenated text only (grounded in a
/// real opencode 1.18.11 stream, 2026-08-06).
#[must_use]
pub fn extract_text_parts(output: &str) -> String {
    let mut out = String::new();
    for line in output.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(serde_json::Value::as_str) != Some("text") {
            continue;
        }
        if let Some(text) = v
            .get("part")
            .and_then(|p| p.get("text"))
            .and_then(serde_json::Value::as_str)
        {
            out.push_str(text);
            out.push('\n');
        }
    }
    out
}

/// Extract a JSON fact list from a distiller's free-form output.
///
/// The distiller may wrap the JSON in prose or fences; the FIRST
/// balanced `[...]` array of the text parts is taken, and each element
/// must carry a `body` string (domain optional, defaults to "general").
/// Defensive: malformed elements are skipped, never fatal.
#[must_use]
pub fn parse_distilled_facts(output: &str) -> Vec<StagedFact> {
    // Plain-text outputs (tests, non-streaming workers) have no JSON
    // events: fall back to the raw output when nothing was extracted.
    let extracted = extract_text_parts(output);
    let text = if extracted.is_empty() {
        output.to_string()
    } else {
        extracted
    };
    let Some(start) = text.find('[') else {
        return Vec::new();
    };
    let mut depth = 0i64;
    let mut in_string = false;
    let mut escaped = false;
    let mut end = None;
    for (i, c) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text[start..=end]) else {
        return Vec::new();
    };
    let Some(items) = v.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let body = item.get("body")?.as_str()?.trim().to_string();
            if body.len() < 8 {
                return None;
            }
            let domain = item
                .get("domain")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("general")
                .to_string();
            Some(StagedFact { body, domain })
        })
        .collect()
}

/// The distiller prompt: cheap model extracts durable facts.
#[must_use]
pub fn distiller_prompt(material: &str) -> String {
    format!(
        "You are the memory DISTILLER. From the material below, extract the durable, \
         load-bearing facts worth remembering — decisions, mechanisms, evidence, \
         constraints. Skip ephemera (status updates, task noise, greetings). \
         Emit ONLY a JSON array, no prose:\n\
         [{{\"body\": \"<fact>\", \"domain\": \"general\"}}]\n\n\
         EVIDENTIAL REGISTER (binding, Manufactured Confidence 2606.29279): keep the \
         source's register — NEVER upgrade hedged, casual, or tentative statements \
         into confident facts. If the material hedges ('likely', 'reported', \
         'suggests', 'appears'), the fact body MUST carry that hedge. A passive \
         'unverified' tag stays 'unverified'.\n\n\
         MATERIAL:\n{material}"
    )
}

/// Retry feedback appended when the distiller's output failed to parse
/// into any facts (bounded retry with validator feedback — cycle 33
/// finding: deterministic validator + bounded retry ≈96% validity).
#[must_use]
pub const fn distiller_retry_feedback() -> &'static str {
    "VALIDATOR FEEDBACK: your previous response contained no parseable JSON array \
     of fact objects. Re-emit the facts as a single JSON array of exactly \
     [{\"body\": \"<fact>\", \"domain\": \"<domain>\"}] items, ONLY that array, \
     no prose, no fences, no commentary."
}

/// Retry feedback for the auditor (procedure-directed retry, #95): a
/// generic "try again" reproduces the failure; naming the missing
/// procedure recovers most cases.
///
/// Appended when the auditor's output failed to parse into any verdicts.
#[must_use]
pub const fn auditor_retry_feedback() -> &'static str {
    "VALIDATOR FEEDBACK: your previous response contained no parseable JSON array \
     of verdict objects. Re-emit ONE array object PER candidate (one object per \
     numbered candidate, same count as the CANDIDATES list), each of exactly \
     [{\"index\": <candidate number>, \"verdict\": \"promote|duplicate|conflict|reject\", \
     \"reason\": \"...\", \"existing_id\": \"<16-hex id when duplicate/conflict, else omit>\"}] — \
     ONLY that array, no prose, no fences, no commentary. Do not collapse multiple \
     candidates into one object."
}

/// The auditor prompt: strong model judges each staged fact against the
/// caller-selected canonical fact index.
#[must_use]
pub fn auditor_prompt(staged: &[StagedFact], canonical_index: &str) -> String {
    let items: Vec<String> = staged
        .iter()
        .enumerate()
        .map(|(i, f)| format!("[{i}] {}\n", f.body))
        .collect();
    format!(
        "You are the memory AUDITOR. For each numbered candidate fact, judge it against \
         the attached canonical facts and emit a JSON array with one object per candidate:\n\
         [{{\"index\": 0, \"verdict\": \"promote|duplicate|conflict|reject\", \"reason\": \"...\", \
         \"existing_id\": \"<16-hex id when duplicate/conflict, else omit>\"}}]\n\n\
         promote = new durable truth; duplicate = same as an existing fact; conflict = \
         contradicts an existing fact; reject = ephemeral, vague, or unverifiable. \
         FAILURE-MODE CHECK (binding, TrustMem 2606.25161): before promoting, test the \
         candidate for OMISSION (it drops a constraint or source limit the material \
         carried), CORRUPTION (wording upgrades hedging to confidence), and \
         FABRICATION (a claim the material does not support). Any of the three -> \
         conflict or reject with the reason, never promote. Emit ONLY the JSON array.\n\n\
         CANDIDATES:\n{}\n\nCANONICAL FACTS (id: body):\n{canonical_index}",
        items.join("\n")
    )
}

/// One auditor verdict.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AuditorVerdict {
    /// Index into the staged facts list this verdict judges.
    pub index: usize,
    /// One of promote | duplicate | conflict | reject.
    pub verdict: String,
    /// Auditor's justification.
    pub reason: Option<String>,
    /// Known fact id when duplicate/conflict.
    pub existing_id: Option<String>,
}

/// Parse the auditor's JSON verdict array (defensive, same balanced-array
/// scan as the distiller output, on the text parts only). Unknown
/// verdicts and out-of-range indexes are dropped.
#[must_use]
pub fn parse_audit_verdicts(output: &str, staged: &[StagedFact]) -> Vec<AuditorVerdict> {
    let extracted = extract_text_parts(output);
    let text = if extracted.is_empty() {
        output.to_string()
    } else {
        extracted
    };
    let Some(start) = text.find('[') else {
        return Vec::new();
    };
    let mut depth = 0i64;
    let mut in_string = false;
    let mut escaped = false;
    let mut end = None;
    for (i, c) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text[start..=end]) else {
        return Vec::new();
    };
    let Some(items) = v.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        let Some(index) = item.get("index").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let Ok(index) = usize::try_from(index) else {
            continue;
        };
        if index >= staged.len() {
            continue;
        }
        let Some(verdict) = item.get("verdict").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if !matches!(verdict, "promote" | "duplicate" | "conflict" | "reject") {
            continue;
        }
        out.push(AuditorVerdict {
            index,
            verdict: verdict.to_string(),
            reason: item
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            existing_id: item
                .get("existing_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        });
    }
    out
}

/// Write the staged candidates to `memory/staging/<date>/<seq>.md` with
/// provenance-by-construction (extracted-by, source, timestamp), one
/// `## S-NNN` block per fact.
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the staging file cannot be written.
pub fn write_staging(
    root: &Path,
    staged: &[StagedFact],
    source: &str,
    extracted_by: &str,
) -> Result<std::path::PathBuf, crate::memory::MemoryError> {
    let today = crate::memory::utc_now_date();
    let seq = staging_seq(root, &today);
    let path = root
        .join(STAGING_REL)
        .join(&today)
        .join(format!("{seq:03}.md"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(crate::memory::MemoryError::Io)?;
    }
    let stamp = crate::memory::utc_now_stamp();
    let mut blocks = vec![format!(
        "# Staged candidates (dream distiller)\n\n- date: {stamp}\n- source: {source}\n- extracted_by: {extracted_by}"
    )];
    for (i, fact) in staged.iter().enumerate() {
        blocks.push(format!("\n## S-{i:03} ({})\n\n{}", fact.domain, fact.body));
    }
    std::fs::write(&path, blocks.join("\n")).map_err(crate::memory::MemoryError::Io)?;
    Ok(path)
}

/// Persist auditor verdicts next to the staging file
/// (`memory/staging/<date>/<seq>.verdicts.json`) so `dream promote`
/// applies the TRUTHFUL audit, not a re-run or a default.
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when the manifest cannot be written.
pub fn write_verdicts(
    staged_path: &Path,
    verdicts: &[AuditorVerdict],
) -> Result<std::path::PathBuf, crate::memory::MemoryError> {
    let manifest = staged_path.with_extension("verdicts.json");
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "staged": staged_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "verdicts": verdicts,
    }))
    .map_err(|e| crate::memory::MemoryError::Io(std::io::Error::other(e)))?;
    std::fs::write(&manifest, json).map_err(crate::memory::MemoryError::Io)?;
    Ok(manifest)
}

/// Read a persisted verdicts manifest.
#[must_use]
pub fn read_verdicts(path: &Path) -> Vec<AuditorVerdict> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(items) = v.get("verdicts").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let index = usize::try_from(item.get("index")?.as_u64()?).ok()?;
            let verdict = item.get("verdict")?.as_str()?.to_string();
            Some(AuditorVerdict {
                index,
                verdict,
                reason: item
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                existing_id: item
                    .get("existing_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

fn staging_seq(root: &Path, today: &str) -> usize {
    let dir = root.join(STAGING_REL).join(today);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 1;
    };
    entries
        .flatten()
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<usize>().ok())
        })
        .max()
        .map_or(1, |m| m + 1)
}

/// Durable proof that one staged batch's recorded verdicts were applied.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PromotionReceipt {
    /// ISO timestamp of the application.
    pub at: String,
    /// Staged file name this receipt covers.
    pub staged: String,
    /// SHA-256 of the exact staged bytes at application time.
    pub staged_sha256: String,
    /// Facts written to canonical memory.
    pub promoted: usize,
    /// Facts routed to the human queue (enforcement/conflict).
    pub queued: usize,
    /// Facts skipped (duplicate/reject/known).
    pub skipped: usize,
}

/// The receipt path for a staged file (`<stem>.promotion.json`).
#[must_use]
pub fn promotion_receipt_path(staged: &Path) -> std::path::PathBuf {
    staged.with_extension("promotion.json")
}

/// Read the application receipt for a staged file, if one exists.
#[must_use]
pub fn read_promotion_receipt(staged: &Path) -> Option<PromotionReceipt> {
    let text = std::fs::read_to_string(promotion_receipt_path(staged)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write the application receipt LAST, after `apply_verdicts` has
/// completed successfully.
///
/// # Errors
///
/// Returns a memory error when the staged file cannot be read or the
/// receipt cannot be written.
pub fn write_promotion_receipt(
    staged: &Path,
    promoted: usize,
    queued: usize,
    skipped: usize,
) -> Result<std::path::PathBuf, crate::memory::MemoryError> {
    let bytes = std::fs::read(staged).map_err(crate::memory::MemoryError::Io)?;
    let receipt = PromotionReceipt {
        at: crate::memory::utc_now_stamp(),
        staged: staged
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string(),
        staged_sha256: crate::hash::source_sha256_bytes(&bytes),
        promoted,
        queued,
        skipped,
    };
    let json = serde_json::to_string_pretty(&receipt)
        .map_err(|error| crate::memory::MemoryError::Io(std::io::Error::other(error)))?;
    let path = promotion_receipt_path(staged);
    std::fs::write(&path, json).map_err(crate::memory::MemoryError::Io)?;
    Ok(path)
}

/// Whether a receipt still describes the CURRENT staged bytes.
#[must_use]
pub fn receipt_matches_staged(staged: &Path, receipt: &PromotionReceipt) -> bool {
    std::fs::read(staged)
        .is_ok_and(|bytes| crate::hash::source_sha256_bytes(&bytes) == receipt.staged_sha256)
}

/// Apply auditor verdicts to staged facts (D2 promotion).
///
/// - `promote` -> written to canonical (kind `dream`), unless the body
///   carries `enforced_by` (ADR-0010: routed to the human queue) or
///   collides with a preserved fact (directed consolidation, D3).
/// - `conflict` -> human queue (`append_contested`) with the auditor's
///   reason; `duplicate` -> skipped (the existing id recorded);
///   `reject` -> skipped with the reason recorded.
///
/// Returns `(promoted, queued, skipped)` counts.
///
/// # Errors
///
/// Returns [`MemoryError::Io`] when a queue or canonical write fails.
pub fn apply_verdicts(
    root: &Path,
    staged: &[StagedFact],
    verdicts: &[AuditorVerdict],
    source: &str,
    dry_run: bool,
) -> Result<(usize, usize, usize), crate::memory::MemoryError> {
    let mut known: std::collections::HashSet<String> =
        crate::memory::existing_fact_ids(root).into_iter().collect();
    let preserved = crate::memory::preserved_ids(root);
    // Hoisted once (cycle-33 review F3): the duplicate arm matches a
    // candidate against an existing fact's body; reading the whole
    // canonical store per duplicate verdict was O(verdicts × entries).
    let all_facts = crate::memory::read_all_facts(root);
    let mut promoted: std::collections::BTreeMap<String, Vec<(String, String)>> =
        std::collections::BTreeMap::new();
    let mut queued = 0usize;
    let mut skipped = 0usize;
    for v in verdicts {
        let Some(fact) = staged.get(v.index) else {
            continue;
        };
        let h = crate::hash::fact_id(&fact.body);
        match v.verdict.as_str() {
            "promote" => {
                if known.contains(&h) {
                    skipped += 1;
                    continue;
                }
                if fact.body.contains("enforced_by") {
                    // ADR-0010: enforcement-bound facts need a human.
                    // Dedup by digest (the queue file holds them as
                    // `## C-<n> `<digest>`` blocks) — never skip on fact
                    // CONTENT (a body mentioning "existing fact hash"
                    // would otherwise be counted queued but never written).
                    let queue = root
                        .join("memory/review")
                        .join(format!("contested-{}.md", crate::memory::utc_now_date()));
                    let already = crate::memory::queued_facts(&queue)
                        .iter()
                        .any(|(d, _)| *d == h);
                    if !already && !dry_run {
                        crate::memory::append_contested(
                            root,
                            &fact.body,
                            &h,
                            source,
                            "0000000000000000",
                        )?;
                    }
                    queued += 1;
                    continue;
                }
                // Preserved-fact collision (ADR-0010 D3): compare against
                // the preserved facts' canonical BODIES (resolved from the
                // preserved ids) — a body-vs-id comparison never matches
                // and would let a promote rewrite a preserved fact.
                let preserved_ids = crate::memory::preserved_ids(root);
                let preserved_bodies: Vec<String> = crate::memory::canonical_facts(root)
                    .into_iter()
                    .filter(|(_, id)| preserved_ids.contains(id))
                    .map(|(body, _)| body)
                    .collect();
                let preserved_collision = preserved_bodies.iter().any(|p| {
                    let take = fact
                        .body
                        .char_indices()
                        .nth(fact.body.chars().count().min(40))
                        .map_or(fact.body.len(), |(i, _)| i);
                    p.starts_with(&fact.body[..take]) || fact.body[..take].starts_with(p.as_str())
                });
                if preserved_collision {
                    if !dry_run {
                        crate::memory::append_contested(root, &fact.body, &h, source, "preserved")?;
                    }
                    queued += 1;
                    continue;
                }
                promoted
                    .entry(fact.domain.clone())
                    .or_default()
                    .push((fact.body.clone(), h.clone()));
                // Two verdicts for the same index (or two staged facts
                // with identical bodies) must not write the same id
                // twice — mark it known as it is promoted.
                known.insert(h);
            }
            "conflict" => {
                let existing = v
                    .existing_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                if !dry_run {
                    crate::memory::append_contested(root, &fact.body, &h, source, &existing)?;
                }
                queued += 1;
            }
            "duplicate" => {
                // Memory evolution: the auditor called the candidate a
                // duplicate of an existing fact. If the bodies are
                // IDENTICAL, skip (the fact already exists). If they
                // differ, the candidate is an IMPROVED version of the
                // existing fact — write it as a supersede so the lineage
                // records the evolution instead of silently dropping the
                // newer wording.
                let existing = v.existing_id.clone();
                let existing_body = existing.as_deref().and_then(|id| {
                    all_facts
                        .iter()
                        .find(|(fid, _, _)| fid == id)
                        .map(|(_, _, body)| body.clone())
                });
                // Flatten the candidate the same way canonical bodies are
                // flattened (split_whitespace), so a body that differs
                // only in line-wrapping is NOT spuriously superseded.
                let flat_candidate: String =
                    fact.body.split_whitespace().collect::<Vec<_>>().join(" ");
                match (existing, existing_body) {
                    (Some(existing_id), Some(body)) if body != flat_candidate => {
                        // ADR-0010: a load-bearing (preserved) fact is a
                        // stronger contract than supersede — a duplicate
                        // verdict against one routes to the human queue
                        // instead of failing the whole batch (review
                        // F2: the write-layer PreservedId error would
                        // abort dream --promote partially).
                        if preserved.contains(&existing_id) {
                            if !dry_run {
                                crate::memory::append_contested(
                                    root,
                                    &flat_candidate,
                                    &h,
                                    source,
                                    &existing_id,
                                )?;
                            }
                            queued += 1;
                            continue;
                        }
                        crate::memory::write_supersede_entry(
                            root,
                            &[(flat_candidate, h.clone())],
                            source,
                            &fact.domain,
                            &[existing_id],
                        )?;
                        // Mark the new fact known so a SECOND duplicate
                        // verdict for the same body cannot write the same
                        // id under another seq number (exact-duplicate
                        // integrity finding).
                        known.insert(h);
                        queued += 1;
                    }
                    _ => {
                        skipped += 1;
                    }
                }
            }
            _ => {
                skipped += 1;
            }
        }
    }
    let promoted_count = promoted.values().map(Vec::len).sum();
    for (domain, facts) in promoted {
        if !dry_run {
            crate::memory::write_canonical_entry(root, &facts, source, &domain, "dream")?;
        }
    }
    Ok((promoted_count, queued, skipped))
}
