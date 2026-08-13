#![allow(dead_code)]
//! AFK verified-iteration supervisor (AFK-SUPERVISOR, S2).
//!
//! `mini-agi loop run <goal-or-case>` supervises a background worker
//! (codex) under the verified-iteration core: per-attempt progress
//! artifact (`progress.md`), a reviewable run report (Markdown), and an
//! optional on-done hook (notification point). This is Matt Pocock's
//! AFK/Ralph supervision model ("give input before and after, not
//! during"; end in a reviewable artifact) realized with the kernel's
//! deterministic verified-iteration.
//!
//! ## Two-phase liveness (S3)
//!
//! The worker has TWO timeouts (Sandcastle's two-phase model):
//! 1. **Idle timeout** (`max_idle_seconds`): the worker's output-file
//!    mtime is the liveness signal — when it stops changing while the
//!    process still runs, the worker is killed as STUCK.
//! 2. **Completion grace**: a worker that emitted the completion
//!    marker and then hangs (e.g. a child holding the pipe) still
//!    resolves with its FULL transcript — the file-redirect design in
//!    `run_capped` makes the transcript readable after the kill, so
//!    the run is success-with-warning, never lost work.
//!
//! ## On-done hook contract
//!
//! `--on-done <cmd>` runs `sh -c <cmd> on-done <report-path> <outcome>`
//! where outcome is `0` (verifier passed), `1` (exhausted) or `3`
//! (aborted); the hook command reads them as `$1` / `$2`. This is the
//! notification point (e.g. a ping script).

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::worker::{self, IterationInput, IterationResult, ProgressEvent};

/// Supervisor inputs (fully resolved).
// The bools mirror the CLI flags one-to-one (no_sandbox, blind_worker,
// read_only, resume) — grouping them would obscure the CLI mapping.
#[allow(clippy::struct_excessive_bools)]
pub struct SupervisorArgs<'a> {
    /// Spec text (case spec or generated ad-hoc spec).
    pub spec_text: &'a str,
    /// Goal (parsed).
    pub goal: &'a str,
    /// Scope list (parsed; may be empty for ad-hoc goals).
    pub scope_list: &'a [String],
    /// The deterministic verifier command (required; P0-3).
    pub verify: &'a str,
    /// The verifier target dir.
    pub target: &'a Path,
    /// Worker workdir (where the code + progress.md live).
    pub workdir: &'a Path,
    /// Iteration count.
    pub iterate: usize,
    /// Blind-worker mode + hidden suite.
    pub blind_worker: bool,
    pub hidden_dir: Option<&'a Path>,
    /// Wall cap per attempt.
    pub wall_cap: Option<u64>,
    /// Idle cap per attempt, overriding the configured value.
    pub max_idle: Option<u64>,
    /// Step cap (accumulated).
    pub step_cap: Option<usize>,
    /// No-sandbox escape hatch.
    pub no_sandbox: bool,
    /// Worker name (default codex).
    pub worker_name: &'a str,
    /// Read-only sandbox.
    pub read_only: bool,
    /// On-done hook: shell command run with the report path + outcome
    /// ("0" | "1" | "3") as arguments.
    pub on_done: Option<&'a str>,
    /// Draft write target (like `cmd_codex`'s `run_out`): the case's
    /// own run.json when the goal resolved from a case, else the
    /// workdir.
    pub run_out: Option<&'a Path>,
    /// Session resume (AFK v2): resume the worker's own codex session
    /// on verifier failure. Default: on for iterate > 1.
    pub resume: bool,
    /// Loop template (`--template`): "sequential-reviewer" runs an
    /// INDEPENDENT read-only review pass after the verified iteration,
    /// then ONE fix attempt via the worker's session resume when the
    /// verdict is REWORK/FIX-MINOR, then the verifier re-runs.
    pub template: Option<&'a str>,
    /// Run report path (default: workdir/REPORT.md).
    pub report: Option<&'a Path>,
}

