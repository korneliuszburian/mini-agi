//! Verifiable reward layer (Phase 8, slice 1, ADR-0011).
//!
//! The kernel stops trusting self-reported outcomes: when a run declares
//! `verify_command` + `verify_target`, `run verify` executes the command
//! in the target repo and reports one of:
//!
//! - `verified` — deterministic gate passed AND the run claims achieved;
//! - `disagrees` — gate failed while the run claims achieved (or the
//!   reverse): a judge-calibration signal, and `loop verify` refuses to
//!   close the gap;
//! - `unverified` — no deterministic verifier declared (outcome is the
//!   agent's own claim only).
//!
//! Trust boundary: the kernel executes `verify_command` ONLY on explicit
//! `run verify` / `loop verify` invocation, never during score/gate
//! (which stay pure). Runs are trusted eval-corpus documents.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// Timeout guard (Phase 9 slice 1): a hung gate must not block the loop
/// forever — 120s then kill and report as disagreement.
const VERIFY_TIMEOUT_SECS: u64 = 120;

/// Outcome of the deterministic verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    /// Case name (parent dir of the run file).
    pub case: String,
    /// `verified` | `disagrees` | `unverified`.
    pub status: String,
    /// Command executed (when declared).
    pub command: Option<String>,
    /// Target repo where it ran.
    pub target: Option<String>,
    /// Exit code of the verifier (when executed).
    pub exit_code: Option<i32>,
    /// Whether the run claims `achieved`.
    pub claimed: bool,
    /// Last line of the verifier output (excerpt).
    pub output_excerpt: String,
}

