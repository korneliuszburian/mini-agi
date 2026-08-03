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
                    break Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("verifier exceeded {VERIFY_TIMEOUT_SECS}s"),
                    ));
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
    Ok(Verification {
        case,
        status,
        command: Some(command),
        target: Some(target),
        exit_code,
        claimed: run.outcome.achieved,
        output_excerpt: excerpt,
    })
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
    let line = serde_json::to_string(row).map_err(std::io::Error::other)?;
    let mut out = String::new();
    if path.exists() {
        out.push_str(&fs::read_to_string(&path)?);
    } else {
        out.push_str("# JUDGE CALIBRATION (derived — appended by run verify / loop verify, never hand-edit)\n");
    }
    out.push_str(&line);
    out.push('\n');
    fs::write(&path, out)
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
}

impl JudgeDrift {
    /// Precision of the judged outcome against the verifier:
    /// verified-successes / claimed-successes. NaN when none claimed.
    #[must_use]
    pub fn precision(&self) -> f64 {
        if self.claimed_successes == 0 {
            f64::NAN
        } else {
            f64::from(u32::try_from(self.verified_successes).unwrap_or(0))
                / f64::from(u32::try_from(self.claimed_successes).unwrap_or(0))
        }
    }
}

/// Compute drift statistics over the calibration corpus.
#[must_use]
pub fn judge_drift(root: &Path) -> JudgeDrift {
    let mut drift = JudgeDrift::default();
    for row in read_calibration(root).unwrap_or_default() {
        drift.total += 1;
        if row.claimed {
            drift.claimed_successes += 1;
            if row.status == "verified" {
                drift.verified_successes += 1;
            }
        }
        if row.status == "disagrees" {
            drift.disagreements += 1;
        }
    }
    drift
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
        assert!((drift.precision() - 1.0 / 3.0).abs() < 1e-9);
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