/// Final outcome resolution (codex review F2/F3): the pre-review
/// iteration verdict is NOT the authority once a fix pass ran or was
/// required-but-impossible. Returns the final verifier-passed flag and
/// the reason recorded in the report/hook/exit.
#[must_use]
pub fn resolve_final_outcome(
    iteration_passed: bool,
    template_used: bool,
    verdict: &str,
    session: Option<&str>,
    fix_passed: Option<bool>,
) -> (bool, String) {
    if !template_used {
        return (iteration_passed, "verified iteration".to_string());
    }
    match verdict {
        "APPROVE" => (iteration_passed, "review approved".to_string()),
        "UNPARSEABLE" => (
            iteration_passed,
            "review unparseable — disposition recorded".to_string(),
        ),
        // REWORK / FIX-MINOR: the review demands changes.
        _ => match (session, fix_passed) {
            (Some(_), Some(p)) => {
                if p {
                    (true, "fix verifier passed".to_string())
                } else {
                    (
                        false,
                        "fix verifier FAILED — the fix regressed or missed".to_string(),
                    )
                }
            }
            (None, _) => (
                false,
                "fix required by review but no worker session to resume".to_string(),
            ),
            (Some(_), None) => (false, "fix attempt did not run".to_string()),
        },
    }
}

/// A parsed review verdict (sequential-reviewer template).
#[derive(Debug, Clone)]
pub struct ReviewVerdict {
    /// APPROVE | FIX-MINOR | REWORK | UNPARSEABLE (raw fallback).
    pub verdict: String,
    /// Score out of 8 when parseable.
    pub score: Option<u8>,
    /// The raw findings text (numbered findings the fix pass uses).
    pub findings: String,
}

/// Tolerant verdict parser: the reviewer's final block is
/// `Verdict: APPROVE|FIX-MINOR|REWORK, score X/8, findings...`. A
/// missing verdict records UNPARSEABLE with the raw text — it never
/// blocks the pipeline (the disposition is recorded, the run report
/// keeps the raw verdict).
pub fn parse_review_verdict(text: &str) -> ReviewVerdict {
    let mut verdict = String::from("UNPARSEABLE");
    let mut score = None;
    let mut findings = String::new();
    let mut in_findings = false;
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("Verdict:") {
            let v = v.trim();
            // Exact token match (codex review F5): "APPROVED"/"REWORKING"
            // must NOT match "APPROVE"/"REWORK".
            let word: String = v
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .next()
                .unwrap_or("")
                .to_string();
            match word.as_str() {
                "APPROVE" => verdict = "APPROVE".into(),
                "FIX-MINOR" => verdict = "FIX-MINOR".into(),
                "REWORK" => verdict = "REWORK".into(),
                _ => {}
            }
            // "Verdict: REWORK, score 2/8" — the score may sit on the
            // Verdict line after the verdict word. Word boundary on BOTH
            // sides: "xscore 2/8" must be rejected (left boundary).
            if let Some(idx) = v.find("score") {
                let left_ok = v[..idx]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
                let rest = &v[idx + "score".len()..];
                let right_ok = rest
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace() || c == ':' || c == '/');
                if left_ok && right_ok {
                    for tok in rest.split(|c: char| !c.is_ascii_digit() && c != '/') {
                        if let Some(s) = tok.strip_suffix("/8")
                            && let Ok(n) = s.parse::<u8>()
                        {
                            score = Some(n.min(8));
                        }
                    }
                }
            }
        }
        // The score requires the "score" keyword (F5): the line must
        // START with the word "score" — "scoreboard 7/8" is rejected.
        if let Some(rest) = l.strip_prefix("score")
            && rest
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == ':' || c == '/')
        {
            for tok in rest.split(|c: char| !c.is_ascii_digit() && c != '/') {
                if let Some(s) = tok.strip_suffix("/8")
                    && let Ok(n) = s.parse::<u8>()
                {
                    score = Some(n.min(8));
                }
            }
        }
        if l.starts_with("Top findings") || l.starts_with("Findings:") {
            in_findings = true;
        } else if in_findings && !l.is_empty() {
            findings.push_str(line);
            findings.push('\n');
        }
    }
    if findings.is_empty() {
        findings = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join(
                "
",
            );
    }
    ReviewVerdict {
        verdict,
        score,
        findings: findings.trim().to_string(),
    }
}

/// A fully supervised run result.
pub struct SupervisorResult {
    /// The iteration result from the core.
    pub iteration: IterationResult,
    /// The report path written.
    pub report_path: PathBuf,
    /// The progress.md path written.
    pub progress_path: PathBuf,
    /// The final verifier-passed flag (after the review/fix gate).
    pub final_passed: bool,
    /// Why (recorded in the report/hook/exit).
    pub final_reason: String,
}