/// Verify one run file: execute its declared gate in its target repo.
///
/// # Errors
///
/// Returns a message when the run file is missing/malformed or the
/// verifier cannot be executed.
pub fn verify_run(root: &Path, run_path: &Path) -> Result<Verification, String> {
    let text = fs::read_to_string(run_path)
        .map_err(|e| format!("cannot read {}: {e}", run_path.display()))?;
    let run: crate::eval::Run =
        serde_json::from_str(&text).map_err(|e| format!("invalid run json: {e}"))?;
    let case = run_path.parent().and_then(|p| p.file_name()).map_or_else(
        || "unknown".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let (Some(command), Some(target)) = (run.verify_command.clone(), run.verify_target.clone())
    else {
        return Ok(Verification {
            case,
            status: "unverified".into(),
            command: None,
            target: None,
            exit_code: None,
            claimed: run.outcome.achieved,
            output_excerpt: "no deterministic verifier declared (outcome is the run's own claim)"
                .into(),
        });
    };
    let target_path = Path::new(&target);
    let target_path = if target_path.is_absolute() {
        target_path.to_path_buf()
    } else {
        root.join(target_path)
    };
    if !target_path.is_dir() {
        return Err(format!(
            "verify target {} is not a directory",
            target_path.display()
        ));
    }
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(&target_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot execute verifier in {}: {e}", target_path.display()))?;
    let mut timed_out = false;
    let started = std::time::Instant::now();
    let output = loop {
        match child.try_wait() {
            Ok(Some(_)) => break child.wait_with_output(),
            Ok(None) => {
                if started.elapsed().as_secs() > VERIFY_TIMEOUT_SECS {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break Ok(std::process::Output {
                        status: std::process::ExitStatus::default(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    });
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => break Err(e),
        }
    }
    .map_err(|e| format!("verifier failed: {e}"))?;
    if timed_out {
        return Ok(Verification {
            case,
            status: "disagrees".into(),
            command: Some(command),
            target: Some(target),
            exit_code: None,
            claimed: run.outcome.achieved,
            output_excerpt: "verifier timed out (>120s) — treated as disagreement".into(),
        });
    }
    let exit_code = output.status.code();
    let excerpt = String::from_utf8_lossy(&output.stderr)
        .lines()
        .chain(String::from_utf8_lossy(&output.stdout).lines())
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(160)
        .collect();
    let verifier_pass = output.status.success();
    let claims_achieved = run.outcome.achieved;
    let status = if verifier_pass == claims_achieved {
        if verifier_pass {
            "verified"
        } else {
            "verified-failed"
        }
    } else {
        "disagrees"
    }
    .to_string();
    let verification = Verification {
        case,
        status,
        command: Some(command),
        target: Some(target),
        exit_code,
        claimed: run.outcome.achieved,
        output_excerpt: excerpt,
    };
    // Comprehensive action log (production-readiness D.1).
    let _ = crate::audit::append_action(root, "run-verify", "kernel", &verification.case);
    Ok(verification)
}

/// Verifier-strength audit (VERIFIABLE-REWARD-RESEARCH D).
///
/// Checks the declared `verify_command` is not VACUOUS: it must PASS on
/// the real `verify_target` (known-good work) and FAIL on an empty
/// counterfactual target. A verifier that "passes" an empty directory
/// accepts non-work and is a fake gate — the literature's core finding
/// is that the TEST SUITE, not the model, is where "verified" goes
/// wrong.
///
/// # Errors
///
/// Returns a message when the run cannot be read/parsed or the verifier
/// cannot be started.
pub fn audit_verifier(run_path: &Path) -> Result<String, String> {
    let run: crate::eval::Run = serde_json::from_str(
        &std::fs::read_to_string(run_path)
            .map_err(|e| format!("cannot read {}: {e}", run_path.display()))?,
    )
    .map_err(|e| format!("invalid run json: {e}"))?;
    let command = run
        .verify_command
        .as_deref()
        .ok_or("verify-audit: the run declares no verify_command")?;
    let target = run
        .verify_target
        .as_deref()
        .ok_or("verify-audit: the run declares no verify_target")?;
    let target_path = std::path::Path::new(target);

    // (a) Gold check: the verifier must pass on the real target.
    let gold = crate::worker::run_capped("sh", &["-c", command], target_path, Some(120))
        .map_err(|e| format!("cannot start verifier: {e}"))?;
    let gold_ok = !gold.aborted && gold.status == Some(0);

    // (b) Counterfactual check: the verifier must FAIL on an empty dir
    // (no deliverables -> the gate must reject non-work).
    let tmp = std::env::temp_dir().join(format!("mag-va-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap_or(());
    let cf = crate::worker::run_capped("sh", &["-c", command], &tmp, Some(120))
        .map_err(|e| format!("cannot start verifier on the counterfactual: {e}"))?;
    let _ = std::fs::remove_dir_all(&tmp);
    let cf_ok = !cf.aborted && cf.status == Some(0);

    let _ = crate::audit::append_action(
        target_path,
        "verify-audit",
        "kernel",
        run_path.to_string_lossy().as_ref(),
    );
    let mut out = String::new();
    if !gold_ok {
        out.push_str("verify-audit: GOLD FAILED — the verifier does not pass on the known-good target (FPR: rejects real work)\n");
    }
    if cf_ok {
        out.push_str("verify-audit: VACUOUS — the verifier PASSES on an empty target (FNR: accepts non-work); the gate is fake\n");
    }
    if gold_ok && !cf_ok {
        out.push_str(
            "verify-audit: PASS — the verifier rejects empty work and accepts the real target\n",
        );
    }
    let _ = std::fmt::write(
        &mut out,
        format_args!(
            "  gold: target {target} -> {}; counterfactual (empty dir) -> {}",
            if gold_ok { "PASS" } else { "FAIL" },
            if cf_ok {
                "PASS (vacuous)"
            } else {
                "FAIL (non-vacuous)"
            }
        ),
    );
    Ok(out)
}

// Process-wide uniqueness counter for the vacuous-audit temp dirs: a
// timestamp can collide across concurrent calls (same-nanosecond reads),
// and a fixed name made concurrent audits stomp each other's cwd.
static AUDIT_DIR_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Verifier-strength counterfactual check (S2, wired into --iterate):
/// run the verifier in an EMPTY dir — if it exits 0 there, it accepts
/// non-work and the iteration would 'pass' garbage.
/// # Errors
///
/// Returns an error when the counterfactual directory cannot be set up
/// (the audit refuses to trust a verifier it could not test).
pub fn audit_verifier_vacuous(verify_command: &str) -> Result<VerifierVacuousAudit, String> {
    let base = std::env::temp_dir().join(format!("mag-va2-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&base);
    let nonce = AUDIT_DIR_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = base.join(format!("{nonce}"));
    if std::fs::create_dir(&tmp).is_err() {
        return Err(format!(
            "vacuous-audit setup failed (cannot create {}) — refusing to trust a verifier without the counterfactual",
            tmp.display()
        ));
    }
    let res = crate::worker::run_capped("sh", &["-c", verify_command], &tmp, Some(120))
        .map_err(|e| {
            format!(
                "vacuous-audit could not RUN the verifier ({e}) — refusing to trust a verifier that failed to execute"
            )
        })?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(VerifierVacuousAudit {
        is_vacuous: !res.aborted && res.status == Some(0),
    })
}

/// Result of the vacuous-verifier counterfactual check.
#[derive(Debug, Clone, Copy)]
pub struct VerifierVacuousAudit {
    /// True when the verifier passes an EMPTY target (accepts non-work).
    pub is_vacuous: bool,
}

/// Judge-calibration record (Phase 9 slice 2): one JSON line per
/// verification, appended to `memory/derived/calibration.md` — the
/// verifier-vs-judged disagreement corpus. Never hand-edited.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CalibrationRow {
    /// ISO timestamp.
    pub at: String,
    /// Case name.
    pub case: String,
    /// `verified` | `disagrees` | `unverified`.
    pub status: String,
    /// Whether the run claimed achieved.
    pub claimed: bool,
    /// Composite of the run (0.0 when not scored).
    pub composite: f64,
    /// Verifier exit code.
    pub exit: Option<i32>,
    /// Verifier command (for dedup of repeated re-verifications).
    #[serde(default)]
    pub command: Option<String>,
    /// Verifier target repo (for dedup).
    #[serde(default)]
    pub target: Option<String>,
}

/// Append one calibration row.
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn append_calibration(root: &Path, row: &CalibrationRow) -> std::io::Result<()> {
    let path = root.join("memory/derived/calibration.md");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    if path.exists() {
        out.push_str(&fs::read_to_string(&path)?);
    } else {
        out.push_str("# JUDGE CALIBRATION (derived — appended by run verify / loop verify, never hand-edit)\n");
    }
    // Dedup by (case, command, target) keeping the latest row — repeated
    // re-verification of the same run must not inflate the corpus (codex
    // review).
    let mut kept: Vec<CalibrationRow> = read_calibration(root).unwrap_or_default();
    kept.retain(|r| !(r.case == row.case && r.command == row.command && r.target == row.target));
    kept.push(row.clone());
    let lines: Vec<String> = kept
        .iter()
        .map(|r| serde_json::to_string(r).map_err(std::io::Error::other))
        .collect::<std::io::Result<Vec<_>>>()?;
    out.clear();
    out.push_str(
        "# JUDGE CALIBRATION (derived — appended by run verify / loop verify, never hand-edit)\n",
    );
    out.push_str(&lines.join("\n"));
    out.push('\n');
    fs::write(&path, out)
}

/// Reset the judge-calibration corpus to an empty header, used by
/// `eval judge-recalibrate` so the judge-abstention gate can resume.
///
/// A stale disagreement row must not permanently freeze every loop close
/// after the verifier/judge is fixed.
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn reset_calibration(root: &Path) -> std::io::Result<()> {
    let path = root.join("memory/derived/calibration.md");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        "# JUDGE CALIBRATION (derived — appended by run verify / loop verify, never hand-edit)\n",
    )
}

/// Attribution log (Phase 9 slice 6, NIST audit-attribution): one line
/// per executed verifier command appended to
/// `memory/episodic/verify.log`. `run verify --dry-run` skips this.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerifyAttribution {
    /// ISO timestamp.
    pub at: String,
    /// Case name.
    pub case: String,
    /// The executed command.
    pub command: String,
    /// Target repo.
    pub target: String,
    /// Verifier status.
    pub status: String,
}

/// Append one attribution row (JSON line).
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn append_attribution(root: &Path, row: &VerifyAttribution) -> std::io::Result<()> {
    let path = root.join("memory/episodic/verify.log");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(row).map_err(std::io::Error::other)?;
    let mut out = String::new();
    if path.exists() {
        out.push_str(&fs::read_to_string(&path)?);
    }
    out.push_str(&line);
    out.push('\n');
    fs::write(&path, out)
}

/// Read the attribution log (absent file = empty).
///
/// # Errors
///
/// Returns the underlying filesystem error on unreadable files.
pub fn read_attribution(root: &Path) -> std::io::Result<Vec<VerifyAttribution>> {
    let text = fs::read_to_string(root.join("memory/episodic/verify.log"))?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(row) = serde_json::from_str::<VerifyAttribution>(line) {
            out.push(row);
        }
    }
    Ok(out)
}

/// Read the calibration corpus (absent file = empty).
///
/// # Errors
///
/// Returns the underlying filesystem error on unreadable files.
pub fn read_calibration(root: &Path) -> std::io::Result<Vec<CalibrationRow>> {
    let text = fs::read_to_string(root.join("memory/derived/calibration.md"))?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Ok(row) = serde_json::from_str::<CalibrationRow>(line) {
            out.push(row);
        }
    }
    Ok(out)
}

/// Verifier-vs-judged drift statistics (Phase 9 slice 2).
#[derive(Debug, Default)]
pub struct JudgeDrift {
    /// Total verifications recorded.
    pub total: usize,
    /// Runs claiming achieved.
    pub claimed_successes: usize,
    /// Claimed successes the verifier confirmed.
    pub verified_successes: usize,
    /// Disagreements (verifier and claim differ).
    pub disagreements: usize,
    /// Cases with a recorded verifier-vs-judge disagreement — the
    /// first-class red-team signal (VERIFIABLE-REWARD-RESEARCH D).
    pub disagreement_cases: Vec<String>,
}

impl JudgeDrift {
    /// Precision of the judged outcome against the verifier:
    /// verified-successes / conclusive-claimed-successes, where
    /// conclusive = verified or disagreed (an `unverified` claim has no
    /// verifier verdict and must not dilute the denominator — codex
    /// review).
    #[must_use]
    pub fn precision(&self) -> f64 {
        let conclusive = self.verified_successes + self.disagreements;
        if conclusive == 0 {
            f64::NAN
        } else {
            f64::from(u32::try_from(self.verified_successes).unwrap_or(0))
                / f64::from(u32::try_from(conclusive).unwrap_or(0))
        }
    }
}

/// Compute drift statistics over the calibration corpus.
#[must_use]
pub fn judge_drift(root: &Path) -> JudgeDrift {
    let mut drift = JudgeDrift::default();
    for row in read_calibration(root).unwrap_or_default() {
        drift.total += 1;
        if row.status == "disagrees" {
            // The case always surfaces as a red-team signal (the spec
            // warns when a prior verifier-vs-judge disagreement exists),
            // but the PRECISION denominator counts only CLAIMED
            // disagreements: a `claimed=false` row where the verifier
            // also disagrees is safe-direction (the run honestly
            // reported failure) — including it would drive precision to
            // 0 and block every loop close for a judge that never
            // overstated (cycle-33 review F2).
            if !drift.disagreement_cases.contains(&row.case) {
                drift.disagreement_cases.push(row.case.clone());
            }
            if row.claimed {
                drift.disagreements += 1;
            }
        }
        if row.claimed {
            drift.claimed_successes += 1;
            if row.status == "verified" {
                drift.verified_successes += 1;
            }
        }
    }
    drift
}

/// Cases with a recorded verifier-vs-judge disagreement (red-team
/// signal): a `loop dispatch` for such a case must warn in its spec.
#[must_use]
pub fn disagreement_cases(root: &Path) -> Vec<String> {
    judge_drift(root).disagreement_cases
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = env::temp_dir().join(format!("mag-verify-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_run(
        root: &std::path::Path,
        case: &str,
        achieved: bool,
        command: Option<&str>,
        target: Option<&str>,
    ) -> std::path::PathBuf {
        let dir = root.join("evals").join("cases").join(case);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("run.json");
        let run = serde_json::json!({
            "goal": "g",
            "scope": ["x"],
            "outcome": {"achieved": achieved},
            "tokens_total": 1,
            "cost_usd": 0.01,
            "golden": null,
            "verify_command": command,
            "verify_target": target,
            "trajectory": [{"step": 1, "tool": "read", "ok": true, "goal_aligned": true, "tokens": 1, "output_tokens": 1}],
        });
        fs::write(&path, serde_json::to_string(&run).unwrap()).unwrap();
        path
    }

    #[test]
    fn verifier_agrees_with_achieved_run() {
        let root = tmp_root("ok");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("ok.sh"), "#!/bin/sh\nexit 0\n").unwrap();
        let run = write_run(
            &root,
            "case-ok",
            true,
            Some("sh ok.sh"),
            Some(target.to_str().unwrap()),
        );
        let v = verify_run(&root, &run).unwrap();
        assert_eq!(v.status, "verified", "{v:?}");
        assert_eq!(v.exit_code, Some(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verifier_disagrees_when_gate_fails_but_run_claims_achieved() {
        let root = tmp_root("disagree");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("fail.sh"), "#!/bin/sh\necho broken\nexit 1\n").unwrap();
        let run = write_run(
            &root,
            "case-bad",
            true,
            Some("sh fail.sh"),
            Some(target.to_str().unwrap()),
        );
        let v = verify_run(&root, &run).unwrap();
        assert_eq!(v.status, "disagrees", "{v:?}");
        assert!(v.output_excerpt.contains("broken"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_declared_verifier_is_unverified() {
        let root = tmp_root("none");
        let run = write_run(&root, "case-plain", true, None, None);
        let v = verify_run(&root, &run).unwrap();
        assert_eq!(v.status, "unverified", "{v:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verifier_passes_but_run_claims_failure_is_also_disagreement() {
        let root = tmp_root("rev");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("ok.sh"), "#!/bin/sh\nexit 0\n").unwrap();
        let run = write_run(
            &root,
            "case-rev",
            false,
            Some("sh ok.sh"),
            Some(target.to_str().unwrap()),
        );
        let v = verify_run(&root, &run).unwrap();
        assert_eq!(v.status, "disagrees", "{v:?}");
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod calibration_tests {
    use super::*;
    use std::env;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = env::temp_dir().join(format!("mag-cal-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn row(case: &str, status: &str, claimed: bool) -> CalibrationRow {
        CalibrationRow {
            at: "2026-08-03T00:00:00Z".into(),
            case: case.into(),
            status: status.into(),
            claimed,
            composite: 0.5,
            exit: Some(0),
            command: Some("make verify".into()),
            target: Some("/tmp/t".into()),
        }
    }

    #[test]
    fn calibration_roundtrip_and_drift_stats() {
        let root = tmp_root("stats");
        append_calibration(&root, &row("a", "verified", true)).unwrap();
        append_calibration(&root, &row("b", "disagrees", true)).unwrap();
        append_calibration(&root, &row("c", "unverified", true)).unwrap();
        let read = read_calibration(&root).unwrap();
        assert_eq!(read.len(), 3);
        let drift = judge_drift(&root);
        assert_eq!(drift.total, 3);
        assert_eq!(drift.claimed_successes, 3);
        assert_eq!(drift.verified_successes, 1);
        assert_eq!(drift.disagreements, 1);
        // Precision excludes the unverified claim from the denominator.
        assert!((drift.precision() - 1.0 / 2.0).abs() < 1e-9);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn safe_direction_disagreement_does_not_dilute_precision() {
        // Cycle-33 review F2: a row where the run honestly claimed
        // failure (claimed=false) and the verifier also disagrees is
        // SAFE direction — the judge never overstated success. It must
        // not count as a disagreement (which would drive precision to 0
        // and block every loop close).
        let root = tmp_root("safe-dir");
        append_calibration(&root, &row("a", "verified", true)).unwrap();
        append_calibration(&root, &row("b", "disagrees", false)).unwrap();
        let drift = judge_drift(&root);
        assert_eq!(drift.claimed_successes, 1);
        assert_eq!(drift.verified_successes, 1);
        assert_eq!(drift.disagreements, 0, "safe-direction row must not count");
        assert!(
            (drift.precision() - 1.0).abs() < 1e-9,
            "precision stays 1.0"
        );
        // The case still surfaces as a disagreement case for the spec
        // red-team signal (dispatch warns), even though it is not a
        // precision dilution.
        assert!(drift.disagreement_cases.iter().any(|c| c == "b"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn calibration_dedups_same_run_reverification() {
        let root = tmp_root("dedup");
        append_calibration(&root, &row("a", "verified", true)).unwrap();
        append_calibration(&root, &row("a", "verified", true)).unwrap();
        assert_eq!(read_calibration(&root).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reset_calibration_clears_corpus_and_drift() {
        let root = tmp_root("reset");
        append_calibration(&root, &row("a", "disagrees", true)).unwrap();
        assert_eq!(judge_drift(&root).disagreements, 1);
        reset_calibration(&root).unwrap();
        assert!(read_calibration(&root).unwrap().is_empty());
        let drift = judge_drift(&root);
        assert_eq!(drift.total, 0);
        assert_eq!(drift.disagreements, 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_corpus_is_zero_drift() {
        let root = tmp_root("empty");
        let drift = judge_drift(&root);
        assert_eq!(drift.total, 0);
        assert!(drift.precision().is_nan());
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod verify_audit_tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-va-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn run_with(command: &str, target: &Path) -> PathBuf {
        std::fs::create_dir_all(target).unwrap();
        let path = target.join("run.json");
        std::fs::write(
            &path,
            format!(
                r#"{{"goal":"g","scope":["x"],"outcome":{{"achieved":true}},"tokens_total":1,"cost_usd":0.0,"golden":null,"verify_command":"{command}","verify_target":"{}","trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}}]}}"#,
                target.to_string_lossy()
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn vacuous_verifier_is_flagged() {
        // A verifier that ALWAYS exits 0 accepts empty work -> VACUOUS.
        let root = tmp_root("vacuous");
        let target = root.join("target");
        let run = run_with("true", &target);
        let text = audit_verifier(&run).unwrap();
        assert!(text.contains("VACUOUS"), "{text}");
        assert!(text.contains("gate is fake"), "{text}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn non_vacuous_verifier_passes() {
        // A verifier that passes only when the deliverable exists ->
        // gold passes (file present), counterfactual fails (empty) ->
        // PASS.
        let root = tmp_root("good");
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("x.txt"), "deliverable").unwrap();
        let run = run_with("sh -c 'test -f x.txt'", &target);
        let text = audit_verifier(&run).unwrap();
        assert!(text.contains("PASS"), "{text}");
        assert!(!text.contains("VACUOUS"), "{text}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_verifier_errors() {
        let root = tmp_root("nover");
        let path = root.join("run.json");
        std::fs::write(
            &path,
            r#"{"goal":"g","scope":[],"outcome":{"achieved":true},"tokens_total":1,"cost_usd":0.0,"golden":null,"trajectory":[{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        let err = audit_verifier(&path).unwrap_err();
        assert!(err.contains("verify_command"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod judge_drift_signal_tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-jds-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn disagreement_case_surfaces_in_judge_drift() {
        // Red-team signal: a disagreement row names its case.
        let root = tmp_root("signal");
        append_calibration(
            &root,
            &CalibrationRow {
                at: "2026-08-04T00:00:00Z".into(),
                case: "case-x".into(),
                status: "disagrees".into(),
                claimed: true,
                composite: 0.9,
                exit: Some(1),
                command: Some("sh v.sh".into()),
                target: Some("/tmp/x".into()),
            },
        )
        .unwrap();
        let drift = judge_drift(&root);
        assert_eq!(drift.disagreements, 1);
        assert!(
            drift.disagreement_cases.iter().any(|c| c == "case-x"),
            "{:?}",
            drift.disagreement_cases
        );
        assert!(disagreement_cases(&root).iter().any(|c| c == "case-x"));
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod vacuous_audit_tests {
    use super::*;
    use std::env;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = env::temp_dir().join(format!("mag-vac-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn always_true_verifier_is_vacuous() {
        // NOTE: must NOT use the mag-va2-<pid> prefix — that is the
        // production audit's OWN base dir; a remove_dir_all here races
        // concurrent audits (their tmp dirs vanish -> spawn fails).
        let root = tmp_root("va-always-true");
        let audit = audit_verifier_vacuous("true").unwrap();
        assert!(
            audit.is_vacuous,
            "a verifier that passes empty work is vacuous"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn concurrent_audits_do_not_stomp_each_other() {
        // The fixed-name race: parallel audits sharing one dir made the
        // verifier's cwd vanish mid-run. Atomic unique dirs must keep
        // every concurrent audit independent.
        let root = tmp_root("va-conc");
        let mut handles = Vec::new();
        for _ in 0..8 {
            handles.push(std::thread::spawn(move || {
                let audit = audit_verifier_vacuous("true").unwrap();
                assert!(audit.is_vacuous, "a 'true' verifier is always vacuous");
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn failing_verifier_is_not_vacuous() {
        let root = std::env::temp_dir().join(format!("mag-va2c-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let audit = audit_verifier_vacuous("sh -c 'test -f x.txt'").unwrap();
        assert!(!audit.is_vacuous, "an empty dir must fail this verifier");
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod attribution_tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-attr-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn attribution_roundtrip_and_persists() {
        // verify.log (the audit's attribution block) had no direct test;
        // append + read roundtrip must preserve the row and the file must
        // live at memory/episodic/verify.log.
        let root = tmp_root("round");
        let row = VerifyAttribution {
            at: "2026-08-04T00:00:00Z".into(),
            case: "case-x".into(),
            command: "sh v.sh".into(),
            target: "/tmp/x".into(),
            status: "verified".into(),
        };
        append_attribution(&root, &row).unwrap();
        assert!(root.join("memory/episodic/verify.log").is_file());
        let rows = read_attribution(&root).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].case, "case-x");
        assert_eq!(rows[0].status, "verified");
        let _ = std::fs::remove_dir_all(&root);
    }
}
