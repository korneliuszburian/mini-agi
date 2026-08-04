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
    // Distinguish absent vs unreadable (codex review): a target that
    // EXISTS but cannot be read must error, not be treated as absent —
    // otherwise restore() would delete it on a rejected claim.
    let original = match fs::read_to_string(&target) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("target {} exists but is unreadable: {e}", target.display()),
            ));
        }
    };
    // The gate must never be its own counterfactual subject (codex
    // review): swapping scripts/verify.sh self-validates.
    if target.file_name().is_some_and(|n| n == "verify.sh")
        && target.components().any(|c| c.as_os_str() == "scripts")
    {
        return Ok(
            "REJECT: refusing to counterfactually validate the gate itself (scripts/verify.sh)"
                .into(),
        );
    }
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
    let (after, after_ok) = gate_run(root);
    restore(&target, original.as_ref())?;
    // A gate that FAILS after the swap (markerless abort, crash) is an
    // INVALID after-observation — automatic rejection, never a
    // countable "reduction" (codex review).
    if !after_ok {
        return Ok(format!(
            "REJECT: gate did not complete cleanly after the swap (exit non-zero, {after:?}) — broken gate must not look like a reduction"
        ));
    }
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
    gate_run(root).0
}

/// Run the gate; returns (parsed [FAIL] labels, gate-exit-success).
/// The markerless-failure synthesis stays in the BEFORE set (a silently
/// broken current gate must be visible); the AFTER check treats any
/// non-success as invalid (see `verify_candidate`).
fn gate_run(root: &Path) -> (Vec<String>, bool) {
    let output = std::process::Command::new("sh")
        .arg("scripts/verify.sh")
        .current_dir(root)
        .output();
    match output {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            let mut failures = gate_failures(&text);
            if !out.status.success() && failures.is_empty() {
                failures.push(format!(
                    "gate exited {} without [FAIL] markers",
                    out.status
                        .code()
                        .map_or_else(|| "-".into(), |c| c.to_string())
                ));
            }
            (failures, out.status.success())
        }
        Err(_) => (vec!["gate unavailable".to_string()], false),
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
        fs::create_dir_all(root.join("candidate")).unwrap();
        fs::write(root.join("scripts/verify.sh"), gate_body).unwrap();
        root
    }

    /// A fake gate that READS an `ok.marker` data file — the
    /// counterfactual target is the marker, never the gate itself.
    fn marker_gate_root(tag: &str) -> std::path::PathBuf {
        tmp_gate_root(
            tag,
            "#!/bin/sh\nif [ \"$(cat ok.marker 2>/dev/null)\" = \"x\" ]; then echo \"[ok] build\"; exit 0; else echo \"[FAIL] marker-missing:\"; exit 1; fi\n",
        )
    }

    #[test]
    fn gate_failures_parses_fail_lines() {
        assert_eq!(
            gate_failures("[FAIL] tests:\n[FAIL] clippy:\n[ok] build\n"),
            vec!["tests:", "clippy:"]
        );
    }

    #[test]
    fn refuses_to_counterfactually_validate_the_gate_itself() {
        let root = tmp_gate_root("self", "#!/bin/sh\nexit 0\n");
        let verdict = verify_candidate(
            &root,
            &root.join("scripts/verify.sh"),
            &root.join("scripts/verify.sh"),
            None,
        )
        .unwrap();
        assert!(verdict.starts_with("REJECT"), "{verdict}");
        assert!(verdict.contains("gate itself"), "{verdict}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn candidate_reducing_failures_is_accepted() {
        // Marker absent -> gate fails. Candidate makes it present ->
        // observed reduction -> ACCEPT; original (absent) restored.
        let root = marker_gate_root("reduce");
        fs::write(root.join("candidate/ok.marker"), "x").unwrap();
        let verdict = verify_candidate(
            &root,
            &root.join("ok.marker"),
            &root.join("candidate/ok.marker"),
            None,
        )
        .unwrap();
        assert!(verdict.starts_with("ACCEPT"), "{verdict}");
        assert!(
            !root.join("ok.marker").exists(),
            "original (absent) restored"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn phantom_claim_rejected_with_evidence() {
        // Gate before observes only 'marker-missing'; the edit claims
        // to fix 'tests' -> fabricated guardrail -> REJECT with evidence.
        let root = marker_gate_root("phantom");
        fs::write(root.join("candidate/ok.marker"), "x").unwrap();
        let verdict = verify_candidate(
            &root,
            &root.join("ok.marker"),
            &root.join("candidate/ok.marker"),
            Some("tests"),
        )
        .unwrap();
        assert!(verdict.starts_with("REJECT"), "{verdict}");
        assert!(verdict.contains("never observed"), "{verdict}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn regressing_candidate_rejected() {
        // Before: marker present -> gate clean. Candidate replaces the
        // marker with a broken gate-copy -> the gate's failure set grows.
        let root = marker_gate_root("regress");
        fs::write(root.join("ok.marker"), "x").unwrap();
        fs::write(
            root.join("candidate/ok.marker"),
            "#!/bin/sh\necho \"[FAIL] clippy:\"\nexit 1\n",
        )
        .unwrap();
        let verdict = verify_candidate(
            &root,
            &root.join("ok.marker"),
            &root.join("candidate/ok.marker"),
            None,
        )
        .unwrap();
        assert!(verdict.starts_with("REJECT"), "{verdict}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn markerless_failing_gate_is_not_green() {
        // A gate that exits non-zero with no [FAIL] markers is broken —
        // the counterfactual must not count it as clean.
        let root = tmp_gate_root("silentfail", "#!/bin/sh\nexit 1\n");
        let failures = gate_failures_text(&root);
        assert!(!failures.is_empty(), "markerless failure must be detected");
        assert!(
            failures.iter().any(|f| f.contains("without [FAIL]")),
            "{failures:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
