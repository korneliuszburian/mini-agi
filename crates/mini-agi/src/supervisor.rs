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
    /// Run report path (default: workdir/REPORT.md).
    pub report: Option<&'a Path>,
}

/// A fully supervised run result.
pub struct SupervisorResult {
    /// The iteration result from the core.
    pub iteration: IterationResult,
    /// The report path written.
    pub report_path: PathBuf,
    /// The progress.md path written.
    pub progress_path: PathBuf,
}

/// Run the supervised verified-iteration loop: writes progress.md per
/// attempt, runs the core, writes the run report, invokes the on-done
/// hook. Returns the result (exit semantics belong to the caller: 0
/// passed, 1 exhausted, 3 aborted).
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
        step_cap: args.step_cap,
        no_sandbox: args.no_sandbox,
        worker_name: args.worker_name,
        read_only: args.read_only,
        iterate: args.iterate,
        blind_worker: args.blind_worker,
        hidden_dir: args.hidden_dir,
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
        };
        // Best-effort append: a progress write failure must not kill the
        // supervision.
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&progress_path)
            .and_then(|mut f| f.write_all(line.as_bytes()));
    })?;

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
        "- total wall: {}s | ~{} tokens (transcript bytes / 4)",
        iteration.total_wall,
        iteration.total_bytes / 4
    );
    let _ = writeln!(report, "- run.json: {}/run.json", args.workdir.display());
    let _ = writeln!(report, "\n## attempt chain");
    for v in &iteration.attempt_verdicts {
        let _ = writeln!(report, "- {v}");
    }
    std::fs::write(&report_path, report).map_err(|e| e.to_string())?;

    // On-done hook: report path + outcome as args (notification point).
    if let Some(cmd) = args.on_done {
        let outcome = if iteration.aborted {
            "3"
        } else if iteration.verifier_passed {
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
    })
}
