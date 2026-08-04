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
}

/// Run codex on a slice spec, capture the transcript, emit a truthful
/// run.json draft under the wall/step caps and (Linux) the Landlock
/// sandbox.
pub fn cmd_codex(args: &CodexRunArgs<'_>) -> ExitCode {
    use mini_agi_core::capture;
    let spec = args.spec;
    let workdir = args.workdir;
    let run_out = args.run_out;
    let verify = args.verify;
    let target = args.target;
    let max_wall = args.max_wall;
    let max_steps = args.max_steps;
    let no_sandbox = args.no_sandbox;
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
    let prompt = format!(
        "{spec_text}\n\nIMPLEMENTATION PROTOCOL (binding): plan first, tests first, never repeat a failing action. When the work is done and your own gate passes, END YOUR FINAL MESSAGE with:\n<promise>COMPLETE</promise>\n<result>{{\"summary\": \"one sentence\"}}</result>\n"
    );
    let log_path = workdir.join("codex.log");
    // P0-1 (hardening audit): the worker runs under a wall-time cap —
    // killed live when it exceeds it (CLI --max-wall, else the workdir's
    // .miniagi.json `max_wall_seconds`). Std-only spawn + poll + kill.
    let cfg = mini_agi_core::config::Config::load(workdir);
    let wall_cap = max_wall.or(cfg.max_wall_seconds);
    let step_cap = max_steps.or(cfg.max_steps);
    let worker_args = vec![
        "exec",
        "-s",
        "workspace-write",
        "--skip-git-repo-check",
        &prompt,
    ];
    // P0-4 (ADR-0012): the worker runs under Landlock write-containment
    // via a self-spawned wrapper on Linux, unless --no-sandbox.
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
    let worker = match run_worker_sandboxed(
        worker_name,
        workdir,
        no_sandbox,
        read_only,
        wall_cap,
        &worker_args,
    ) {
        Ok(w) => w,
        Err(e) => return fail(&format!("{worker_name} not available: {e}")),
    };
    let combined = worker.output;
    let worker_status = worker.status;
    std::fs::write(&log_path, &combined).unwrap_or(());
    // The prompt (which embeds the completion protocol) is echoed at the
    // start of the transcript — strip it so the marker detection cannot
    // self-forge (codex review).
    let stripped = combined.replace(&prompt, "");
    let outcome = capture::CaptureOutcome {
        log_path: log_path.clone(),
        steps: capture::parse_transcript(&combined),
        completed: capture::completed(&stripped),
        result: capture::extract_result(&combined),
    };
    println!(
        "codex: exit {}, completed={}, {} captured steps, log: {}",
        worker_status.map_or_else(|| "-".into(), |c| c.to_string()),
        outcome.completed,
        outcome.steps.len(),
        log_path.display()
    );
    if let Some(result) = &outcome.result {
        println!("  result: {result}");
    }
    for step in &outcome.steps {
        println!(
            "  [{}] {}",
            step.tool,
            step.action.chars().take(100).collect::<String>()
        );
    }
    let scope_list: Vec<String> = scope
        .split(',')
        .map(|s| s.trim().trim_matches('`').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // P0-1 post-hoc cap check: wall + step caps are enforced after
    // capture (cost is self-reported and enforced at `run ingest`).
    // A breach aborts the run: the draft is still written (inspectable)
    // with outcome.achieved=false and the exit code is 3 (aborted), so
    // a budget breach can never be mistaken for a clean run.
    let violations = mini_agi_core::worker::budget_violations(
        outcome.steps.len(),
        0.0,
        worker.wall_seconds,
        step_cap,
        None,
        wall_cap,
    );
    let aborted = worker.aborted || !violations.is_empty();
    for v in &violations {
        eprintln!("  [abort] {v}");
    }
    if worker.aborted {
        eprintln!("  [abort] worker killed by the wall-time cap ({wall_cap:?}s)");
    }
    let run = crate::clifmt::build_run_draft(
        &goal,
        &scope_list,
        &outcome.steps,
        Some(&verify),
        Some(&target),
        Some(worker.wall_seconds),
    );
    let exit = write_draft(run_out, workdir, &run);
    if aborted {
        println!("  run ABORTED by a budget cap (exit 3) — not a clean run");
        ExitCode::from(3)
    } else {
        exit
    }
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

/// Production-readiness D.2: does the spec declare a read-only sandbox?
fn is_read_only_spec(spec_text: &str) -> bool {
    spec_text
        .lines()
        .any(|l| l.trim_start().starts_with("- sandbox: read-only"))
}

fn run_worker_sandboxed(
    worker_name: &str,
    workdir: &Path,
    no_sandbox: bool,
    read_only: bool,
    wall_cap: Option<u64>,
    worker_args: &[&str],
) -> std::io::Result<mini_agi_core::worker::WorkerResult> {
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
                let codex_state = std::path::Path::new(&home).join(".codex");
                if codex_state.is_dir() {
                    wrapper.push("--allow-write".to_string());
                    wrapper.push(codex_state.to_string_lossy().into_owned());
                }
            }
            wrapper.push("--".to_string());
            wrapper.extend(worker_args.iter().map(|s| (*s).to_string()));
            let arg_refs: Vec<&str> = wrapper.iter().map(String::as_str).collect();
            if let Ok(exe) = std::env::current_exe() {
                return mini_agi_core::worker::run_capped(
                    &exe.to_string_lossy(),
                    &arg_refs,
                    workdir,
                    wall_cap,
                );
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (no_sandbox, read_only, wall_cap);
    }
    // Multi-worker (production-readiness P2/E): the runner resolves the
    // worker command from the parameter — codex today, a second type
    // (e.g. claude) behind the same budget/sandbox/capture contract.
    mini_agi_core::worker::run_capped(worker_name, worker_args, workdir, wall_cap)
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
}
