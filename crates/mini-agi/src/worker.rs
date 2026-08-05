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
    let protocol = "IMPLEMENTATION PROTOCOL (binding): plan first, tests first, never repeat a failing action. When the work is done and your own gate passes, END YOUR FINAL MESSAGE with:\n<promise>COMPLETE</promise>\n<result>{\"summary\": \"one sentence\"}</result>";
    let base_prompt = format!("{spec_text}\n\n{protocol}\n");
    let scope_list: Vec<String> = scope
        .split(',')
        .map(|s| s.trim().trim_matches('`').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let log_path = workdir.join("codex.log");
    let mut all_steps: Vec<mini_agi_core::capture::CapturedStep> = Vec::new();
    let mut attempt_verdicts: Vec<serde_json::Value> = Vec::new();
    let mut failure_context = String::new();
    let mut final_wall = 0u64;
    let mut attempts_done = 0;
    let mut aborted = false;
    let mut verifier_passed = false;
    let iterations = iterate.max(1);
    // S2: verify-audit wired into the loop — before trusting the
    // iteration, confirm the verifier is non-vacuous (it must reject an
    // empty counterfactual target). A vacuous verifier would make the
    // loop 'pass' garbage; refuse instead.
    if iterations > 1 {
        let audit =
            mini_agi_core::verifier::audit_verifier_vacuous(std::path::Path::new(&target), &verify);
        if audit.is_vacuous {
            return fail(
                "refusing verified-iteration: the verifier is VACUOUS (passes an empty \
                 target) — fix the verifier or drop --iterate (verify-audit)",
            );
        }
    }
    for attempt in 1..=iterations {
        attempts_done = attempt;
        let prompt = if attempt == 1 {
            base_prompt.clone()
        } else {
            format!(
                "{base_prompt}\n\nFAILURE FEEDBACK FROM ATTEMPT {prev} (binding — fix these):\n{failure_context}\nStart from your last state, address each failing case, and re-run the verifier until it passes.\n",
                prev = attempt - 1
            )
        };
        let worker_args = vec![
            "exec",
            "-s",
            "workspace-write",
            "--skip-git-repo-check",
            &prompt,
        ];
        // Blind-worker mode: the hidden suite is unavailable to the
        // worker — the kernel's loop is the only feedback path.
        let hidden_away = if blind_worker {
            match hidden_dir {
                Some(dir) => hide_verifier(dir).unwrap_or(false),
                None => {
                    return fail(
                        "refusing blind-worker mode without --hidden-dir (the isolation                          requires the verifier's hidden suite to be movable)",
                    );
                }
            }
        } else {
            false
        };
        let worker = match run_worker_sandboxed(
            worker_name,
            workdir,
            no_sandbox,
            read_only,
            wall_cap,
            &worker_args,
        ) {
            Ok(w) => w,
            Err(e) => {
                if hidden_away && let Some(dir) = hidden_dir {
                    let _ = restore_verifier(dir);
                }
                return fail(&format!("{worker_name} not available: {e}"));
            }
        };
        if hidden_away && let Some(dir) = hidden_dir {
            let _ = restore_verifier(dir);
        }
        let combined = worker.output;
        final_wall = worker.wall_seconds;
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
        println!(
            "codex attempt {attempt}: exit {}, completed={}, {} steps",
            worker.status.map_or_else(|| "-".into(), |c| c.to_string()),
            outcome.completed,
            outcome.steps.len()
        );
        // P0-1 post-hoc cap check (accumulated).
        let violations = mini_agi_core::worker::budget_violations(
            all_steps.len(),
            0.0,
            final_wall,
            step_cap,
            None,
            wall_cap,
        );
        aborted = worker.aborted || !violations.is_empty();
        for v in &violations {
            eprintln!("  [abort] {v}");
        }
        if worker.aborted {
            eprintln!("  [abort] worker killed by the wall-time cap ({wall_cap:?}s)");
        }
        if aborted {
            break;
        }
        // Single shot: the kernel does not drive iteration (the verifier
        // is loop verify's job). Only iterate > 1 runs the verifier here.
        if iterations == 1 {
            break;
        }
        // Verified-iteration (BREAKTHROUGH): run the deterministic
        // verifier; on failure distill the feedback and re-invoke.
        let verifier = mini_agi_core::worker::run_capped(
            "sh",
            &["-c", &verify],
            std::path::Path::new(&target),
            Some(120),
        );
        match verifier {
            Ok(v) if v.status == Some(0) && !v.aborted => {
                verifier_passed = true;
                attempt_verdicts.push(serde_json::json!({
                    "attempt": attempt,
                    "failed_cases": [],
                    "passed": true,
                }));
                println!("  verifier PASSED on attempt {attempt}");
                break;
            }
            Ok(v) => {
                attempt_verdicts.push(serde_json::json!({
                    "attempt": attempt,
                    "failed_cases": extract_failing_cases(&v.output),
                    "passed": false,
                }));
                failure_context = distill_failure(attempt, &v.output);
                println!(
                    "  verifier FAILED on attempt {attempt}; {} attempt(s) left",
                    iterations.saturating_sub(attempt)
                );
            }
            Err(e) => return fail(&format!("verifier not available: {e}")),
        }
    }
    let run = crate::clifmt::build_run_draft(
        &goal,
        &scope_list,
        &all_steps,
        Some(&verify),
        Some(&target),
        Some(final_wall),
    );
    let mut run = run;
    run["attempts"] = serde_json::json!(attempts_done);
    run["verifier_passed"] = serde_json::json!(verifier_passed);
    run["attempt_verdicts"] = serde_json::json!(attempt_verdicts);
    let exit = write_draft(run_out, workdir, &run);
    if aborted {
        println!("  run ABORTED by a budget cap (exit 3) — not a clean run");
        ExitCode::from(3)
    } else if iterations > 1 && !verifier_passed {
        println!("  run did NOT pass the verifier after {attempts_done} attempts (exit 1)");
        ExitCode::from(1)
    } else {
        exit
    }
}

/// Blind-worker isolation (EXP-012 as a capability): move the verifier's
/// hidden-suite directory aside so the worker cannot self-verify.
/// Returns true when the directory was moved.
fn hide_verifier(hidden_dir: &Path) -> std::io::Result<bool> {
    if !hidden_dir.exists() {
        return Ok(false);
    }
    let away = hidden_dir.with_extension("blind-hidden");
    if away.exists() {
        let _ = std::fs::remove_dir_all(&away);
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

/// Distill a verifier failure into a compact, binding instruction for
/// the next iteration (BREAKTHROUGH; Reflexion-style test-grounded
/// feedback). Structured as a per-case checklist when the failing case
/// names are extractable (process supervision: the next attempt sees
/// exactly the REMAINING cases).
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
    for (i, c) in cases.iter().enumerate() {
        use std::fmt::Write as _;
        let _ = writeln!(out, "  {}. {c}", i + 1);
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
            }
            wrapper.push("--".to_string());
            // The wrapper runs `<worker_name> <worker_args...>` — the
            // command itself is NOT part of worker_args (a real bug the
            // proof-of-advantage experiment caught: the wrapper tried to
            // run `exec` instead of `codex exec`).
            wrapper.push(worker_name.to_string());
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

#[cfg(test)]
mod iteration_tests {
    use super::*;

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
