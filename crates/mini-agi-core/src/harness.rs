//! Harness evolution (Phase 8 slice 7, RHI 2607.15524): the harness
//! spec is data — versioned snapshots + a gate-verdict ledger.
//!
//! A revision is accepted only when the frozen-suite gate passes with
//! it (pairwise eval); the checkpoint journal is the evolution ledger.

use std::fs;
use std::io;
use std::path::Path;

/// The harness spec snapshot: the constants and loop semantics that
/// define how the kernel evaluates work. Bump intentionally.
#[must_use]
pub fn spec_text() -> String {
    format!(
        "# HARNESS SPEC (versioned; docs/harness/)\n\n\
         - target composite: {} (constant kept for API compatibility; the\n\
           minimal model is achieved=true)\n\
         - verifier: declared verify_command executed in verify_target (ADR-0011)\n\
         - close requires: run achieved AND verifier pass (the gate IS the verification;\n\
           composite scoring / judge calibration / registers were removed as\n\
           over-verification)\n\
         - gap lifecycle: evals/ledger/<case>.json is authoritative, written by\n\
           loop dispatch/verify under the claims lock; terminal states are never redispatched\n\
         - unverified runs are never reported successful: loop verify closes a gap\n\
           only when the run is achieved AND the declared gate passes (or --allow-unverified)\n\
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
        write_atomic(&path, &spec_text())?;
    }
    let ledger = dir.join("ledger.md");
    // Hold the harness lock ACROSS the gate run AND the ledger write: a
    // concurrent `harness verify <candidate>` swaps a file — measuring the
    // gate here while a candidate is live would record a wrong-verdict
    // ledger row for the real revision.
    let _lock = crate::ticket::lock_file(&root.join("tickets/.harness.lock"), HARNESS_STALE_SECS)
        .map_err(io::Error::other)?;
    let (failures, passed) = gate_run(root);
    let gate = format!(
        "{} regression(s), {}",
        failures.len(),
        if passed { "green" } else { "red" }
    );
    let header = "| rev | date | spec | gate |\n| --- | --- | --- | --- |\n";
    let existing = fs::read_to_string(&ledger).unwrap_or_default();
    let body = if existing.contains("| rev |") {
        existing
    } else {
        header.to_string()
    };
    if let Some(pos) = body.lines().position(|l| l.contains(&format!("| {rev} |"))) {
        // Same-rev re-snapshot: REFRESH the row's gate verdict (a frozen
        // suite that regressed on the same rev must not keep a stale
        // green row — the ledger's purpose is recording the verdict).
        let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
        lines[pos] = format!(
            "| {rev} | {} | {name} | {gate} |",
            crate::memory::utc_now_date(),
        );
        let refreshed = lines.join("\n") + "\n";
        let tmp = crate::ticket::tmp_unique(&ledger, "harness");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .and_then(|mut f| std::io::Write::write_all(&mut f, refreshed.as_bytes()))?;
        crate::ticket::sync_then_rename(&tmp, &ledger)?;
        return Ok((name, format!("gate: {gate} (revision {rev} refreshed)")));
    }
    let row = format!(
        "| {rev} | {} | {name} | {gate} |\n",
        crate::memory::utc_now_date(),
    );
    let tmp = crate::ticket::tmp_unique(&ledger, "harness");
    let content = format!("{body}{row}");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| std::io::Write::write_all(&mut f, content.as_bytes()))?;
    crate::ticket::sync_then_rename(&tmp, &ledger)?;
    Ok((name, format!("gate: {gate}")))
}