/// Run the supervised verified-iteration loop: writes progress.md per
/// attempt, runs the core, writes the run report, invokes the on-done
/// hook. Returns the result (exit semantics belong to the caller: 0
/// passed, 1 exhausted, 3 aborted).
///
/// # Errors
///
/// Returns a message when the workdir cannot be created, the worker
/// cannot be spawned, or the run fails outside the defined budget paths.
pub fn run(args: &SupervisorArgs<'_>) -> Result<SupervisorResult, String> {
    std::fs::create_dir_all(args.workdir).map_err(|e| e.to_string())?;
    let progress_path = args.workdir.join("progress.md");
    let stamp = || mini_agi_core::memory::utc_now_stamp();
    std::fs::write(&progress_path, format!("# progress — {}\n\n", args.goal))
        .map_err(|e| e.to_string())?;

    let input = IterationInput {
        spec_text: args.spec_text,
        goal: args.goal,
        scope_list: args.scope_list,
        verify: args.verify,
        target: args.target,
        workdir: args.workdir,
        wall_cap: args.wall_cap,
        max_idle: args.max_idle,
        step_cap: args.step_cap,
        no_sandbox: args.no_sandbox,
        worker_name: args.worker_name,
        read_only: args.read_only,
        iterate: args.iterate,
        blind_worker: args.blind_worker,
        hidden_dir: args.hidden_dir,
        resume: args.resume,
    };
    let iteration = worker::run_verified_iteration(&input, |event| {
        let line = match &event {
            ProgressEvent::AttemptStarted { attempt } => {
                format!("- {} attempt {attempt} started\n", stamp())
            }
            ProgressEvent::Verifier {
                attempt,
                failed_cases,
                passed,
            } => {
                if *passed {
                    format!("- {} attempt {attempt}: VERIFIER PASSED\n", stamp())
                } else {
                    format!(
                        "- {} attempt {attempt}: verifier FAILED — remaining cases: {}\n",
                        stamp(),
                        failed_cases.join(", ")
                    )
                }
            }
            ProgressEvent::Aborted { reason } => {
                format!("- {} ABORTED: {reason}\n", stamp())
            }
            ProgressEvent::SessionResumed {
                attempt,
                session_id,
            } => {
                format!(
                    "- {} attempt {attempt}: RESUMING worker session {session_id}\n",
                    stamp()
                )
            }
        };
        // Best-effort append: a progress write failure must not kill the
        // supervision.
        append_progress(&progress_path, &line);
    })?;

    // Sequential-reviewer template: an INDEPENDENT read-only review of
    // the produced work; REWORK/FIX-MINOR triggers ONE fix attempt via
    // the worker's session resume, then the verifier re-runs.
    let mut review: Option<ReviewVerdict> = None;
    let mut fix_passed: Option<bool> = None;
    if args.template == Some("sequential-reviewer") && iteration.verifier_passed {
        let review_text = run_review_pass(args)?;
        let verdict = parse_review_verdict(&review_text);
        review = Some(verdict.clone());
        append_progress(
            &progress_path,
            &format!(
                "- {} REVIEW: {} {}\n",
                stamp(),
                verdict.verdict,
                verdict
                    .score
                    .map_or_else(String::new, |s| format!("({s}/8)"))
            ),
        );
        if matches!(verdict.verdict.as_str(), "REWORK" | "FIX-MINOR")
            && let Some(session) = iteration.resume_session.as_deref()
        {
            append_progress(
                &progress_path,
                &format!(
                    "- {} FIX PASS: resuming worker session {session} with the findings\n",
                    stamp()
                ),
            );
            let fix_prompt = format!(
                "Independent review of your work found issues — address them now in this session.\n\n{}\nRe-run the verifier when done; it must pass.",
                verdict.findings
            );
            let fix_args = vec![
                "exec",
                "resume",
                session,
                "--skip-git-repo-check",
                &fix_prompt,
            ];
            let wall_cap = args.wall_cap.unwrap_or(600);
            let idle_cap = resolve_idle_cap(args.max_idle, args.workdir);
            let fix = worker::run_worker_sandboxed(
                args.worker_name,
                args.workdir,
                args.no_sandbox,
                args.read_only,
                Some(wall_cap),
                idle_cap,
                &fix_args,
            );
            let verifier = mini_agi_core::worker::run_capped(
                "sh",
                &["-c", args.verify],
                args.target,
                Some(120),
            );
            let passed_after_fix =
                fix.is_ok() && verifier.is_ok_and(|v| !v.aborted && v.status == Some(0));
            fix_passed = Some(passed_after_fix);
            append_progress(
                &progress_path,
                &format!(
                    "- {} FIX PASS verifier: {}\n",
                    stamp(),
                    if passed_after_fix { "PASSED" } else { "FAILED" }
                ),
            );
        }
    }

    let (final_passed, final_reason) = resolve_final_outcome(
        iteration.verifier_passed,
        args.template.is_some(),
        review.as_ref().map_or("", |v| v.verdict.as_str()),
        iteration.resume_session.as_deref(),
        fix_passed,
    );

    // Persist the run draft (the run.json the verifier/loop verify use):
    // the supervisor owns the run record. The run's claim is backed by
    // the kernel's in-loop verifier result (achieved = verifier_passed),
    // but per ADR-0011 the TRUSTED verification record is written only
    // by `run verify` / `loop verify` — the in-loop pass is iteration
    // feedback; the claim stays the run's own until then.
    let mut run = iteration.run.clone();
    run["outcome"]["achieved"] = serde_json::json!(final_passed);
    run["final_reason"] = serde_json::json!(final_reason);
    let draft_path = args
        .run_out
        .unwrap_or(&args.workdir.join("run.json"))
        .to_path_buf();
    std::fs::write(
        &draft_path,
        serde_json::to_string_pretty(&run).unwrap_or_default(),
    )
    .map_err(|e| format!("cannot write run draft: {e}"))?;

    // Run report (the reviewable artifact — Matt's "end in a PR", here
    // a report the agent reads and cites).
    let report_path = args
        .report
        .unwrap_or(&args.workdir.join("REPORT.md"))
        .to_path_buf();
    let mut report = String::new();
    let _ = writeln!(report, "# run report — {}", args.goal);
    let _ = writeln!(report, "- goal: {}", args.goal);
    let _ = writeln!(report, "- attempts: {}", iteration.attempts_done);
    let _ = writeln!(
        report,
        "- verifier: {}",
        if iteration.verifier_passed {
            "PASSED"
        } else {
            "NOT PASSED"
        }
    );
    let _ = writeln!(
        report,
        "- final outcome: {} ({final_reason})",
        if final_passed { "PASSED" } else { "NOT PASSED" }
    );
    let _ = writeln!(
        report,
        "- total wall: {}s | ~{} tokens (transcript bytes / 4)",
        iteration.total_wall,
        iteration.total_bytes / 4
    );
    let _ = writeln!(report, "- run.json: {}/run.json", args.workdir.display());
    if iteration.completion_grace {
        let _ = writeln!(
            report,
            "- completion grace: a cap-killed worker still delivered its full \
             completed transcript — success-with-warning"
        );
    }
    if let Some(v) = &review {
        let _ = writeln!(report, "\n## review (sequential-reviewer)");
        let _ = writeln!(
            report,
            "- verdict: {} {}/8",
            v.verdict,
            v.score.map_or_else(|| "?".into(), |s| s.to_string())
        );
        if v.verdict == "UNPARSEABLE" {
            let _ = writeln!(report, "- raw verdict:\n{}", v.findings);
        } else {
            let _ = writeln!(report, "- findings:\n{}", v.findings);
        }
    }
    let _ = writeln!(report, "\n## attempt chain");
    for v in &iteration.attempt_verdicts {
        let _ = writeln!(report, "- {v}");
    }
    std::fs::write(&report_path, report).map_err(|e| e.to_string())?;

    // On-done hook: report path + outcome as args (notification point).
    if let Some(cmd) = args.on_done {
        let outcome = if iteration.aborted {
            "3"
        } else if final_passed {
            "0"
        } else {
            "1"
        };
        let _ = std::process::Command::new("sh")
            .args([
                "-c",
                cmd,
                "on-done",
                &report_path.to_string_lossy(),
                outcome,
            ])
            .status();
    }

    Ok(SupervisorResult {
        iteration,
        report_path,
        progress_path,
        final_passed,
        final_reason,
    })
}

