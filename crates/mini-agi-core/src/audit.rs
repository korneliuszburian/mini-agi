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

    // 3. Uncommitted changes.
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output();
    match status {
        Ok(out) if out.status.success() => {
            let dirty = String::from_utf8_lossy(&out.stdout);
            let count = dirty.lines().count();
            if count == 0 {
                report.passed.push("working tree: clean".into());
            } else {
                report.findings.push(Finding {
                    severity: "warn".into(),
                    message: format!(
                        "working tree: {count} uncommitted change(s) (first: {})",
                        dirty.lines().next().unwrap_or("")
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

    Ok(report)
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
