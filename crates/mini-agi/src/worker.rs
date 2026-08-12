//! Codex worker orchestration (hardening audit C.6: extracted from
//! `main.rs`): run codex on a slice spec under the wall/step caps and
//! the Landlock sandbox, capture the transcript, emit a truthful
//! run.json draft. The reparse path rebuilds a draft from an existing
//! log without re-running codex.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[cfg(target_os = "linux")]
use crate::sandbox;

fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::from(1)
}

/// Rebuild a run.json draft from an existing transcript log (no codex
/// run). `--verify`/`--target` may be supplied; otherwise the draft
/// carries null verifier fields (the caller decides).
pub fn cmd_codex_reparse(
    log: &Path,
    workdir: &Path,
    run_out: Option<&Path>,
    verify: Option<&str>,
    target: Option<&str>,
) -> ExitCode {
    use mini_agi_core::capture;
    let text = match std::fs::read_to_string(log) {
        Ok(t) => t,
        Err(e) => return fail(&format!("cannot read log {}: {e}", log.display())),
    };
    let outcome = capture::CaptureOutcome {
        log_path: log.to_path_buf(),
        steps: capture::parse_transcript(&text),
        completed: capture::completed(&text),
        result: capture::extract_result(&text),
    };
    println!(
        "reparse: {} captured steps, completed={}",
        outcome.steps.len(),
        outcome.completed
    );
    for step in &outcome.steps {
        println!(
            "  [{}] {}",
            step.tool,
            step.action.chars().take(90).collect::<String>()
        );
    }
    let goal = outcome.result.as_deref().unwrap_or("(goal not extracted)");
    let run = crate::clifmt::build_run_draft(goal, &[], &outcome.steps, verify, target, None);
    write_draft(run_out, workdir, &run)
}

/// Codex run contract (bundled to keep the worker entry point
/// readable; the hardening audit C.6 extraction).
#[derive(Debug, Clone)]
pub struct CodexRunArgs<'a> {
    /// Slice spec path.
    pub spec: &'a Path,
    /// Scratch workdir.
    pub workdir: &'a Path,
    /// Where to write the draft (default: workdir/run.json).
    pub run_out: Option<&'a Path>,
    /// Deterministic verifier command (P0-3).
    pub verify: Option<&'a str>,
    /// Verifier target repo.
    pub target: Option<&'a str>,
    /// Wall-time cap in seconds.
    pub max_wall: Option<u64>,
    /// Step cap.
    pub max_steps: Option<usize>,
    /// Skip the Landlock sandbox (ADR-0012 escape hatch).
    pub no_sandbox: bool,
    /// Worker executable name (multi-worker, production-readiness P2/E;
    /// default "codex").
    pub worker_name: Option<String>,
    /// HITL approval reason (production-readiness D.4): required when
    /// the workdir config sets `require_approval`; the decision is
    /// logged to the action log.
    pub approve: Option<String>,
    /// Verified-iteration loop attempts (BREAKTHROUGH): on verifier
    /// failure, re-invoke the worker with the distilled failure
    /// register. 1 = single shot (default).
    pub iterate: usize,
    /// Blind-worker mode (EXP-012's isolation as a capability): the
    /// verifier's hidden suite is moved away during the worker's run so
    /// the worker genuinely cannot self-verify — the kernel's loop is
    /// the ONLY feedback path. Requires `hidden_dir`.
    pub blind_worker: bool,
    /// The verifier's private hidden-suite directory (moved away during
    /// a blind-worker run, restored before verification).
    pub hidden_dir: Option<std::path::PathBuf>,
}

/// Process-wide nonce for session-ownership markers.
static SESSION_MARKER_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Inputs for the verified-iteration core (AFK supervisor S1: the
/// iteration loop extracted from `cmd_codex` so the supervisor reuses
/// it instead of duplicating).
// The bools mirror the CLI flags one-to-one (no_sandbox, read_only,
// blind_worker, resume).
#[allow(clippy::struct_excessive_bools)]
pub struct IterationInput<'a> {
    /// The slice spec text (prompt base for attempt 1).
    pub spec_text: &'a str,
    /// The goal (parsed from the spec).
    pub goal: &'a str,
    /// The parsed scope list.
    pub scope_list: &'a [String],
    /// The deterministic verifier command (P0-3 enforced).
    pub verify: &'a str,
    /// The verifier target dir.
    pub target: &'a Path,
    /// The worker workdir.
    pub workdir: &'a Path,
    /// Wall cap per attempt.
    pub wall_cap: Option<u64>,
    /// Idle cap per attempt, overriding the configured value.
    pub max_idle: Option<u64>,
    /// Step cap (accumulated).
    pub step_cap: Option<usize>,
    /// Skip the sandbox (ADR-0012 escape hatch).
    pub no_sandbox: bool,
    /// Worker executable name.
    pub worker_name: &'a str,
    /// Read-only sandbox mode (D.2).
    pub read_only: bool,
    /// Iteration count (1 = single shot).
    pub iterate: usize,
    /// Blind-worker mode (EXP-012 isolation).
    pub blind_worker: bool,
    /// Hidden-suite dir for blind-worker.
    pub hidden_dir: Option<&'a Path>,
    /// Session resume (AFK v2, Sandcastle parity): on verifier failure
    /// the next attempt resumes the worker's OWN codex session with the
    /// distilled failure, instead of a cold re-invoke. Falls back to a
    /// fresh exec when no session was captured.
    pub resume: bool,
}

/// A supervision event from the iteration core (the AFK supervisor's
/// progress sink; `cmd_codex` ignores them beyond printing).
#[derive(Debug)]
pub enum ProgressEvent {
    /// One worker attempt started.
    AttemptStarted { attempt: usize },
    /// The verifier verdict for an attempt.
    Verifier {
        attempt: usize,
        failed_cases: Vec<String>,
        passed: bool,
    },
    /// The run aborted (budget cap).
    Aborted { reason: String },
    /// The worker's own session will be resumed (attempt > 1).
    SessionResumed { attempt: usize, session_id: String },
}

/// Completion-grace decision (two-phase timeout S3, codex review F3):
/// a worker killed by a cap is NOT an abort when its transcript already
/// contains the completion marker — the file-redirect design keeps the
/// full transcript readable after the kill, so the attempt resolves as
/// success-with-warning instead of lost work.
#[must_use]
pub const fn attempt_grace(worker_aborted: bool, completed: bool) -> bool {
    worker_aborted && completed
}

/// The verified-iteration result (the draft + supervision metadata).
#[derive(Debug)]
// The AFK supervisor (S2) consumes all fields; `cmd_codex` reads the
// subset it needs today.
#[allow(dead_code)]
pub struct IterationResult {
    /// Number of attempts executed.
    pub attempts_done: usize,
    /// Whether the verifier passed on the final attempt.
    pub verifier_passed: bool,
    /// Whether the run aborted (budget cap).
    pub aborted: bool,
    /// Completion grace fired: a cap-killed worker still delivered its
    /// full completed transcript (success-with-warning).
    pub completion_grace: bool,
    /// The worker's last captured codex session id (AFK v2): the
    /// supervisor's fix attempt resumes it.
    pub resume_session: Option<String>,
    /// All captured steps across attempts.
    pub all_steps: Vec<mini_agi_core::capture::CapturedStep>,
    /// Per-attempt verdicts (process supervision).
    pub attempt_verdicts: Vec<serde_json::Value>,
    /// Accumulated wall time.
    pub total_wall: u64,
    /// Accumulated transcript bytes.
    pub total_bytes: u64,
    /// The run.json draft (trace header + attempts + verdicts).
    pub run: serde_json::Value,
}

