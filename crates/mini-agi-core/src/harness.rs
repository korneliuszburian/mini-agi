//! Harness evolution (Phase 8 slice 7, RHI 2607.15524): the harness
//! spec is data — versioned snapshots + a gate-verdict ledger.
//!
//! A revision is accepted only when the frozen-suite gate passes with
//! it (pairwise eval); the checkpoint journal is the evolution ledger.

use std::fs;
use std::path::Path;

/// The harness spec snapshot: the constants and loop semantics that
/// define how the kernel evaluates work. Bump intentionally.
#[must_use]
pub fn spec_text() -> String {
    format!(
        "# HARNESS SPEC (versioned; docs/harness/)\n\n\
         - loop target composite: {}\n\
         - gap threshold (insights): 0.05\n\
         - gate composite tolerance: 0.05\n\
         - gate mismatch tolerance: 1\n\
         - verifier: declared verify_command executed in verify_target (ADR-0011)\n\
         - close requires: composite >= target AND verifier pass AND zero gate regressions\n\
         - registers: failures (Reflexion + MAST), mismatches (tool parity)\n\
         - discipline: checkpoint begin/verify per edit; verify refuses without open BEGIN\n",
        crate::loopcmd::TARGET_COMPOSITE
    )
}

/// `harness snapshot`: write the versioned spec and append the ledger
/// row with the frozen-suite gate verdict.
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn snapshot(root: &Path) -> Result<(String, String), std::io::Error> {
    let dir = root.join("docs/harness");
    fs::create_dir_all(&dir)?;
    let rev = git_rev(root).unwrap_or_else(|| "no-git".to_string());
    let name = format!("HARNESS-{}-{rev}.md", crate::memory::utc_now_date());
    let path = dir.join(&name);
    if !path.exists() {
        fs::write(&path, spec_text())?;
    }
    // Frozen-suite verdict with this revision. Errors propagate — a
    // corrupt or missing baseline must NEVER record a fabricated green
    // (codex review finding, Phase 8).
    let entries =
        crate::eval::score_all_cases(&root.join("evals/cases"), root, &root.join("evals/golden"))
            .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("cannot score the frozen suite: {e}"),
            )
        })?;
    let baseline: Vec<crate::eval::GateEntry> = serde_json::from_str(&fs::read_to_string(
        root.join("evals/results/baseline.json"),
    )?)
    .map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("cannot read the frozen baseline: {e}"),
        )
    })?;
    let gate = crate::eval::run_gate(&entries, &baseline, 0.05, 1);
    let ledger = dir.join("ledger.md");
    let header = "| rev | date | spec | gate |\n| --- | --- | --- | --- |\n";
    let existing = fs::read_to_string(&ledger).unwrap_or_default();
    let body = if existing.contains("| rev |") {
        existing
    } else {
        header.to_string()
    };
    if body.contains(&format!("| {rev} |")) {
        return Ok((
            name,
            format!(
                "gate: {} regressions (revision {rev} already recorded)",
                gate.failures
            ),
        ));
    }
    let row = format!(
        "| {rev} | {} | {name} | {} regressions |\n",
        crate::memory::utc_now_date(),
        gate.failures
    );
    fs::write(&ledger, format!("{body}{row}"))?;
    Ok((
        name,
        format!(
            "gate: {} regressions across {} cases",
            gate.failures, gate.case_count
        ),
    ))
}