/// Inputs for spec resolution.
pub struct ResolveInput<'a> {
    /// Goal text or an existing case name.
    pub goal_or_case: &'a str,
    /// Repo root (evals/cases/<name>/run.json).
    pub root: &'a Path,
    /// Worker workdir.
    pub workdir: &'a Path,
    /// Explicit verifier command (flag).
    pub verify: Option<&'a str>,
    /// Explicit verifier target (flag).
    pub target: Option<&'a Path>,
}

/// A resolved, verifiable supervised spec.
#[derive(Debug)]
pub struct ResolvedSpec {
    /// The spec text (prompt base).
    pub spec_text: String,
    /// The goal.
    pub goal: String,
    /// The scope list.
    pub scope_list: Vec<String>,
    /// The deterministic verifier command.
    pub verify_cmd: String,
    /// The verifier target — a case's own `verify_target` unless an
    /// explicit `--target` overrides it (codex review F2).
    pub target: PathBuf,
}

/// Resolve `loop run` inputs into a verifiable spec (P0-3 enforced).
///
/// A case name reuses its run.json (goal, scope, `verify_command` AND
/// its `verify_target` — the verifier must run where the case says it
/// runs);
/// an ad-hoc goal requires an explicit `--verify` (and defaults the
/// target to the workdir).
///
/// # Errors
///
/// Returns a message when the case name is not path-safe or cannot be
/// parsed, when the goal declares no verifier, or when the verifier
/// shape is invalid.
pub fn resolve(input: &ResolveInput<'_>) -> Result<ResolvedSpec, String> {
    // Path safety (mirrors loopcmd/snapshot validation): a raw join
    // would let `loop run ../x` read a run.json outside evals/cases.
    if !mini_agi_core::loopcmd::case_is_plain_segment(input.goal_or_case) {
        return Err(format!(
            "invalid case name '{}' — use a plain name (no separators)",
            input.goal_or_case
        ));
    }
    let case_dir = input.root.join("evals/cases").join(input.goal_or_case);
    if case_dir.join("run.json").is_file() {
        let run: mini_agi_core::eval::Run = serde_json::from_str(
            &std::fs::read_to_string(case_dir.join("run.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("cannot parse case {}: {e}", input.goal_or_case))?;
        let vc = run.verify_command.as_deref().ok_or_else(|| {
            format!(
                "case {} declares no verify_command (P0-3)",
                input.goal_or_case
            )
        })?;
        // A case's `verify_target` is UNTRUSTED run data (ARCHITECTURE-
        // CONDENSED 5.1): when no explicit --target overrides it, resolve
        // it against the repo root and REJECT a target that escapes it —
        // the supervisor's verifier runs the gate in this directory.
        // The operator's explicit --target stays trusted as-is.
        let vt = if let Some(t) = input.target {
            t.to_string_lossy().into_owned()
        } else {
            let declared = run
                .verify_target
                .unwrap_or_else(|| input.workdir.to_string_lossy().into_owned());
            mini_agi_core::loopcmd::resolve_target(input.root, &declared)?
                .to_string_lossy()
                .into_owned()
        };
        let spec_text = format!(
            "# SLICE SPEC (supervised)\n\n- goal: {}\n- scope: {}\n- verify_command: {vc} in {vt}\n",
            run.goal,
            run.scope.join(", ")
        );
        Ok(ResolvedSpec {
            spec_text,
            goal: run.goal,
            scope_list: run.scope,
            verify_cmd: vc.to_string(),
            target: PathBuf::from(vt),
        })
    } else {
        let vc = input.verify.ok_or_else(|| {
            "ad-hoc goal requires --verify (the deterministic verifier, P0-3)".to_string()
        })?;
        let vt = input.target.unwrap_or(input.workdir);
        let spec_text = format!(
            "# SLICE SPEC (ad-hoc, supervised)\n\n- goal: {}\n- scope: (none declared)\n- verify_command: {vc} in {}\n",
            input.goal_or_case,
            vt.display()
        );
        Ok(ResolvedSpec {
            spec_text,
            goal: input.goal_or_case.to_string(),
            scope_list: Vec::new(),
            verify_cmd: vc.to_string(),
            target: vt.to_path_buf(),
        })
    }
}

/// Append a line to the progress artifact (best-effort).
/// Resolve the idle cap: an explicit CLI value wins, else the config
/// default (`max_idle_seconds`).
fn resolve_idle_cap(explicit: Option<u64>, workdir: &Path) -> Option<u64> {
    explicit.or_else(|| mini_agi_core::config::Config::load(workdir).max_idle_seconds)
}

/// The verifier command to run: an explicit `--verify` wins, else the
/// goal's resolved verifier (empty means the goal declares none).
///
/// # Errors
///
/// Returns a message when neither source yields a verifier.
pub fn resolve_verify_cmd(explicit: Option<&str>, resolved_cmd: String) -> Result<String, String> {
    if let Some(v) = explicit {
        return Ok(v.to_string());
    }
    if resolved_cmd.is_empty() {
        return Err("cannot resolve a verifier for this goal".to_string());
    }
    Ok(resolved_cmd)
}

fn append_progress(path: &Path, line: &str) {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

/// The independent review pass (sequential-reviewer): a read-only codex
/// session reviews the produced work against the rubric and returns a
/// verdict. Wall-capped like a worker run.
fn run_review_pass(args: &SupervisorArgs<'_>) -> Result<String, String> {
    let prompt = format!(
        "Read-only adversarial review of the work just produced in this workdir by a supervised worker run (see progress.md and run.json for the goal and attempt chain). Review the working tree: the changes the worker made.\n\nGoal: {}\n\nScore 4 dimensions 0-2 (Correctness, Security, Tests, Scope), total /8: APPROVE >=7, FIX-MINOR 5-6, REWORK <5. Evidence-first: cite file:line or verifier output for EVERY finding. You are READ-ONLY: make NO changes, run NO writes.\n\nEnd with exactly:\nVerdict: APPROVE|FIX-MINOR|REWORK\nscore X/8\nTop findings:\n1. ... (each with file:line + severity)\n",
        args.goal
    );
    let review_args = vec!["exec", "-s", "read-only", "--skip-git-repo-check", &prompt];
    let idle_cap = resolve_idle_cap(args.max_idle, args.workdir);
    let review = worker::run_worker_sandboxed(
        args.worker_name,
        args.workdir,
        args.no_sandbox,
        true,
        Some(600),
        idle_cap,
        &review_args,
    )
    .map_err(|e| format!("review pass not available: {e}"))?;
    Ok(review.output)
}

#[cfg(test)]
mod resolve_tests {
    use super::*;
    use std::fs;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("mag-sup-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("evals/cases/zz-case")).unwrap();
        root
    }

    fn write_case(root: &std::path::Path, target: &str) {
        let run = serde_json::json!({
            "goal": "g", "scope": [], "outcome": {"achieved": false},
            "trajectory": [],
            "verify_command": "true",
            "verify_target": target,
        });
        fs::write(
            root.join("evals/cases/zz-case/run.json"),
            serde_json::to_string(&run).unwrap(),
        )
        .unwrap();
    }

    fn input(root: &std::path::Path) -> ResolveInput<'_> {
        ResolveInput {
            goal_or_case: "zz-case",
            root,
            workdir: root,
            verify: None,
            target: None,
        }
    }

    #[test]
    fn resolve_rejects_a_case_verify_target_outside_the_root() {
        let root = tmp_root("out");
        write_case(&root, "/etc");
        let err = resolve(&input(&root)).unwrap_err();
        assert!(
            err.contains("outside"),
            "an untrusted case verify_target that escapes the root is rejected: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_accepts_a_relative_verify_target_inside_the_root() {
        let root = tmp_root("in");
        fs::create_dir_all(root.join("crates/x")).unwrap();
        write_case(&root, "crates/x");
        let spec = resolve(&input(&root)).unwrap();
        assert!(
            spec.target.is_absolute() && spec.target.starts_with(&root),
            "relative target resolves inside the root: {:?}",
            spec.target
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_keeps_an_explicit_operator_target_unresolved() {
        let root = tmp_root("opt");
        write_case(&root, "this-is-overridden");
        let spec = resolve(&ResolveInput {
            target: Some(std::path::Path::new("/etc")),
            ..input(&root)
        })
        .unwrap();
        assert_eq!(spec.target, std::path::PathBuf::from("/etc"));
        let _ = fs::remove_dir_all(&root);
    }
}
