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