/// Find the session whose rollout file CONTAINS the ownership marker
/// (AFK v2 session resume, codex review F1): content match establishes
/// ownership — the worker's session is the one that recorded OUR prompt
/// (the marker is embedded in the base prompt). A newest-file heuristic
/// would attribute ANY concurrent codex process's session (e.g. an IDE
/// session) to the worker; content matching cannot.
/// How many newest sessions the marker scan reads before giving up.
///
/// The marker is embedded in THIS run's prompt, so the owning session is
/// brand new and always near the top of the newest-first order. Without
/// the bound the scan reads the whole `~/.codex/sessions` tree (observed
/// at 30 GB / 3372 rollout files — a minutes-long hang on every run);
/// the marker match lives in the newest handful, so the cap is a
/// correctness-neutral bound, not a heuristic.
const MAX_SESSION_SCAN: usize = 200;

fn find_session_with_marker(home: &Path, marker: &str) -> Option<String> {
    let sessions_root = home.join(".codex/sessions");
    let mut files: Vec<PathBuf> = Vec::new();
    collect_session_files(&sessions_root, &mut files);
    // Newest first: the worker's session is the newest file carrying the
    // marker (the marker appears in exactly one session — ours). Only the
    // newest MAX_SESSION_SCAN files are read; older sessions cannot hold
    // our marker (this run wrote it minutes ago).
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    for p in files.into_iter().rev().take(MAX_SESSION_SCAN) {
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        if text.contains(marker) {
            let name = p.file_name()?.to_string_lossy().into_owned();
            let body = name.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
            let parts: Vec<&str> = body.split('-').collect();
            if parts.len() < 5 {
                continue;
            }
            return Some(parts[parts.len() - 5..].join("-"));
        }
    }
    None
}

/// Recursive collector of rollout files for `find_session_with_marker`.
fn collect_session_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_session_files(&p, out);
            continue;
        }
        let Some(name) = p.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if name.to_lowercase().ends_with(".jsonl") && name.starts_with("rollout-") {
            out.push(p);
        }
    }
}

/// Resume decision (AFK v2): resume only when enabled, on a re-attempt,
/// and a worker session was actually captured; otherwise the loop falls
/// back to the cold re-invoke.
#[must_use]
pub const fn should_resume(resume: bool, attempt: usize, session: Option<&str>) -> bool {
    resume && attempt > 1 && session.is_some()
}