fn git_rev(root: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn snapshot_writes_spec_and_ledger_row_once() {
        let root = env::temp_dir().join(format!("mag-harness-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("evals/cases")).unwrap();
        fs::create_dir_all(root.join("evals/results")).unwrap();
        fs::write(root.join("evals/results/baseline.json"), "[]").unwrap();
        let (name1, verdict1) = snapshot(&root).unwrap();
        assert!(verdict1.contains("regressions"));
        let (name2, _) = snapshot(&root).unwrap();
        assert_eq!(name1, name2, "idempotent spec name");
        let ledger = fs::read_to_string(root.join("docs/harness/ledger.md")).unwrap();
        assert_eq!(ledger.matches("| rev |").count(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn spec_text_is_stable_and_documents_the_loop() {
        let text = spec_text();
        assert!(text.contains("composite"));
        assert!(text.contains("verifier"));
        assert!(text.contains("0.5"));
    }
}

/// Parse `[FAIL] <label>` lines out of a gate run (the deterministic
/// gate's failure set).
#[must_use]
pub fn gate_failures(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("[FAIL]")
                .map(|rest| rest.trim().to_string())
        })
        .collect()
}

/// Counterfactual harness gate (Phase 9 slice 5, Phantom Guardrails
/// 2607.13083): a candidate edit must REDUCE observed gate failures;
/// fixing a failure never observed before the edit is fabricated.
///
/// Runs the gate before (with the current file), swaps in the candidate,
/// runs the gate after, then restores the original. Returns a human
/// verdict. `claims` names the failure(s) the edit claims to fix; if a
/// claim is not in the BEFORE failure set, the edit is rejected with
/// evidence (byte-exact replay, not suppression).
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn verify_candidate(
    root: &Path,
    target: &Path,
    candidate: &Path,
    claims: Option<&str>,
) -> Result<String, std::io::Error> {
    let target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        root.join(target)
    };
    let original = fs::read_to_string(&target).ok();
    let before = gate_failures_text(root);
    // Phantom-guardrail check: claims must name failures observed BEFORE.
    if let Some(claims) = claims {
        for claim in claims.split(',').map(str::trim).filter(|c| !c.is_empty()) {
            if !before.iter().any(|f| f.contains(claim)) {
                let _ = restore(&target, original.as_ref());
                return Ok(format!(
                    "REJECT: claimed failure '{claim}' was never observed before the edit (Phantom Guardrails) — gate before: {before:?}"
                ));
            }
        }
    }
    let candidate_text = fs::read_to_string(candidate)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let _ = fs::write(&target, &candidate_text);
    let after = gate_failures_text(root);
    restore(&target, original.as_ref())?;
    let before_count = before.len();
    let after_count = after.len();
    match after_count.cmp(&before_count) {
        std::cmp::Ordering::Less => Ok(format!(
            "ACCEPT: gate failures {before_count} -> {after_count} (observed reduction)\n  before: {before:?}\n  after: {after:?}"
        )),
        std::cmp::Ordering::Equal => Ok(format!(
            "NEUTRAL: gate failures unchanged ({before_count}) — no observed reduction, edit not justified\n  before: {before:?}\n  after: {after:?}"
        )),
        std::cmp::Ordering::Greater => Ok(format!(
            "REJECT: gate failures {before_count} -> {after_count} (regression)\n  before: {before:?}\n  after: {after:?}"
        )),
    }
}

fn gate_failures_text(root: &Path) -> Vec<String> {
    let output = std::process::Command::new("sh")
        .arg("scripts/verify.sh")
        .current_dir(root)
        .output();
    match output {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            gate_failures(&text)
        }
        Err(_) => vec!["gate unavailable".to_string()],
    }
}

fn restore(target: &Path, original: Option<&String>) -> std::io::Result<()> {
    original.map_or_else(
        || {
            let _ = fs::remove_file(target);
            Ok(())
        },
        |text| fs::write(target, text),
    )
}

#[cfg(test)]
mod counterfactual_tests {
    use super::*;
    use std::env;

    fn tmp_gate_root(tag: &str, gate_body: &str) -> std::path::PathBuf {
        let root = env::temp_dir().join(format!("mag-hv-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(root.join("scripts/verify.sh"), gate_body).unwrap();
        root
    }

    #[test]
    fn gate_failures_parses_fail_lines() {
        assert_eq!(
            gate_failures("[FAIL] tests:\n[FAIL] clippy:\n[ok] build\n"),
            vec!["tests:", "clippy:"]
        );
    }

    #[test]
    fn candidate_reducing_failures_is_accepted() {
        // Target gate: one observed failure. Candidate file makes the
        // gate emit none -> observed reduction -> ACCEPT; original kept.
        let root = tmp_gate_root("reduce", "#!/bin/sh\necho \"[FAIL] tests:\"\nexit 1\n");
        fs::write(
            root.join("candidate.sh"),
            "#!/bin/sh\necho \"[ok] build\"\nexit 0\n",
        )
        .unwrap();
        let verdict = verify_candidate(
            &root,
            &root.join("scripts/verify.sh"),
            &root.join("candidate.sh"),
            None,
        )
        .unwrap();
        assert!(verdict.starts_with("ACCEPT"), "{verdict}");
        let restored = fs::read_to_string(root.join("scripts/verify.sh")).unwrap();
        assert!(restored.contains("exit 1"), "original must be restored");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn phantom_claim_rejected_with_evidence() {
        // Gate before observes only 'build'; the edit claims to fix
        // 'tests' -> fabricated guardrail -> REJECT with evidence.
        let root = tmp_gate_root("phantom", "#!/bin/sh\necho \"[FAIL] build:\"\nexit 1\n");
        let verdict = verify_candidate(
            &root,
            &root.join("scripts/verify.sh"),
            &root.join("scripts/verify.sh"),
            Some("tests"),
        )
        .unwrap();
        assert!(verdict.starts_with("REJECT"), "{verdict}");
        assert!(verdict.contains("never observed"), "{verdict}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regressing_candidate_rejected() {
        // Gate before: clean. Candidate introduces an observed failure.
        let root = tmp_gate_root("regress", "#!/bin/sh\necho \"[ok] build\"\nexit 0\n");
        fs::write(
            root.join("candidate.sh"),
            "#!/bin/sh\necho \"[FAIL] clippy:\"\nexit 1\n",
        )
        .unwrap();
        let verdict = verify_candidate(
            &root,
            &root.join("scripts/verify.sh"),
            &root.join("candidate.sh"),
            None,
        )
        .unwrap();
        assert!(verdict.starts_with("REJECT"), "{verdict}");
        let _ = fs::remove_dir_all(&root);
    }
}
