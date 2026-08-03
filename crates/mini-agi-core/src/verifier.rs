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
        output_excerpt: excerpt,
    })
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