/// The verified-iteration core (BREAKTHROUGH): run the worker up to N
/// attempts; on verifier failure distill the failure register and
/// re-invoke a fresh worker, bounded by budget caps; the verifier must
/// be non-vacuous before iterating. `progress` receives supervision
/// events (the AFK supervisor writes progress.md from them).
pub fn run_verified_iteration(
    input: &IterationInput<'_>,
    mut progress: impl FnMut(ProgressEvent),
) -> Result<IterationResult, String> {
    let protocol = "IMPLEMENTATION PROTOCOL (binding): plan first, tests first, never repeat a failing action. When the work is done and your own gate passes, END YOUR FINAL MESSAGE with:\n<promise>COMPLETE</promise>\n<result>{\"summary\": \"one sentence\"}</result>";
    // Session-ownership marker (codex review F1): a run-unique string
    // embedded ONLY in the worker's prompt; the session whose rollout
    // file contains it is provably the worker's (content match beats the
    // newest-file heuristic under concurrent codex processes). The
    // marker is a hash of (now, pid, counter) — unpredictable, so no
    // other process can accidentally record it. It is an OWNERSHIP
    // token, not an auth boundary.
    let marker_nonce = SESSION_MARKER_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let marker = {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::hash::RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.write_u64(std::process::id().into());
        h.write_u64(marker_nonce);
        let tag = std::env::var("MINIAGI_SESSION_TAG").unwrap_or_default();
        format!(
            "SESS-OWN-{:016x}{}",
            h.finish(),
            if tag.is_empty() {
                String::new()
            } else {
                format!("-{tag}")
            }
        )
    };
    let base_prompt = format!(
        "{}\n\n{protocol}\n\n<session-marker>{marker}</session-marker>\n",
        input.spec_text
    );
    let log_path = input.workdir.join("codex.log");
    let mut all_steps: Vec<mini_agi_core::capture::CapturedStep> = Vec::new();
    let mut attempt_verdicts: Vec<serde_json::Value> = Vec::new();
    let mut failure_context = String::new();
    let mut final_wall = 0u64;
    let mut total_wall = 0u64;
    let mut total_bytes = 0u64;
    let mut total_tokens_in = 0u64;
    let mut total_tokens_out = 0u64;
    let mut total_cost = 0.0f64;
    let mut attempts_done = 0;
    let mut aborted = false;
    let mut completion_grace = false;
    let mut verifier_passed = false;
    let iterations = input.iterate.max(1);
    // S2: verify-audit wired into the loop — before trusting the
    // iteration, confirm the verifier is non-vacuous.
    if iterations > 1 {
        let audit = mini_agi_core::verifier::audit_verifier_vacuous(input.verify)
            .map_err(|e| format!("verify-audit: {e}"))?;
        if audit.is_vacuous {
            return Err(
                "refusing verified-iteration: the verifier is VACUOUS (passes an empty \
                 target) — fix the verifier or drop --iterate (verify-audit)"
                    .to_string(),
            );
        }
    }
    let mut resume_session: Option<String> = None;
    for attempt in 1..=iterations {
        attempts_done = attempt;
        progress(ProgressEvent::AttemptStarted { attempt });
        let prompt = if attempt == 1 {
            base_prompt.clone()
        } else {
            format!(
                "{base_prompt}\n\nFAILURE FEEDBACK FROM ATTEMPT {prev} (binding — fix these):\n{failure_context}\nStart from your last state, address each failing case, and re-run the verifier until it passes.\n",
                prev = attempt - 1
            )
        };
        // Resume the worker's OWN session (AFK v2, Sandcastle parity):
        // the distilled failure goes back into the same context instead
        // of a cold re-invoke; falls back to a fresh exec when no
        // session was captured (or --no-resume).
        let resuming = should_resume(input.resume, attempt, resume_session.as_deref());
        let worker_kind = worker_kind(input.worker_name);
        let worker_args = if resuming {
            progress(ProgressEvent::SessionResumed {
                attempt,
                session_id: resume_session.clone().unwrap_or_default(),
            });
            build_worker_args(&worker_kind, true, resume_session.as_deref(), &prompt)
        } else {
            build_worker_args(&worker_kind, false, None, &prompt)
        };
        // Blind-worker mode: the hidden suite is unavailable to the
        // worker — the kernel's loop is the only feedback path.
        let hidden_away = if input.blind_worker {
            match input.hidden_dir {
                // A hide failure is NOT silently swallowed: the
                // blind-worker isolation claim would be false while the
                // worker still sees the hidden suite (silent
                // degradation of the experiment boundary).
                Some(dir) if dir.exists() => hide_verifier(dir)
                    .map_err(|e| format!("cannot isolate the hidden suite: {e}"))?,
                Some(dir) => {
                    return Err(format!(
                        "refusing blind-worker mode: the hidden suite {} does not exist — \
                         isolation cannot be guaranteed",
                        dir.display()
                    ));
                }
                None => {
                    return Err(
                        "refusing blind-worker mode without --hidden-dir (the isolation \
                         requires the verifier's hidden suite to be movable)"
                            .to_string(),
                    );
                }
            }
        } else {
            false
        };
        let idle_cap = input
            .max_idle
            .or_else(|| mini_agi_core::config::Config::load(input.workdir).max_idle_seconds);
        // Session resume (AFK v2): ownership via the marker — the
        // worker's session is the one containing OUR marker.
        // The marker scan reads codex's own session tree; opencode keeps
        // its sessions in its own store and resumes via `--continue`/`-s`
        // (D1), so the scan runs for codex workers only.
        let session_before = if matches!(worker_kind, WorkerKind::Codex) {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .and_then(|h| find_session_with_marker(&h, &marker))
        } else {
            None
        };
        let worker = match run_worker_sandboxed(
            input.worker_name,
            input.workdir,
            input.no_sandbox,
            input.read_only,
            input.wall_cap,
            idle_cap,
            &worker_args.iter().map(String::as_str).collect::<Vec<_>>(),
        ) {
            Ok(w) => w,
            Err(e) => {
                if hidden_away && let Some(dir) = input.hidden_dir {
                    let _ = restore_verifier(dir);
                }
                return Err(format!("{} not available: {e}", input.worker_name));
            }
        };
        if hidden_away && let Some(dir) = input.hidden_dir {
            let _ = restore_verifier(dir);
        }
        let worker_session = std::env::var_os("HOME")
            .map(PathBuf::from)
            .and_then(|h| find_session_with_marker(&h, &marker))
            .filter(|id| session_before.as_ref() != Some(id));
        if worker_session.is_some() {
            resume_session.clone_from(&worker_session);
        }
        let combined = worker.output;
        // D1 telemetry: the opencode adapter reports usage in its JSON
        // stream; the kernel folds it into the run draft (codex runs
        // keep reporting None).
        let worker_usage = if matches!(worker_kind, WorkerKind::OpenCode { .. }) {
            mini_agi_core::worker::parse_opencode_usage(&combined)
        } else {
            None
        };
        if let Some(u) = worker_usage {
            total_tokens_in += u.tokens_in;
            total_tokens_out += u.tokens_out;
            total_cost += u.cost_usd;
        }
        final_wall = worker.wall_seconds;
        total_wall += worker.wall_seconds;
        total_bytes += combined.len() as u64;
        std::fs::write(
            &log_path,
            format!("{combined}\n--- attempt {attempt} ---\n"),
        )
        .unwrap_or(());
        let stripped = combined.replace(&prompt, "");
        let outcome = mini_agi_core::capture::CaptureOutcome {
            log_path: log_path.clone(),
            steps: mini_agi_core::capture::parse_transcript(&combined),
            completed: mini_agi_core::capture::completed(&stripped),
            result: mini_agi_core::capture::extract_result(&combined),
        };
        all_steps.extend(outcome.steps.iter().cloned());
        // Completion grace (two-phase timeout S3): a cap-killed worker
        // whose transcript already carries the completion marker counts
        // as success-with-warning — the attempt is not aborted.
        let grace = attempt_grace(worker.aborted, outcome.completed);
        // P0-1 post-hoc cap check (accumulated).
        let violations = mini_agi_core::worker::budget_violations(
            all_steps.len(),
            0.0,
            final_wall,
            input.step_cap,
            None,
            input.wall_cap,
        );
        aborted = worker.aborted || !violations.is_empty();
        for v in &violations {
            eprintln!("  [abort] {v}");
        }
        if worker.aborted {
            eprintln!(
                "  [abort] worker killed by the wall-time cap ({:?}s)",
                input.wall_cap
            );
        }
        if grace {
            completion_grace = true;
        }
        if aborted {
            progress(ProgressEvent::Aborted {
                reason: "budget cap".to_string(),
            });
            break;
        }
        // S4: total-budget governor across ALL attempts.
        let cfg = mini_agi_core::config::Config::load(input.workdir);
        if let Some(max_tokens) = cfg.max_tokens
            && total_bytes / 4 > max_tokens
        {
            eprintln!(
                "  [abort] total token budget exceeded: ~{} tokens > max {max_tokens}",
                total_bytes / 4
            );
            aborted = true;
            progress(ProgressEvent::Aborted {
                reason: "total token budget".to_string(),
            });
            break;
        }
        if let Some(max_wall) = input.wall_cap
            && total_wall > max_wall.saturating_mul(iterations as u64)
        {
            eprintln!(
                "  [abort] total wall budget exceeded: {total_wall}s > max {max_wall}s x {iterations} attempts"
            );
            aborted = true;
            progress(ProgressEvent::Aborted {
                reason: "total wall budget".to_string(),
            });
            break;
        }
        // Single shot: the kernel does not drive iteration.
        if iterations == 1 {
            break;
        }
        // Verified-iteration (BREAKTHROUGH): run the deterministic
        // verifier; on failure distill the feedback and re-invoke.
        let verifier =
            mini_agi_core::worker::run_capped("sh", &["-c", input.verify], input.target, Some(120));
        match verifier {
            Ok(v) if v.status == Some(0) && !v.aborted => {
                verifier_passed = true;
                attempt_verdicts.push(serde_json::json!({
                    "attempt": attempt,
                    "failed_cases": [],
                    "passed": true,
                }));
                progress(ProgressEvent::Verifier {
                    attempt,
                    failed_cases: Vec::new(),
                    passed: true,
                });
                break;
            }
            Ok(v) => {
                let failed_cases = extract_failing_cases(&v.output);
                attempt_verdicts.push(serde_json::json!({
                    "attempt": attempt,
                    "failed_cases": failed_cases.clone(),
                    "passed": false,
                }));
                failure_context = distill_failure(attempt, &v.output);
                progress(ProgressEvent::Verifier {
                    attempt,
                    failed_cases,
                    passed: false,
                });
            }
            Err(e) => return Err(format!("verifier not available: {e}")),
        }
    }
    let run = crate::clifmt::build_run_draft(
        input.goal,
        input.scope_list,
        &all_steps,
        Some(input.verify),
        Some(&input.target.to_string_lossy()),
        Some(final_wall),
    );
    let mut run = run;
    run["attempts"] = serde_json::json!(attempts_done);
    run["verifier_passed"] = serde_json::json!(verifier_passed);
    run["attempt_verdicts"] = serde_json::json!(attempt_verdicts);
    // D1 layered economics: fold the adapter's telemetry into the draft.
    run["worker"] = serde_json::json!(input.worker_name);
    run["cost_usd"] = serde_json::json!(total_cost);
    run["tokens_total"] = serde_json::json!(total_tokens_in + total_tokens_out);
    run["usage"] = serde_json::json!({
        "tokens_in": total_tokens_in,
        "tokens_out": total_tokens_out,
        "cost_usd": total_cost,
    });
    Ok(IterationResult {
        attempts_done,
        verifier_passed,
        aborted,
        completion_grace,
        resume_session,
        all_steps,
        attempt_verdicts,
        total_wall,
        total_bytes,
        run,
    })
}