fn git_rev(root: &Path) -> Option<String> {
    // Through `run_capped` like every other subprocess — a bare
    // `Command::output()` is the one remaining unbounded spawn.
    let result =
        crate::worker::run_capped("git", &["rev-parse", "--short", "HEAD"], root, Some(30)).ok()?;
    if result.status != Some(0) {
        return None;
    }
    Some(result.output.trim().to_string())
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
/// 2607.13083): a candidate edit must reduce observed gate failures to
/// ZERO; fixing a failure never observed before the edit is fabricated.
///
/// Runs the gate before (with the current file), swaps in the candidate,
/// runs the gate after, then restores the original. Returns a human
/// verdict. `claims` names the failure(s) the edit claims to fix; if a
/// claim is not in the BEFORE failure set, the edit is rejected with
/// evidence (byte-exact replay, not suppression).
///
/// ACCEPT is reachable ONLY on a fully green after-gate (exit 0 ⟹ zero
/// remaining failures); a partial reduction that still leaves any
/// failure is REJECTED (fail-closed) — the after-gate must never look
/// like a reduction while the repo stays red.
///
/// Stale threshold for the DEDICATED harness lock: above the capped
/// gate run so a concurrent harness cannot steal a live counterfactual.
const HARNESS_STALE_SECS: u64 = 3600;

/// Counterfactual gate (Phantom Guardrails): a candidate edit must
/// reduce observed gate failures to ZERO; fixing a failure never
/// observed before the edit is fabricated.
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
    // The WHOLE counterfactual (before -> phantom check -> swap -> gate
    // -> restore) is serialized under a DEDICATED harness lock with a
    // stale threshold ABOVE the gate cap — two concurrent `harness verify`
    // runs must not measure each other's candidate as a baseline (a
    // phantom ACCEPT). The claims lock stays for short ticket ops.
    let _harness_lock =
        crate::ticket::lock_file(&root.join("tickets/.harness.lock"), HARNESS_STALE_SECS)
            .map_err(io::Error::other)?;
    let before = gate_failures_text(root);
    // Phantom-guardrail check: claims must name failures observed BEFORE.
    if let Some(claims) = claims {
        for claim in claims.split(',').map(str::trim).filter(|c| !c.is_empty()) {
            // TOKEN-grade match: a claim of "foo" must not pass on an
            // observed "foobar failed" (substring matching would let a
            // claim ride on an unrelated failure's label).
            let token_match = |f: &str, claim: &str| {
                f.split(|c: char| !c.is_alphanumeric())
                    .any(|tok| tok == claim)
            };
            if !before.iter().any(|f| token_match(f, claim)) {
                let _ = restore(&target, original.as_ref());
                return Ok(format!(
                    "REJECT: claimed failure '{claim}' was never observed before the edit (Phantom Guardrails) — gate before: {before:?}"
                ));
            }
        }
    }
    // The dedicated harness lock (above) already serializes this whole
    // section — no claims-lock involvement (its 30s steal would trip on
    // the 1800s gate).
    let candidate_text = fs::read_to_string(candidate)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    // A failed swap must NOT be measured as the original file (a
    // fabricated NEUTRAL/ACCEPT) — propagate the write error.
    // A failed swap must NOT be measured as the original file — and must
    // NOT leave the target swapped/truncated: restore on write error.
    if let Err(e) = write_atomic(&target, &candidate_text) {
        let _ = restore(&target, original.as_ref());
        return Err(e);
    }
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
            "ACCEPT: gate fully green after the swap ({before_count} -> 0 observed failures)\n  before: {before:?}\n  after: {after:?}"
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
    // §5.2: the harness gate runs through the capped runner too — never a
    // bare `Command::output()` (a hung verify.sh must not block the
    // counterfactual forever). verify.sh runs cargo test, so the cap is
    // generous; the [FAIL] markers survive in the captured output.
    let cap = 1800u64;
    match crate::worker::run_capped("sh", &["scripts/verify.sh"], root, Some(cap)) {
        Ok(res) => {
            let text = res.output;
            let mut failures = gate_failures(&text);
            if (res.status != Some(0) || res.aborted) && failures.is_empty() {
                failures.push(format!(
                    "gate exited {} without [FAIL] markers",
                    res.status.map_or_else(|| "-".into(), |c| c.to_string())
                ));
            }
            (failures, res.status == Some(0) && !res.aborted)
        }
        Err(_) => (vec!["gate unavailable".to_string()], false),
    }
}

/// Write `content` to `path` atomically (temp + rename): a crash/SIGKILL
/// mid-write must not leave a truncated file, and concurrent readers must
/// see the OLD or the NEW content, never a partial write.
fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let tmp = crate::ticket::tmp_unique(path, "harness");
    let res = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| f.write_all(content.as_bytes()));
    if res.is_ok() {
        // Preserve the target's file MODE: temp+rename replaces the
        // inode, so an executable/restricted target would come back 0644
        // (lost +x, or a previously private file made world-readable).
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        crate::ticket::sync_then_rename(&tmp, path)
    } else {
        let _ = std::fs::remove_file(&tmp);
        res
    }
}

fn restore(target: &Path, original: Option<&String>) -> std::io::Result<()> {
    original.map_or_else(
        || {
            let _ = fs::remove_file(target);
            Ok(())
        },
        |text| write_atomic(target, text),
    )
}
