//! `mini-agi audit` — repo invariants (Phase 7, slice 2).
//!
//! Checks the things that silently rot: provenance drift (canonical vs
//! derived brief), gate baseline freshness vs `evals/cases`, uncommitted
//! changes, and the eval gate itself. Verdict FAIL > WARN > OK; exit
//! 0/1/2. Findings reuse the `health` severity model so one vocabulary
//! covers machine and repo state.

use std::fs;
use std::path::Path;

use crate::health::Finding;

/// The audit report.
#[derive(Debug, Default)]
pub struct AuditReport {
    /// Findings across all checks.
    pub findings: Vec<Finding>,
    /// Passed checks (for the sensor contract: visible output per check).
    pub passed: Vec<String>,
}

impl AuditReport {
    /// Overall verdict: fail beats warn beats ok.
    #[must_use]
    pub fn verdict(&self) -> &'static str {
        if self.findings.iter().any(|f| f.severity == "fail") {
            "FAIL"
        } else if self.findings.iter().any(|f| f.severity == "warn") {
            "WARN"
        } else {
            "OK"
        }
    }
}

/// Run the audit.
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn audit(root: &Path) -> Result<AuditReport, std::io::Error> {
    let mut report = AuditReport::default();

    // 1. Provenance drift: canonical fingerprint vs the committed brief.
    let entries_dir = root.join("memory/canonical/entries");
    let has_entries = fs::read_dir(&entries_dir).is_ok_and(|rd| rd.flatten().next().is_some());
    if has_entries {
        let fresh = crate::memory::canonical_fingerprint(root);
        let brief = root.join("memory/derived/context-brief.md");
        let recorded = fs::read_to_string(&brief).ok().and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("# canonical_sha256:"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        });
        match recorded {
            Some(rec) if rec == fresh => report
                .passed
                .push(format!("provenance: canonical_sha256 {fresh} matches the brief")),
            Some(rec) => report.findings.push(Finding {
                severity: "fail".into(),
                message: format!(
                    "provenance drift: brief records {rec}, canonical is {fresh} — run mini-agi derive"
                ),
            }),
            None => report.findings.push(Finding {
                severity: "fail".into(),
                message: "provenance: context-brief.md missing or unreadable — run mini-agi derive".into(),
            }),
        }
    } else {
        report
            .passed
            .push("provenance: no canonical entries yet — nothing to drift".into());
    }

    // 2. Gate baseline freshness vs evals/cases.
    let cases_dir = root.join("evals/cases");
    if cases_dir.is_dir() {
        let cases: Vec<String> = fs::read_dir(&cases_dir)?
            .flatten()
            .filter(|e| e.path().join("run.json").is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let baseline_path = root.join("evals/results/baseline.json");
        let baseline: Vec<String> = fs::read_to_string(&baseline_path)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).ok())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| e["case"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let missing: Vec<&String> = cases.iter().filter(|c| !baseline.contains(c)).collect();
        if missing.is_empty() {
            report.passed.push(format!(
                "baseline: {} cases match evals/cases",
                baseline.len()
            ));
        } else {
            report.findings.push(Finding {
                severity: "warn".into(),
                message: format!(
                    "baseline stale: {} case(s) without a baseline entry ({} — run eval gate --write-baseline)",
                    missing.len(),
                    missing
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    } else {
        report.passed.push("baseline: no evals/cases yet".into());
    }

    // 3. Uncommitted changes. Kernel-owned artifacts (the checkpoint
    // journal, derived views, calibration/attribution logs, METRICS,
    // harness ledger) are written by the kernel/checkpoint flow and are
    // legitimately dirty during verify — only source/config drift counts.
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output();
    match status {
        Ok(out) if out.status.success() => {
            let kernel_owned: &[&str] = &["memory/", "docs/METRICS.md", "docs/harness/ledger.md"];
            let dirty = String::from_utf8_lossy(&out.stdout);
            // porcelain is "XY path" (or "?? path" untracked): the status
            // block is exactly 2 chars + 1 space; renames have " -> ".
            let drift: Vec<&str> = dirty
                .lines()
                .map(|l| {
                    let after_status = if l.len() > 3 { l[3..].trim() } else { "" };
                    match after_status.split_once(" -> ") {
                        Some((_, new_path)) => new_path.trim(),
                        None => after_status,
                    }
                })
                .filter(|p| !p.is_empty() && !kernel_owned.iter().any(|k| p.starts_with(k)))
                .collect();
            if drift.is_empty() {
                report
                    .passed
                    .push("working tree: clean (kernel-owned artifacts excluded)".into());
            } else {
                report.findings.push(Finding {
                    severity: "warn".into(),
                    message: format!(
                        "working tree: {} uncommitted source/config change(s) (first: {})",
                        drift.len(),
                        drift.first().unwrap_or(&"")
                    ),
                });
            }
        }
        _ => report
            .passed
            .push("working tree: git unavailable — skipped".into()),
    }

    // 4. The eval gate itself.
    let golden = root.join("evals/golden");
    match crate::eval::score_all_cases(&cases_dir, root, &golden) {
        Ok(entries) => {
            let baseline: Vec<crate::eval::GateEntry> = fs::read_to_string(baseline_path_for(root))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            let result = crate::eval::run_gate(&entries, &baseline, 0.05, 1);
            if result.failures == 0 {
                report.passed.push(format!(
                    "eval gate: PASS — {} cases, 0 regressions",
                    result.case_count
                ));
            } else {
                report.findings.push(Finding {
                    severity: "fail".into(),
                    message: format!(
                        "eval gate: {} regressions across {} cases",
                        result.failures, result.case_count
                    ),
                });
            }
        }
        Err(_) => report
            .passed
            .push("eval gate: no scoreable cases — skipped".into()),
    }

    // 5. Memory-load validation (Phase 9 slice 6, Anthropic
    // containment): persistent memory is a post-exploitation vector —
    // scan canonical/derived fact bodies for suspicious patterns
    // (machine-specific absolute paths in actions, injection markers).
    let memory_dirs = [
        root.join("memory/canonical/entries"),
        root.join("memory/derived"),
    ];
    let suspicious: &[&str] = &["/home/", "/Users/", "; rm ", "eval("];
    for dir in memory_dirs {
        if !dir.is_dir() {
            continue;
        }
        for path in walk_md(&dir) {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            for (i, line) in text.lines().enumerate() {
                if line.trim_start().starts_with('#') {
                    continue;
                }
                if suspicious.iter().any(|s| line.contains(s)) {
                    report.findings.push(Finding {
                        severity: "warn".into(),
                        message: format!(
                            "memory-load: {}:{} contains suspicious pattern ({})",
                            path.display(),
                            i + 1,
                            suspicious
                                .iter()
                                .find(|s| line.contains(**s))
                                .unwrap_or(&"")
                        ),
                    });
                }
            }
        }
    }

    // 5b. Calibration integrity (codex review): verified rows must
    // carry command/target evidence; impossible rows are a signal.
    let calibration = crate::verifier::read_calibration(root).unwrap_or_default();
    let impossible: Vec<&crate::verifier::CalibrationRow> = calibration
        .iter()
        .filter(|r| r.status == "verified" && (r.command.is_none() || r.target.is_none()))
        .collect();
    if !impossible.is_empty() {
        report.findings.push(Finding {
            severity: "warn".into(),
            message: format!(
                "calibration integrity: {} verified row(s) without command/target evidence",
                impossible.len()
            ),
        });
    }

    // 6. Verifier attribution (Phase 9 slice 6, NIST audit trail): the
    // last executed verifier commands — audit attribution for commands
    // the kernel has run in target repos.
    let attribution_path = root.join("memory/episodic/verify.log");
    let attribution = match crate::verifier::read_attribution(root) {
        Ok(rows) => rows,
        Err(_) if !attribution_path.exists() => Vec::new(),
        Err(e) => {
            report.findings.push(Finding {
                severity: "fail".into(),
                message: format!(
                    "attribution: verify.log exists but is unreadable/malformed — {e}"
                ),
            });
            Vec::new()
        }
    };
    if attribution.is_empty() {
        report
            .passed
            .push("attribution: no verifier commands executed yet".into());
    } else {
        report.passed.push(format!(
            "attribution: {} verifier command(s) executed (see memory/episodic/verify.log)",
            attribution.len()
        ));
        for row in attribution.iter().rev().take(3).rev() {
            report.passed.push(format!(
                "  {} | {} | {} | {}",
                row.at, row.case, row.status, row.command
            ));
        }
    }

    // 7. Comprehensive action log (production-readiness D.1): an
    // append-only, timestamped, principal + content-hash record of every
    // kernel action. Rows are validated for shape; a malformed line is a
    // finding (tampering or a bug must not go silent).
    let actions_path = root.join(ACTIONS_LOG_REL);
    match fs::read_to_string(&actions_path) {
        Ok(text) => {
            let rows: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            let malformed = rows.iter().filter(|l| !is_valid_action_row(l)).count();
            report.passed.push(format!(
                "action log: {} action(s) recorded ({} malformed)",
                rows.len(),
                malformed
            ));
            if malformed > 0 {
                report.findings.push(Finding {
                    severity: "warn".into(),
                    message: format!(
                        "{malformed} malformed action-log row(s) in {ACTIONS_LOG_REL}"
                    ),
                });
            }
        }
        Err(_) if !actions_path.exists() => report
            .passed
            .push("action log: no kernel actions recorded yet".into()),
        Err(e) => {
            report.findings.push(Finding {
                severity: "fail".into(),
                message: format!("action log: {ACTIONS_LOG_REL} exists but is unreadable — {e}"),
            });
        }
    }

    Ok(report)
}

/// Relative path of the comprehensive action log (production-readiness
/// D.1). Append-only; rows are `<utc-stamp> <principal> <action>
/// <content-hash> <detail>`.
pub const ACTIONS_LOG_REL: &str = "memory/episodic/actions.log";

/// Append one kernel action row (best-effort for callers: a log write
/// failure must never break the action itself). Content hash binds the
/// row to its inputs so tampering is detectable.
pub fn append_action(
    root: &Path,
    action: &str,
    principal: &str,
    detail: &str,
) -> std::io::Result<()> {
    use std::io::Write;
    let path = root.join(ACTIONS_LOG_REL);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let stamp = crate::memory::utc_now_stamp();
    let hash = crate::hash::source_sha256(&format!("{stamp}|{action}|{principal}|{detail}"));
    let row = format!("{stamp} {principal} {action} {hash} {detail}\n");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    f.write_all(row.as_bytes())
}

/// Validate one action-log row shape: `20-char-stamp principal action
/// 16-hex content-hash detail`.
fn is_valid_action_row(row: &str) -> bool {
    let mut fields = row.split_whitespace();
    let stamp = fields.next();
    let principal = fields.next();
    let action = fields.next();
    let hash = fields.next();
    let detail = fields.next();
    matches!(stamp, Some(s) if s.len() == 20 && s.ends_with('Z'))
        && principal.is_some_and(|p| !p.is_empty())
        && action.is_some_and(|a| !a.is_empty())
        && hash.is_some_and(|h| h.len() == 16 && h.chars().all(|c| c.is_ascii_hexdigit()))
        && detail.is_some_and(|d| !d.is_empty())
}

fn walk_md(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_md(&path));
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    out
}

fn baseline_path_for(root: &Path) -> std::path::PathBuf {
    root.join("evals/results/baseline.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = env::temp_dir().join(format!("mag-audit-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn audit_flags_stale_baseline_and_clean_tree() {
        let root = tmp_root("stale");
        // One case, empty baseline -> baseline-stale warn.
        fs::create_dir_all(root.join("evals/cases/weak")).unwrap();
        fs::write(
            root.join("evals/cases/weak/run.json"),
            r#"{"goal":"weak","scope":["x"],"outcome":{"achieved":false},"tokens_total":1,"cost_usd":0.01,"golden":null,"trajectory":[{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("evals/results")).unwrap();
        fs::write(root.join("evals/results/baseline.json"), "[]").unwrap();
        let report = audit(&root).unwrap();
        assert!(report.verdict() == "WARN" || report.verdict() == "FAIL");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.message.contains("baseline stale")),
            "{:?}",
            report.findings
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn audit_passes_on_a_healthy_layout() {
        let root = tmp_root("clean");
        fs::create_dir_all(root.join("memory/canonical/entries")).unwrap();
        fs::create_dir_all(root.join("memory/derived")).unwrap();
        fs::create_dir_all(root.join("evals/cases")).unwrap();
        fs::create_dir_all(root.join("evals/results")).unwrap();
        fs::write(root.join("evals/results/baseline.json"), "[]").unwrap();
        let report = audit(&root).unwrap();
        assert_eq!(report.verdict(), "OK", "{:?}", report.findings);
        assert!(!report.passed.is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod memory_load_tests {
    use super::*;
    use std::env;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = env::temp_dir().join(format!("mag-ml-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn audit_flags_absolute_paths_in_fact_bodies() {
        let root = tmp_root("abs");
        fs::create_dir_all(root.join("memory/canonical/entries/2026-08-03")).unwrap();
        fs::write(
            root.join("memory/canonical/entries/2026-08-03/2026-08-03-001.md"),
            "# Canonical entry\n\n- domain: eval\n\n## F-000 `aa`\n\nexec wrote /home/krn/proj/x.py\n",
        )
        .unwrap();
        let report = audit(&root).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.message.contains("memory-load") && f.message.contains("/home/")),
            "{:?}",
            report.findings
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn attribution_log_reported_by_audit() {
        let root = tmp_root("attr");
        fs::create_dir_all(root.join("memory/episodic")).unwrap();
        crate::verifier::append_attribution(
            &root,
            &crate::verifier::VerifyAttribution {
                at: "2026-08-03T00:00:00Z".into(),
                case: "case-a".into(),
                command: "make verify".into(),
                target: "/tmp/t".into(),
                status: "verified".into(),
            },
        )
        .unwrap();
        let report = audit(&root).unwrap();
        assert!(
            report
                .passed
                .iter()
                .any(|p| p.contains("attribution: 1 verifier command(s) executed")),
            "{:?}",
            report.passed
        );
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod action_log_tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-actions-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn append_action_writes_valid_rows_and_audit_accepts_them() {
        let root = tmp_root("valid");
        append_action(&root, "loop-verify", "local", "case-a").unwrap();
        append_action(&root, "run-ingest", "kernel", "case-b").unwrap();
        let text = std::fs::read_to_string(root.join(ACTIONS_LOG_REL)).unwrap();
        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|l| is_valid_action_row(l)), "{text}");
        // The audit surfaces them as passed, not findings.
        let report = audit(&root).unwrap();
        assert!(
            report
                .passed
                .iter()
                .any(|p| p.contains("2 action(s) recorded")),
            "{:?}",
            report.passed
        );
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn audit_flags_malformed_action_rows() {
        let root = tmp_root("malformed");
        std::fs::create_dir_all(root.join("memory/episodic")).unwrap();
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join(ACTIONS_LOG_REL))
            .unwrap();
        f.write_all(b"THIS IS NOT A VALID ROW\n").unwrap();
        let report = audit(&root).unwrap();
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.message.contains("malformed action-log row")),
            "{:?}",
            report.findings
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