/// Run codex on a slice spec, capture the transcript, emit a truthful
/// run.json draft under the wall/step caps and (Linux) the Landlock
/// sandbox.
pub fn cmd_codex(args: &CodexRunArgs<'_>) -> ExitCode {
    let spec = args.spec;
    let workdir = args.workdir;
    let run_out = args.run_out;
    let verify = args.verify;
    let target = args.target;
    let max_wall = args.max_wall;
    let max_steps = args.max_steps;
    let no_sandbox = args.no_sandbox;
    let iterate = args.iterate;
    let blind_worker = args.blind_worker;
    let hidden_dir = args.hidden_dir.as_deref();
    let spec_text = match std::fs::read_to_string(spec) {
        Ok(t) => t,
        Err(e) => return fail(&format!("cannot read spec {}: {e}", spec.display())),
    };
    // P0-3 (hardening audit C.3): refuse to START a worker whose spec
    // declares no verifier — the `--verify`/`--target` flags take
    // precedence, otherwise the spec's embedded verify_command is used;
    // with neither the run would be trust-only and must not execute.
    let Some(verify) = verify.map(str::to_owned).or_else(|| {
        spec_text
            .lines()
            .find_map(|l| l.strip_prefix("- verify_command: "))
            .map(|l| l.split(" in ").next().unwrap_or("").trim().to_owned())
            .filter(|s| !s.is_empty())
    }) else {
        return fail(
            "refusing to run codex: spec declares no verifier and --verify was not given \
             (P0-3 no-dispatch-without-verifier)",
        );
    };
    let Some(target) = target.map(str::to_owned).or_else(|| {
        spec_text
            .lines()
            .find_map(|l| l.strip_prefix("- verify_command: "))
            .map(|l| l.split(" in ").nth(1).unwrap_or_default().trim().to_owned())
            .filter(|s| !s.is_empty())
    }) else {
        return fail(
            "refusing to run codex: spec declares no verify target and --target was not given \
             (P0-3 no-dispatch-without-verifier)",
        );
    };
    std::fs::create_dir_all(workdir).unwrap_or(());
    let goal = spec_text
        .lines()
        .find_map(|l| l.strip_prefix("- goal: "))
        .unwrap_or("(goal not parsed from spec)")
        .to_string();
    let scope = spec_text
        .lines()
        .find_map(|l| l.strip_prefix("- scope: "))
        .unwrap_or("")
        .to_string();
    let scope_list: Vec<String> = scope
        .split(',')
        .map(|s| s.trim().trim_matches('`').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let cfg = mini_agi_core::config::Config::load(workdir);
    let wall_cap = max_wall.or(cfg.max_wall_seconds);
    let step_cap = max_steps.or(cfg.max_steps);
    let read_only = is_read_only_spec(&spec_text);
    let worker_name = resolve_worker_name(args.worker_name.as_deref());
    // HITL approval gate (production-readiness D.4 / ADR-0014): when the
    // worker's config requires approval, a run without --approve refuses
    // BEFORE spawning the worker; an approved run logs the decision to
    // the action log.
    if mini_agi_core::config::Config::load(workdir).require_approval {
        match &args.approve {
            Some(reason) => {
                let _ = mini_agi_core::audit::append_action(workdir, "approval", "human", reason);
            }
            None => {
                return fail(
                    "refusing to run the worker: config require_approval is set and \
                     --approve <reason> was not given (HITL approval gate, ADR-0014 D.4)",
                );
            }
        }
    }
    let input = IterationInput {
        spec_text: &spec_text,
        goal: &goal,
        scope_list: &scope_list,
        verify: &verify,
        target: std::path::Path::new(&target),
        workdir,
        wall_cap,
        max_idle: None,
        step_cap,
        no_sandbox,
        worker_name,
        read_only,
        iterate,
        blind_worker,
        hidden_dir,
        resume: false,
    };
    let result = match run_verified_iteration(&input, |event| match event {
        ProgressEvent::AttemptStarted { attempt } => {
            println!("{worker_name} attempt {attempt} started");
        }
        ProgressEvent::Verifier {
            attempt,
            failed_cases,
            passed,
        } => {
            if passed {
                println!("  verifier PASSED on attempt {attempt}");
            } else {
                println!(
                    "  verifier FAILED on attempt {attempt}: {} case(s)",
                    failed_cases.len()
                );
            }
        }
        ProgressEvent::Aborted { reason } => {
            eprintln!("  [abort] {reason}");
        }
        ProgressEvent::SessionResumed {
            attempt,
            session_id,
        } => {
            println!("  resuming session {session_id} for attempt {attempt}");
        }
    }) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    let run = result.run;
    let exit = write_draft(run_out, workdir, &run);
    if result.aborted {
        println!("  run ABORTED by a budget cap (exit 3) — not a clean run");
        ExitCode::from(3)
    } else if iterate.max(1) > 1 && !result.verifier_passed {
        println!(
            "  run did NOT pass the verifier after {} attempts (exit 1)",
            result.attempts_done
        );
        ExitCode::from(1)
    } else {
        exit
    }
}

/// Move the hidden suite aside for a blind worker run.
///
/// A pre-existing `*.blind-hidden` dir is a CRASHED run's hidden suite
/// (gitignored user data, often the only copy) — it must NEVER be
/// deleted, and the isolation must not silently proceed without it: the
/// stale state is an error the operator resolves.
fn hide_verifier(hidden_dir: &Path) -> std::io::Result<bool> {
    if !hidden_dir.exists() {
        return Ok(false);
    }
    let away = hidden_dir.with_extension("blind-hidden");
    if away.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "stale blind-hidden state at {} — a previous blind run crashed mid-hide; \
                 resolve it (restore or remove) before re-running",
                away.display()
            ),
        ));
    }
    std::fs::rename(hidden_dir, &away)?;
    Ok(true)
}

/// Restore the hidden-suite directory after the worker run.
fn restore_verifier(hidden_dir: &Path) -> std::io::Result<bool> {
    let away = hidden_dir.with_extension("blind-hidden");
    if !away.exists() {
        return Ok(false);
    }
    std::fs::rename(&away, hidden_dir)?;
    Ok(true)
}

/// Extract the failing case names from a unittest-style verifier report
/// (lines like `FAIL: test_x` / `ERROR: test_y`).
fn extract_failing_cases(verifier_output: &str) -> Vec<String> {
    verifier_output
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t
                .strip_prefix("FAIL: ")
                .or_else(|| t.strip_prefix("ERROR: "))?;
            let name = rest.split_whitespace().next().unwrap_or(rest);
            Some(name.to_string())
        })
        .collect()
}

