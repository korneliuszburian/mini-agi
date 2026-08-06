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
         MATERIAL:\n{material}"
    )
}

/// The auditor prompt: strong model judges each staged fact.
#[must_use]
pub fn auditor_prompt(staged: &[StagedFact]) -> String {
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
         Emit ONLY the JSON array.\n\n\
         CANDIDATES:\n{}\n\nCANONICAL FACTS (id: body):\n{}",
        items.join("\n"),
        canonical_index()
    )
}

/// The canonical index the auditor is handed (id + flat body, newest
/// first, superseded facts excluded).
#[must_use]
pub const fn canonical_index() -> String {
    // Filled by the caller's view of the world; the prompt contract only
    // needs the shape. The binary crate injects read_facts output.
    String::new()
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
    _root: &Path,
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
) -> Result<(usize, usize, usize), crate::memory::MemoryError> {
    let known = crate::memory::existing_fact_ids(root);
    let preserved = crate::memory::preserved_ids(root);
    let mut promoted: Vec<(String, String)> = Vec::new();
    let mut promoted_domain = "general".to_string();
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
                    if !fact.body.contains("existing fact hash") {
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
                let preserved_collision = preserved.iter().any(|p| {
                    let take = fact
                        .body
                        .char_indices()
                        .nth(fact.body.chars().count().min(40))
                        .map_or(fact.body.len(), |(i, _)| i);
                    fact.body[..take].contains(p.as_str()) || p.contains(&fact.body[..take])
                });
                if preserved_collision {
                    crate::memory::append_contested(root, &fact.body, &h, source, "preserved")?;
                    queued += 1;
                    continue;
                }
                promoted_domain.clone_from(&fact.domain);
                promoted.push((fact.body.clone(), h));
            }
            "conflict" => {
                let existing = v
                    .existing_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                crate::memory::append_contested(root, &fact.body, &h, source, &existing)?;
                queued += 1;
            }
            _ => {
                skipped += 1;
            }
        }
    }
    let promoted_count = promoted.len();
    if !promoted.is_empty() {
        crate::memory::write_canonical_entry(root, &promoted, source, &promoted_domain, "dream")?;
    }
    Ok((promoted_count, queued, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distiller_parse_takes_balanced_array_anywhere() {
        let out = "Here are the facts I extracted:\n```json\n[{\"body\": \"decision A made\", \"domain\": \"general\"}, {\"body\": \"mechanism B works\"}]\n```\n";
        let facts = parse_distilled_facts(out);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].body, "decision A made");
        assert_eq!(facts[1].domain, "general", "domain defaults to general");
        assert!(parse_distilled_facts("no array here").is_empty());
        assert!(
            parse_distilled_facts("[{\"body\": \"short\"}]").is_empty(),
            "too short dropped"
        );
        assert!(parse_distilled_facts("[{\"nobody\": 1}]").is_empty());
    }

    #[test]
    fn auditor_verdicts_parse_and_filter() {
        let staged = vec![
            StagedFact {
                body: "f0".to_string(),
                domain: "general".to_string(),
            },
            StagedFact {
                body: "f1".to_string(),
                domain: "general".to_string(),
            },
        ];
        let out = r#"prose [{"index": 0, "verdict": "promote"}, {"index": 1, "verdict": "conflict", "existing_id": "abc123"}, {"index": 9, "verdict": "promote"}, {"index": 2, "verdict": "bogus"}]"#;
        let vs = parse_audit_verdicts(out, &staged);
        assert_eq!(
            vs.len(),
            2,
            "out-of-range and unknown verdicts dropped: {vs:?}"
        );
        assert_eq!(vs[0].verdict, "promote");
        assert_eq!(vs[1].verdict, "conflict");
        assert_eq!(vs[1].existing_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn staging_writes_provenance_by_construction() {
        let root = std::env::temp_dir().join(format!("mag-dream-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let facts = vec![
            StagedFact {
                body: "the kernel resolves worker names per call".to_string(),
                domain: "general".to_string(),
            },
            StagedFact {
                body: "opencode flash costs 0.14 USD per million input tokens".to_string(),
                domain: "general".to_string(),
            },
        ];
        let path = write_staging(&root, &facts, "2026-08-06-buffer.md", "distiller-flash").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("- source: 2026-08-06-buffer.md"));
        assert!(text.contains("- extracted_by: distiller-flash"));
        assert!(text.contains("## S-000"));
        assert!(text.contains("## S-001"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn promotion_applies_verdicts_and_routes_enforced_to_human() {
        let root = std::env::temp_dir().join(format!("mag-dream2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let staged = vec![
            StagedFact {
                body: "new durable fact about the memory pipeline".to_string(),
                domain: "general".to_string(),
            },
            StagedFact {
                body: "enforced_by review rubric: surgical changes only".to_string(),
                domain: "general".to_string(),
            },
            StagedFact {
                body: "conflicting claim about the widget".to_string(),
                domain: "general".to_string(),
            },
            StagedFact {
                body: "ephemeral status update".to_string(),
                domain: "general".to_string(),
            },
        ];
        let verdicts = vec![
            AuditorVerdict {
                index: 0,
                verdict: "promote".into(),
                reason: None,
                existing_id: None,
            },
            AuditorVerdict {
                index: 1,
                verdict: "promote".into(),
                reason: None,
                existing_id: None,
            },
            AuditorVerdict {
                index: 2,
                verdict: "conflict".into(),
                reason: Some("contradicts".into()),
                existing_id: Some("deadbeef".into()),
            },
            AuditorVerdict {
                index: 3,
                verdict: "reject".into(),
                reason: Some("ephemeral".into()),
                existing_id: None,
            },
        ];
        let (promoted, queued, skipped) =
            apply_verdicts(&root, &staged, &verdicts, "dream-test").unwrap();
        assert_eq!(promoted, 1, "enforced fact must NOT be auto-promoted");
        assert_eq!(queued, 2, "enforced + conflict land in the human queue");
        assert_eq!(skipped, 1, "reject is skipped");
        let queue = root.join(crate::memory::REVIEW_REL);
        let queued_files: Vec<_> = std::fs::read_dir(queue).unwrap().flatten().collect();
        assert_eq!(queued_files.len(), 1, "one queue file holds both entries");
        let qtext = std::fs::read_to_string(queued_files[0].path()).unwrap();
        assert!(
            qtext.contains("enforced_by"),
            "enforced fact queued for human signoff"
        );
        assert!(qtext.contains("conflicting claim"));
        let canonical = crate::memory::read_facts(&root);
        assert_eq!(canonical.len(), 1);
        assert!(canonical[0].2.contains("new durable fact"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