/// Extract per-case details from a unittest-style verifier report:
/// (failing-case-name, detail-line) pairs, where the detail line is the
/// AssertionError/Error message that follows the case's traceback.
fn extract_case_details(verifier_output: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = verifier_output.lines().collect();
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in lines {
        let t = line.trim();
        if let Some(rest) = t
            .strip_prefix("FAIL: ")
            .or_else(|| t.strip_prefix("ERROR: "))
        {
            let name = rest.split_whitespace().next().unwrap_or(rest).to_string();
            current = Some(name);
        } else if let Some(cur) = &current {
            if let Some(msg) = t.strip_prefix("AssertionError: ") {
                out.push((cur.clone(), msg.to_string()));
                current = None;
            } else if !t.is_empty() && !t.starts_with('-') && !t.starts_with("Traceback") {
                // First meaningful non-traceback line as a fallback detail.
                out.push((cur.clone(), t.to_string()));
                current = None;
            }
        }
    }
    out
}

/// Distill a verifier failure into a compact, binding instruction for
/// the next iteration (BREAKTHROUGH; Reflexion-style test-grounded
/// feedback). ESCALATES specificity across attempts (S8 research:
/// feedback quality is the bottleneck): attempt 1 lists the failing
/// case names; later attempts add each case's expected/got detail.
fn distill_failure(attempt: usize, verifier_output: &str) -> String {
    let cases = extract_failing_cases(verifier_output);
    if cases.is_empty() {
        let excerpt: String = verifier_output.chars().take(600).collect();
        return format!(
            "- the verifier FAILED on attempt {attempt}. Its output (fix the failing cases; do not repeat them):\n{excerpt}"
        );
    }
    let mut out = format!(
        "- the verifier FAILED on attempt {attempt}. The failing cases (fix each; do not repeat them):\n"
    );
    let details = extract_case_details(verifier_output);
    for (i, c) in cases.iter().enumerate() {
        use std::fmt::Write as _;
        let _ = write!(out, "  {}. {c}", i + 1);
        if attempt > 1
            && let Some((_, detail)) = details.iter().find(|(n, _)| n == c)
        {
            let _ = write!(out, " — {detail}");
        }
        let _ = writeln!(out);
    }
    out
}

fn write_draft(run_out: Option<&Path>, workdir: &Path, run: &serde_json::Value) -> ExitCode {
    let out_path = run_out.unwrap_or(&workdir.join("run.json")).to_path_buf();
    match std::fs::write(
        &out_path,
        serde_json::to_string_pretty(run).unwrap_or_default(),
    ) {
        Ok(()) => {
            println!("  run draft: {}", out_path.display());
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot write run draft: {e}")),
    }
}

/// Run the codex worker, routing it through the Landlock wrapper on
/// Linux (ADR-0012) unless `no_sandbox`. The wrapper self-spawns
/// (`exec-sandbox`), applies write-containment, then runs codex.
/// Resolve the worker executable name (multi-worker, production-readiness
/// P2/E): `None` defaults to `codex`.
fn resolve_worker_name(name: Option<&str>) -> &str {
    name.unwrap_or("codex")
}

/// Which worker adapter drives the run (D1 layered economics).
///
/// `codex` (default) keeps the existing exec/resume contract. Any
/// `opencode`-prefixed name selects the thin opencode adapter: the same
/// budget/sandbox/capture contract, but `run --format json` with the
/// usage telemetry parsed back into the run.
#[derive(Debug, Clone, PartialEq)]
enum WorkerKind {
    Codex,
    OpenCode { model: Option<String> },
}

fn worker_kind(name: &str) -> WorkerKind {
    name.strip_prefix("opencode-").map_or_else(
        || {
            if name == "opencode" {
                WorkerKind::OpenCode { model: None }
            } else {
                WorkerKind::Codex
            }
        },
        |model| WorkerKind::OpenCode {
            model: Some(model.to_string()),
        },
    )
}

/// Build the worker argv for one attempt (D1). Codex keeps the existing
/// `exec`/`resume` shapes byte-identical; opencode maps them to
/// `run --format json [-m <model>] [--continue|-s <session>] <prompt>`
/// (opencode 1.18.11 CLI, grounded 2026-08-06).
fn build_worker_args(
    kind: &WorkerKind,
    resuming: bool,
    session_id: Option<&str>,
    prompt: &str,
) -> Vec<String> {
    match kind {
        WorkerKind::Codex => {
            if resuming {
                vec![
                    "exec".to_string(),
                    "resume".to_string(),
                    session_id.unwrap_or("").to_string(),
                    "--skip-git-repo-check".to_string(),
                    prompt.to_string(),
                ]
            } else {
                vec![
                    "exec".to_string(),
                    "-s".to_string(),
                    "workspace-write".to_string(),
                    "--skip-git-repo-check".to_string(),
                    prompt.to_string(),
                ]
            }
        }
        WorkerKind::OpenCode { model } => {
            let mut args = vec![
                "run".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ];
            if let Some(m) = model {
                args.push("-m".to_string());
                args.push(m.clone());
            }
            if resuming {
                if let Some(s) = session_id.filter(|s| !s.is_empty()) {
                    args.push("-s".to_string());
                    args.push(s.to_string());
                } else {
                    args.push("--continue".to_string());
                }
            }
            // `--` ends flag parsing: the spec prompt starts with
            // `- goal:` and yargs would misread it as flags (observed:
            // opencode dumped --help instead of running).
            args.push("--".to_string());
            args.push(prompt.to_string());
            args
        }
    }
}

/// Production-readiness D.2: does the spec declare a read-only sandbox?
/// The flag is the EXACT value `read-only`; a prefixed variant
/// (`read-only-more`) is a different declaration and must not match.
fn is_read_only_spec(spec_text: &str) -> bool {
    spec_text.lines().any(|l| {
        l.trim_start()
            .strip_prefix("- sandbox:")
            .is_some_and(|rest| rest.trim() == "read-only")
    })
}

pub fn run_worker_sandboxed(
    worker_name: &str,
    workdir: &Path,
    no_sandbox: bool,
    read_only: bool,
    wall_cap: Option<u64>,
    idle_cap: Option<u64>,
    worker_args: &[&str],
) -> std::io::Result<mini_agi_core::worker::WorkerResult> {
    // D1: the opencode adapter's EXECUTABLE is always `opencode` — the
    // model suffix (`opencode-<model>`) rides in the args, never in the
    // spawn command.
    let exe = match worker_kind(worker_name) {
        WorkerKind::OpenCode { .. } => "opencode",
        WorkerKind::Codex => worker_name,
    };
    #[cfg(target_os = "linux")]
    {
        if !no_sandbox {
            // Production-readiness D.2: least authority — a read-only
            // skill grants NO workdir write access (only codex's own
            // state dir), so the worker cannot modify the tree.
            let mut wrapper = vec!["exec-sandbox".to_string()];
            if !read_only {
                wrapper.push("--allow-write".to_string());
                wrapper.push(workdir.to_string_lossy().into_owned());
            }
            if let Ok(home) = std::env::var("HOME") {
                // EXP-009: npx-style codex wrappers write their package
                // cache under ~/.npm — include it in the default write
                // set or the wrapper fails (EACCES). ~/.codex carries
                // codex's own session state.
                for state_dir in [".codex", ".npm"] {
                    let dir = std::path::Path::new(&home).join(state_dir);
                    if dir.is_dir() {
                        wrapper.push("--allow-write".to_string());
                        wrapper.push(dir.to_string_lossy().into_owned());
                    }
                }
                // D1: the opencode adapter keeps its state under XDG dirs
                // (data db, config+plugins, cache, state locks) AND the
                // goal plugin writes under its own data dir — observed via
                // strace: EACCES on each one kills the server silently.
                // /dev/null needs write for O_RDWR (bun runtime).
                if worker_name.starts_with("opencode") {
                    for state_dir in [
                        ".local/share/opencode",
                        ".local/share/opencode-goal-plugin",
                        ".opencode",
                        ".cache/opencode",
                        ".config/opencode",
                        ".local/state/opencode",
                    ] {
                        let dir = std::path::Path::new(&home).join(state_dir);
                        if dir.is_dir() {
                            wrapper.push("--allow-write".to_string());
                            wrapper.push(dir.to_string_lossy().into_owned());
                        }
                    }
                    // opencode logs under `~/.local/share/opencode/log/`
                    // (observed live: a sandboxed chat worker died with
                    // `PermissionDenied: FileSystem.open (.../log/opencode.log)`
                    // while the CLI ran fine unsandboxed — the log dir was
                    // missing from the write set).
                    let log_dir = std::path::Path::new(&home).join(".local/share/opencode/log");
                    if log_dir.is_dir() {
                        wrapper.push("--allow-write".to_string());
                        wrapper.push(log_dir.to_string_lossy().into_owned());
                    }
                    wrapper.push("--allow-write".to_string());
                    wrapper.push("/dev/null".to_string());
                }
            }
            wrapper.push("--".to_string());
            // The wrapper runs `<worker_name> <worker_args...>` — the
            // command itself is NOT part of worker_args (a real bug the
            // proof-of-advantage experiment caught: the wrapper tried to
            // run `exec` instead of `codex exec`). For the opencode
            // adapter the executable is `opencode`, never the
            // `opencode-<model>` name.
            wrapper.push(exe.to_string());
            wrapper.extend(worker_args.iter().map(|s| (*s).to_string()));
            let arg_refs: Vec<&str> = wrapper.iter().map(String::as_str).collect();
            // A current_exe() failure must NOT silently fall through to
            // an unsandboxed run — the ADR-0012 boundary would be gone
            // without anyone noticing. It is an error.
            let exe_path = std::env::current_exe().map_err(|e| {
                std::io::Error::other(format!("exec-sandbox wrapper unavailable: {e}"))
            })?;
            return mini_agi_core::worker::run_capped_idle(
                &exe_path.to_string_lossy(),
                &arg_refs,
                workdir,
                wall_cap,
                idle_cap,
            );
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (no_sandbox, read_only, wall_cap);
    }
    // Multi-worker (production-readiness P2/E): the runner resolves the
    // worker command from the parameter — codex today, a second type
    // (e.g. claude) behind the same budget/sandbox/capture contract.
    mini_agi_core::worker::run_capped_idle(exe, worker_args, workdir, wall_cap, idle_cap)
}

/// Run one opencode worker invocation (D1 adapter) with the given model
/// and prompt — the dream-loop distiller/auditor seam. Reuses the same
/// budget/sandbox/args contract as loop runs.
pub fn run_opencode_worker(
    workdir: &Path,
    model: &str,
    prompt: &str,
    wall_cap: Option<u64>,
    idle_cap: Option<u64>,
) -> std::io::Result<mini_agi_core::worker::WorkerResult> {
    let kind = worker_kind(model);
    let args = build_worker_args(&kind, false, None, prompt);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut result =
        run_worker_sandboxed(model, workdir, false, true, wall_cap, idle_cap, &arg_refs)?;
    // D1 telemetry: parse the usage from the opencode stream here (the
    // loop path does this in run_verified_iteration; the standalone
    // worker seam must too).
    result.usage = mini_agi_core::worker::parse_opencode_usage(&result.output);
    Ok(result)
}

/// `exec-sandbox`: apply the Landlock write-containment policy to the
/// current process, then run the command after `--` and forward its exit
/// code. Linux-only (ADR-0012); on other targets it is a documented
/// no-op error.
pub fn cmd_exec_sandbox(allow_write: &[PathBuf], command: &[String]) -> ExitCode {
    #[cfg(target_os = "linux")]
    {
        if command.is_empty() {
            return fail("exec-sandbox: no command given after `--`");
        }
        let mut policy = sandbox::SandboxPolicy::new();
        for dir in allow_write {
            policy.allow_write(dir);
        }
        match policy.apply() {
            Ok(()) => {}
            Err(e) => {
                eprintln!("  [warn] sandbox unavailable: {e}");
                eprintln!("  [warn] running the worker UNSANDBOXED (ADR-0012)");
            }
        }
        match std::process::Command::new(&command[0])
            .args(&command[1..])
            .status()
        {
            Ok(s) => ExitCode::from(s.code().and_then(|c| u8::try_from(c).ok()).unwrap_or(1)),
            Err(e) => fail(&format!("exec-sandbox: cannot run {}: {e}", command[0])),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (allow_write, command);
        fail("exec-sandbox: Linux-only (Landlock, ADR-0012)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_verifier_preserves_a_stale_blind_hidden_state() {
        // A pre-existing `*.blind-hidden` dir is a CRASHED run's hidden
        // suite (gitignored, user data, the only copy). hide_verifier
        // used to DELETE it — the next blind run destroyed the suite
        // permanently. It must refuse instead: the operator resolves
        // the stale state, the isolation never silently proceeds.
        let root = std::env::temp_dir().join(format!("mag-hide-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let hidden = root.join("hidden-suite");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("cases.txt"), "precious").unwrap();
        // Stale state from a crashed run: the away dir exists.
        let away = root.join("hidden-suite.blind-hidden");
        std::fs::create_dir_all(&away).unwrap();
        std::fs::write(away.join("cases.txt"), "the-only-copy").unwrap();
        let err = hide_verifier(&hidden).unwrap_err();
        assert!(
            err.to_string().contains("blind-hidden"),
            "the stale state must be named, got {err}"
        );
        assert!(
            std::fs::read_to_string(away.join("cases.txt")).unwrap() == "the-only-copy",
            "the stale copy must survive untouched"
        );
        assert!(hidden.exists(), "the live suite is left in place");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_only_spec_is_detected() {
        assert!(is_read_only_spec("- goal: x\n- sandbox: read-only\n"));
        assert!(!is_read_only_spec("- goal: x\n- sandbox: write\n"));
        assert!(!is_read_only_spec("- goal: x\n"));
        // A workdir write mention must not be confused with the flag.
        assert!(!is_read_only_spec(
            "- goal: x\n- scope: sandbox/read-only\n"
        ));
    }

    #[test]
    fn worker_name_resolves_with_codex_default() {
        // Multi-worker (P2/E): the runner command resolves from the
        // parameter, defaulting to codex.
        assert_eq!(resolve_worker_name(None), "codex");
        assert_eq!(resolve_worker_name(Some("claude")), "claude");
        assert_eq!(resolve_worker_name(Some("codex")), "codex");
    }

    #[test]
    fn worker_kind_resolves_codex_opencode_and_model() {
        assert_eq!(worker_kind("codex"), WorkerKind::Codex);
        assert_eq!(worker_kind("anything-else"), WorkerKind::Codex);
        assert_eq!(
            worker_kind("opencode"),
            WorkerKind::OpenCode { model: None }
        );
        assert_eq!(
            worker_kind("opencode-deepseek-v4-flash"),
            WorkerKind::OpenCode {
                model: Some("deepseek-v4-flash".to_string())
            }
        );
    }

    #[test]
    fn opencode_args_map_run_continue_and_model() {
        // D1 adapter: the codex-shaped resume contract maps to
        // `run --format json` with --continue / -s; the model rides on -m.
        let s = |v: Vec<&str>| v.into_iter().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            build_worker_args(&WorkerKind::OpenCode { model: None }, false, None, "do it"),
            s(vec!["run", "--format", "json", "--", "do it"])
        );
        assert_eq!(
            build_worker_args(
                &WorkerKind::OpenCode {
                    model: Some("deepseek-v4-flash".to_string())
                },
                false,
                None,
                "do it"
            ),
            s(vec![
                "run",
                "--format",
                "json",
                "-m",
                "deepseek-v4-flash",
                "--",
                "do it"
            ])
        );
        assert_eq!(
            build_worker_args(
                &WorkerKind::OpenCode { model: None },
                true,
                Some("ses_abc"),
                "do it"
            ),
            s(vec![
                "run", "--format", "json", "-s", "ses_abc", "--", "do it"
            ])
        );
        assert_eq!(
            build_worker_args(&WorkerKind::OpenCode { model: None }, true, None, "do it"),
            s(vec!["run", "--format", "json", "--continue", "--", "do it"])
        );
    }

    #[test]
    fn codex_args_keep_the_existing_shapes() {
        // Byte-identical to the pre-D1 contract: any drift breaks the
        // codex worker (existing behavior is locked).
        let s = |v: Vec<&str>| v.into_iter().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            build_worker_args(&WorkerKind::Codex, false, None, "do it"),
            s(vec![
                "exec",
                "-s",
                "workspace-write",
                "--skip-git-repo-check",
                "do it"
            ])
        );
        assert_eq!(
            build_worker_args(&WorkerKind::Codex, true, Some("ses_abc"), "do it"),
            s(vec![
                "exec",
                "resume",
                "ses_abc",
                "--skip-git-repo-check",
                "do it"
            ])
        );
    }

    #[test]
    fn codex_resume_without_session_keeps_empty_arg_shape() {
        // The resume contract is positional: a missing session id must
        // stay a literal empty argument (never vanish, never reorder).
        let s = |v: Vec<&str>| v.into_iter().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            build_worker_args(&WorkerKind::Codex, true, None, "do it"),
            s(vec!["exec", "resume", "", "--skip-git-repo-check", "do it"])
        );
        let resume = build_worker_args(&WorkerKind::Codex, true, None, "do it");
        assert_eq!(resume[2], "", "the session slot stays positionally stable");
    }

    #[test]
    fn write_draft_writes_pretty_json_to_default_and_custom_path() {
        let root = std::env::temp_dir().join(format!("mag-wd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let run = serde_json::json!({"goal": "g", "outcome": {"achieved": true}});
        let code = write_draft(None, &root, &run);
        assert_eq!(code, ExitCode::SUCCESS);
        let text = std::fs::read_to_string(root.join("run.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            parsed["outcome"]["achieved"], true,
            "draft must be valid JSON"
        );
        assert!(
            text.contains("\n  "),
            "draft must be pretty-printed, not a single line"
        );
        let custom = root.join("deep/report.json");
        std::fs::create_dir_all(custom.parent().unwrap()).unwrap();
        let code = write_draft(Some(&custom), &root, &run);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(custom.is_file(), "custom run_out path must be honored");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_draft_fails_loudly_on_unwritable_target() {
        let root = std::env::temp_dir().join(format!("mag-wd-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let run = serde_json::json!({"goal": "g"});
        let code = write_draft(Some(&root.join("run.json")), &root, &run);
        assert_eq!(code, ExitCode::SUCCESS);
        // A run_out whose parent is a FILE is unwritable: the default
        // run.json now exists, so the write must fail loudly.
        let blocked = root.join("blocked/run.json");
        std::fs::write(root.join("blocked"), "not a dir").unwrap();
        let code = write_draft(Some(&blocked), &root, &run);
        assert_ne!(code, ExitCode::SUCCESS, "an unwritable target must fail");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn is_read_only_spec_edge_variants() {
        assert!(
            is_read_only_spec("  - sandbox: read-only\n"),
            "indented flag lines must be detected"
        );
        assert!(
            is_read_only_spec("- sandbox: read-only\r\n"),
            "CRLF line endings must not hide the flag"
        );
        assert!(
            !is_read_only_spec("- goal: use a sandbox: read-only habit\n"),
            "a flag mention inside prose is not a declaration"
        );
        assert!(
            !is_read_only_spec("- sandbox: read-only-more\n"),
            "a prefixed variant is not the exact flag"
        );
    }

    #[test]
    fn bare_fail_prefix_must_not_produce_an_empty_case_name() {
        // A malformed verifier line ("FAIL:" with no name) must not
        // pollute the failure checklist with an empty entry.
        let out = "FAIL:\nFAIL: test_zero (t.Test)\nOK\n";
        let cases = extract_failing_cases(out);
        assert_eq!(
            cases,
            vec!["test_zero"],
            "empty names must be dropped, got {cases:?}"
        );
    }
}

#[cfg(test)]
mod iteration_tests {
    use super::*;

    #[test]
    fn attempt_grace_semantics() {
        // Killed + completed marker -> grace (success-with-warning).
        assert!(attempt_grace(true, true));
        // Not killed: no grace involved.
        assert!(!attempt_grace(false, true));
        // Killed without the marker: a genuine abort.
        assert!(!attempt_grace(true, false));
        assert!(!attempt_grace(false, false));
    }

    #[test]
    fn distill_failure_is_compact_and_binding() {
        let out = distill_failure(
            2,
            "FAIL: test_inline_comment\nAssertionError: 'k', 'v  # comment' != ('k', 'v')\n",
        );
        assert!(out.contains("FAILED on attempt 2"), "{out}");
        assert!(out.contains("test_inline_comment"), "{out}");
        assert!(out.contains("do not repeat them"), "{out}");
        assert!(out.len() < 400, "excerpt is bounded: {}", out.len());
    }

    #[test]
    fn resolve_worker_name_defaults_to_codex() {
        assert_eq!(resolve_worker_name(None), "codex");
        assert_eq!(resolve_worker_name(Some("claude")), "claude");
    }
}

#[cfg(test)]
mod blind_worker_tests {
    use super::*;

    #[test]
    fn hide_and_restore_moves_the_suite() {
        let root = std::env::temp_dir().join(format!("mag-bw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let hidden = root.join("hidden-suite");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("test_hidden.py"), "pass").unwrap();
        // Hide: the suite is gone from its place.
        assert!(hide_verifier(&hidden).unwrap());
        assert!(!hidden.exists());
        assert!(hidden.with_extension("blind-hidden").exists());
        // Restore: back in place, suite intact.
        assert!(restore_verifier(&hidden).unwrap());
        assert!(hidden.join("test_hidden.py").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hide_of_missing_dir_is_a_noop() {
        let root = std::env::temp_dir().join(format!("mag-bw2-{}", std::process::id()));
        assert!(!hide_verifier(&root.join("nope")).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod checklist_tests {
    use super::*;

    #[test]
    fn extracts_failing_case_names() {
        let out = "FAIL: test_inline_comment (tests.TestCli.test_inline_comment)\nAssertionError: ...\nERROR: test_zero (tests.TestCli.test_zero)\nOK\n";
        let cases = extract_failing_cases(out);
        assert_eq!(cases, vec!["test_inline_comment", "test_zero"]);
        assert!(extract_failing_cases("OK\nRan 5 tests").is_empty());
    }

    #[test]
    fn checklist_lists_only_the_failing_cases() {
        let out = distill_failure(
            2,
            "FAIL: test_inline_comment (tests.TestCli.test_inline_comment)\nFAIL: test_quoted_value (tests.TestCli.test_quoted_value)\n",
        );
        assert!(out.contains("attempt 2"), "{out}");
        assert!(out.contains("1. test_inline_comment"), "{out}");
        assert!(out.contains("2. test_quoted_value"), "{out}");
        assert!(out.contains("do not repeat them"), "{out}");
    }
}

#[cfg(test)]
mod escalation_tests {
    use super::*;

    #[test]
    fn later_attempts_include_expected_got_details() {
        let out = "FAIL: test_inline_comment (tests.TestCli.test_inline_comment)\nTraceback (most recent call last):\nAssertionError: ('k', 'v  # comment') != ('k', 'v')\nFAIL: test_quoted (t.TestCli.test_quoted)\nTraceback (most recent call last):\nAssertionError: 'x' != '\"hi\"'\n";
        let d1 = distill_failure(1, out);
        assert!(d1.contains("1. test_inline_comment"), "{d1}");
        assert!(!d1.contains("!= ("), "attempt 1 has no details: {d1}");
        let d2 = distill_failure(2, out);
        assert!(d2.contains("test_inline_comment —"), "{d2}");
        assert!(d2.contains("!= ("), "attempt 2 carries the detail: {d2}");
        assert!(d2.contains("test_quoted —"), "{d2}");
    }
}

#[cfg(test)]
mod session_resume_tests {
    use super::*;

    fn tmp_home(tag: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("mag-sess-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    fn write_rollout(home: &Path, day: &str, ts: &str, uuid: &str) -> PathBuf {
        let dir = home.join(".codex/sessions/2026/08").join(day);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("rollout-{ts}-{uuid}.jsonl"));
        std::fs::write(&p, "{}").unwrap();
        p
    }

    #[test]
    fn no_sessions_yields_none() {
        let home = tmp_home("none");
        assert_eq!(find_session_with_marker(&home, "SESS-OWN-1-1"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn marker_ownership_beats_newest_file() {
        // A NEWER session WITHOUT our marker (e.g. an IDE session) must
        // NOT be attributed to the worker — the marker match decides
        // (codex review F1).
        let home = tmp_home("own");
        write_rollout(
            &home,
            "05",
            "2026-08-05T09-00-00-019fd196-d25d-7390-bbaa-bca2d026e17c",
            "019fd196-d25d-7390-bbaa-bca2d026e17c",
        );
        let newer = write_rollout(
            &home,
            "06",
            "2026-08-06T09-00-00-019fd199-aaaa-bbbb-cccc-ddddeeee0001",
            "019fd199-aaaa-bbbb-cccc-ddddeeee0001",
        );
        // The marker sits in the OLDER session (the worker's); the newer
        // file (someone else's session) must lose.
        let dir = home.join(".codex/sessions/2026/08/05");
        let worker_file =
            dir.join("rollout-2026-08-05T09-00-00-019fd196-d25d-7390-bbaa-bca2d026e17c.jsonl");
        std::fs::write(&worker_file, "user: SESS-OWN-1-1 marker recorded").unwrap();
        std::fs::write(&newer, "unrelated content").unwrap();
        let id = find_session_with_marker(&home, "SESS-OWN-1-1").unwrap();
        assert_eq!(id, "019fd196-d25d-7390-bbaa-bca2d026e17c");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn no_marker_match_yields_none() {
        let home = tmp_home("nomatch");
        write_rollout(
            &home,
            "05",
            "2026-08-05T09-00-00-019fd196-d25d-7390-bbaa-bca2d026e17c",
            "019fd196-d25d-7390-bbaa-bca2d026e17c",
        );
        assert_eq!(
            find_session_with_marker(&home, "SESS-OWN-MISSING"),
            None,
            "a session without the marker is not the worker's"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn malformed_names_are_ignored() {
        let home = tmp_home("malformed");
        let dir = home.join(".codex/sessions/2026/08/05");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rollout-too-short.jsonl"), "SESS-OWN-1-1").unwrap();
        assert_eq!(find_session_with_marker(&home, "SESS-OWN-1-1"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn resume_decision_falls_back_to_cold_invoke() {
        assert!(!should_resume(false, 2, Some("s")), "disabled");
        assert!(!should_resume(true, 1, Some("s")), "first attempt");
        assert!(!should_resume(true, 2, None), "no session captured");
        assert!(should_resume(true, 2, Some("s")), "resume");
    }

    #[test]
    fn case_details_extracts_fail_and_error_names() {
        let out = "FAIL: test_a\nAssertionError: want 1 got 2\nFAIL: test_b\nTraceback\n  x\n";
        let d = extract_case_details(out);
        assert_eq!(
            d[0],
            ("test_a".to_string(), "want 1 got 2".to_string()),
            "{d:?}"
        );
        assert_eq!(d[1].0, "test_b", "{d:?}");
        // ERROR: prefix and the non-traceback fallback detail.
        let out2 = "ERROR: test_c\nboom detail\n";
        let d2 = extract_case_details(out2);
        assert_eq!(d2.len(), 1, "{d2:?}");
        assert_eq!(d2[0], ("test_c".to_string(), "boom detail".to_string()));
    }

    #[test]
    fn distill_failure_escalates_details_on_later_attempts() {
        let out = "FAIL: test_a\nAssertionError: want 1 got 2\n";
        let d1 = distill_failure(1, out);
        assert!(d1.contains("test_a"), "{d1}");
        assert!(!d1.contains("want 1 got 2"), "attempt 1: names only: {d1}");
        let d2 = distill_failure(2, out);
        assert!(d2.contains("want 1 got 2"), "attempt 2: detail added: {d2}");
        // No failing case: the excerpt path.
        let empty = distill_failure(1, "no case here");
        assert!(empty.contains("the verifier FAILED"), "{empty}");
    }
}
