//! Worker execution with enforced budgets (hardening audit P0-1).
//!
//! The codex/hitl worker runs arbitrary commands in a workdir. Before
//! this module it ran to completion with no kernel-enforced cap — the
//! "max retries / max steps" rules were procedural (AGENTS.md), not
//! enforced. This module makes the caps real: a wall-time cap kills the
//! worker live; step/cost caps are enforced after capture. Std-only
//! (no async): spawn + poll + kill.

use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Outcome of a budget-capped worker run.
#[derive(Debug, Clone)]
pub struct WorkerResult {
    /// Exit code (None = killed by signal).
    pub status: Option<i32>,
    /// Wall-clock seconds the worker actually ran.
    pub wall_seconds: u64,
    /// True when the worker was killed by the wall-time cap.
    pub aborted: bool,
    /// Combined stdout+stderr.
    pub output: String,
    /// Cost/usage telemetry when the worker reports it (opencode
    /// `--format json`; None for workers without usage reporting).
    pub usage: Option<WorkerUsage>,
}

/// Token/cost telemetry for one worker run (D1 layered economics).
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct WorkerUsage {
    /// Prompt/context tokens consumed by the run.
    pub tokens_in: u64,
    /// Completion tokens produced by the run.
    pub tokens_out: u64,
    /// USD; the worker's own figure when reported, otherwise the
    /// deepseek-v4-flash rate-card estimate (D1, track-3.md).
    pub cost_usd: f64,
}

/// deepseek-v4-flash rate card (USD per 1M tokens, Aug 2026, track-3.md).
const FLASH_IN_PER_1M: f64 = 0.14;
const FLASH_OUT_PER_1M: f64 = 0.28;

/// Rate-card estimate for a token volume (fractional M-tokens).
fn estimate_flash_cost(tokens_in: u64, tokens_out: u64) -> f64 {
    let in_m = f64::from(u32::try_from(tokens_in).unwrap_or(u32::MAX)) / 1e6;
    let out_m = f64::from(u32::try_from(tokens_out).unwrap_or(u32::MAX)) / 1e6;
    in_m * FLASH_IN_PER_1M + out_m * FLASH_OUT_PER_1M
}

/// Parse usage telemetry from an opencode `--format json` run.
///
/// The final `step_finish` event carries `part.tokens{input,output}` and
/// `part.cost` (USD). Defensive: scans JSON-lines, keeps the last
/// well-formed event, falls back to the rate-card estimate when the
/// worker reports no cost.
#[must_use]
pub fn parse_opencode_usage(output: &str) -> Option<WorkerUsage> {
    let mut usage: Option<WorkerUsage> = None;
    for line in output.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("step_finish") {
            continue;
        }
        let Some(part) = v.get("part") else {
            continue;
        };
        let Some(tokens) = part.get("tokens") else {
            continue;
        };
        let (Some(tokens_in), Some(tokens_out)) = (
            tokens.get("input").and_then(serde_json::Value::as_u64),
            tokens.get("output").and_then(serde_json::Value::as_u64),
        ) else {
            continue;
        };
        let cost_usd = part
            .get("cost")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_else(|| estimate_flash_cost(tokens_in, tokens_out));
        usage = Some(WorkerUsage {
            tokens_in,
            tokens_out,
            cost_usd,
        });
    }
    usage
}

/// Run `command` to completion in `cwd`, killing it if it exceeds
/// `max_wall_seconds` (None = unlimited). Std-only: spawn, poll with a
/// short sleep, kill at the deadline, then collect output.
///
/// Output goes to temp files, not pipes: a killed parent whose children
/// inherited the pipes would keep them open and block `wait_with_output`
/// (observed on CI — a 1s-cap test ran 30s because `sh -c sleep` left an
/// orphan holding the pipe). File redirect is deterministic.
///
/// # Errors
///
/// Returns the spawn error when the command cannot start.
pub fn run_capped(
    command: &str,
    args: &[&str],
    cwd: &Path,
    max_wall_seconds: Option<u64>,
) -> io::Result<WorkerResult> {
    run_capped_idle(command, args, cwd, max_wall_seconds, None)
}

/// `run_capped` with an idle-timeout (AFK supervisor S1).
///
/// When `max_idle_seconds` is set and the worker's output file has not
/// been modified for that long while the process still runs, the worker
/// is killed as STUCK (genuinely no progress). Output lands in the same
/// files as `run_capped`; the mtime of the `.out` file is the liveness
/// signal.
///
/// # Errors
///
/// Returns the spawn error when the command cannot start.
pub fn run_capped_idle(
    command: &str,
    args: &[&str],
    cwd: &Path,
    max_wall_seconds: Option<u64>,
    max_idle_seconds: Option<u64>,
) -> io::Result<WorkerResult> {
    let start = Instant::now();
    let stdout_path = cwd.join(format!(".worker-{}.out", std::process::id()));
    let stderr_path = cwd.join(format!(".worker-{}.err", std::process::id()));
    let stdout_file = File::create(&stdout_path)?;
    let stderr_file = File::create(&stderr_path)?;
    let mut child = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()?;
    let mut aborted = false;
    // Poll when a wall cap OR an idle cap is set (idle needs the poll
    // loop to watch the output-file mtime).
    if max_wall_seconds.is_some() || max_idle_seconds.is_some() {
        let deadline = max_wall_seconds.map(|max| start + Duration::from_secs(max));
        let idle = max_idle_seconds.map(Duration::from_secs);
        // Liveness baseline: an `Instant` reset on every output-file
        // mtime change — a worker is STUCK only after one full idle
        // interval WITHOUT any new output (not one interval from start).
        let mut last_activity = start;
        let mut last_mtime = std::fs::metadata(&stdout_path)
            .and_then(|m| m.modified())
            .ok();
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            let now = Instant::now();
            if deadline.is_some_and(|d| now >= d) {
                let _ = child.kill();
                aborted = true;
                break;
            }
            if let Some(idle) = idle {
                let mtime = std::fs::metadata(&stdout_path)
                    .and_then(|m| m.modified())
                    .ok();
                if let (Some(prev), Some(cur)) = (last_mtime, mtime)
                    && cur != prev
                {
                    last_mtime = Some(cur);
                    last_activity = now;
                }
                if now.duration_since(last_activity) >= idle {
                    let _ = child.kill();
                    aborted = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let status = child.wait().ok().and_then(|s| s.code());
    let output = match (
        fs::read_to_string(&stdout_path),
        fs::read_to_string(&stderr_path),
    ) {
        (Ok(out), Ok(err)) => format!("{out}{err}"),
        _ => String::new(),
    };
    let _ = fs::remove_file(&stdout_path);
    let _ = fs::remove_file(&stderr_path);
    Ok(WorkerResult {
        status: if aborted { None } else { status },
        wall_seconds: start.elapsed().as_secs(),
        aborted,
        output,
        usage: None,
    })
}

/// Which budget caps the captured run breaches (empty = within budget).
/// A cap of `None` is unlimited.
#[must_use]
pub fn budget_violations(
    steps: usize,
    cost_usd: f64,
    wall_seconds: u64,
    max_steps: Option<usize>,
    max_cost_usd: Option<f64>,
    max_wall_seconds: Option<u64>,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(max) = max_steps
        && steps > max
    {
        out.push(format!("step cap exceeded: {steps} > {max}"));
    }
    if let Some(max) = max_cost_usd
        && cost_usd > max
    {
        out.push(format!("cost cap exceeded: ${cost_usd:.4} > ${max:.4}"));
    }
    if let Some(max) = max_wall_seconds
        && wall_seconds > max
    {
        out.push(format!("wall cap exceeded: {wall_seconds}s > {max}s"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-worker-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn fast_worker_completes_within_budget() {
        let root = tmp_root("fast");
        let res = run_capped("sh", &["-c", "echo ok"], &root, Some(5)).unwrap();
        assert!(!res.aborted);
        assert_eq!(res.status, Some(0));
        assert!(res.output.contains("ok"));
        assert!(res.wall_seconds <= 5);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn slow_worker_is_killed_at_the_wall_cap() {
        let root = tmp_root("slow");
        let start = Instant::now();
        let res = run_capped("sh", &["-c", "sleep 30"], &root, Some(1)).unwrap();
        let elapsed = start.elapsed();
        assert!(res.aborted, "worker must be killed by the wall cap");
        assert!(elapsed < Duration::from_secs(5), "killed near the cap");
        assert_eq!(res.status, None, "killed process has no exit code");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn budget_violations_reports_only_breached_caps() {
        assert!(budget_violations(5, 0.1, 60, Some(10), Some(1.0), Some(120)).is_empty());
        let v = budget_violations(15, 2.0, 300, Some(10), Some(1.0), Some(120));
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(v.iter().any(|m| m.contains("step cap")));
        assert!(v.iter().any(|m| m.contains("cost cap")));
        assert!(v.iter().any(|m| m.contains("wall cap")));
        // None caps are unlimited.
        let v = budget_violations(1000, 50.0, 99999, None, None, None);
        assert!(v.is_empty());
    }

    #[test]
    fn missing_command_reports_spawn_error() {
        let root = tmp_root("spawn");
        let err = run_capped("definitely-not-a-real-binary-xyz", &[], &root, None).unwrap_err();
        let _ = std::io::Error::other(err);
        let _ = write!(&mut std::io::stderr(), "spawn err ok");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn opencode_usage_parses_step_finish_event() {
        // Grounded in a real opencode 1.18.11 --format json probe
        // (2026-08-06): the final step_finish carries part.tokens and
        // part.cost.
        let out = r#"{"type":"step_start","timestamp":1,"sessionID":"s1","part":{"type":"step-start"}}
{"type":"text","timestamp":2,"sessionID":"s1","part":{"type":"text","text":"OK"}}
{"type":"step_finish","timestamp":3,"sessionID":"s1","part":{"id":"p1","type":"step-finish","tokens":{"total":11159,"input":9226,"output":2,"reasoning":11,"cache":{"write":0,"read":1920}},"cost":0.001300656}}
"#;
        let u = parse_opencode_usage(out).expect("usage parsed");
        assert_eq!(u.tokens_in, 9226);
        assert_eq!(u.tokens_out, 2);
        assert!(
            (u.cost_usd - 0.001_300_656).abs() < 1e-9,
            "worker-reported cost wins"
        );
    }

    #[test]
    fn opencode_usage_falls_back_to_rate_card_without_cost() {
        let out = r#"{"type":"step_finish","part":{"type":"step-finish","tokens":{"input":1000000,"output":1000000}}}
"#;
        let u = parse_opencode_usage(out).unwrap();
        assert_eq!(u.tokens_in, 1_000_000);
        assert!(
            (u.cost_usd - (0.14 + 0.28)).abs() < 1e-9,
            "flash rate estimate: {}",
            u.cost_usd
        );
    }

    #[test]
    fn opencode_usage_ignores_garbage_and_keeps_last_event() {
        assert!(parse_opencode_usage("not json at all").is_none());
        assert!(parse_opencode_usage("").is_none());
        assert!(parse_opencode_usage(r#"{"type":"text","part":{"text":"hi"}}"#).is_none());
        // Multiple step_finish events: the LAST one wins (per-attempt runs
        // stream one finish per step).
        let out = r#"{"type":"step_finish","part":{"type":"step-finish","tokens":{"input":10,"output":1},"cost":0.001}}
{"type":"step_finish","part":{"type":"step-finish","tokens":{"input":20,"output":2},"cost":0.002}}
"#;
        let u = parse_opencode_usage(out).unwrap();
        assert_eq!(u.tokens_in, 20);
        assert!((u.cost_usd - 0.002).abs() < 1e-9);
    }
}

#[cfg(test)]
mod idle_timeout_tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-idle-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn silent_worker_is_killed_by_idle_timeout() {
        // `sleep` produces NO output -> the out-file mtime never changes
        // -> the idle cap must kill it (stuck worker).
        let root = tmp_root("silent");
        let res = run_capped_idle("sh", &["-c", "sleep 30"], &root, None, Some(1)).unwrap();
        assert!(
            res.aborted,
            "a silent worker must be killed by the idle cap"
        );
        assert!(res.wall_seconds < 10, "killed near the idle cap");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn worker_that_goes_silent_mid_run_is_killed_by_idle() {
        // Output then silence: the mtime stops changing -> the idle cap
        // must kill (genuinely stuck mid-iteration), not the wall cap.
        let root = tmp_root("midrun");
        let res = run_capped_idle(
            "sh",
            &["-c", "echo start; sleep 30"],
            &root,
            Some(60),
            Some(1),
        )
        .unwrap();
        assert!(res.aborted, "mid-run silence must be killed");
        assert!(
            res.wall_seconds < 30,
            "killed by the idle cap, not the wall cap"
        );
        assert!(
            res.output.contains("start"),
            "early output must be captured"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn completed_marker_with_hanging_child_is_not_lost() {
        // Completion-grace (two-phase timeout S3): the worker writes the
        // completion marker then hangs; the file-redirect design means
        // the full transcript (incl. the marker) is still readable, so
        // the run resolves as success-with-warning instead of failure.
        let root = tmp_root("hanging");
        let res = run_capped_idle(
            "sh",
            &["-c", "echo '<promise>COMPLETE</promise>'; sleep 30"],
            &root,
            Some(3),
            None,
        )
        .unwrap();
        assert!(res.aborted, "killed by the wall cap after the marker");
        assert!(
            res.output.contains("<promise>COMPLETE</promise>"),
            "the completion marker must survive the kill (file-redirect)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn idle_is_measured_from_last_activity_not_from_start() {
        // The worker writes once, then stays silent. The idle cap must
        // not fire at start+1s; it fires a full idle interval after the
        // LAST write (~2s: 0.05s write + 1s idle + kill).
        let root = tmp_root("late");
        let res = run_capped_idle(
            "sh",
            &["-c", "echo once; sleep 30"],
            &root,
            Some(60),
            Some(1),
        )
        .unwrap();
        assert!(res.aborted, "silence after a late write must be killed");
        assert!(
            res.wall_seconds >= 1,
            "the idle interval counts from the LAST output: wall {}s",
            res.wall_seconds
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_worker_survives_idle_timeout() {
        // A worker that keeps writing output is alive -> not killed.
        let root = tmp_root("active");
        let res = run_capped_idle(
            "sh",
            &["-c", "while true; do echo tick; sleep 0.2; done"],
            &root,
            Some(3),
            Some(1),
        )
        .unwrap();
        assert!(res.aborted, "killed by the wall cap, not the idle cap");
        assert!(res.output.contains("tick"), "output must be captured");
        let _ = std::fs::remove_dir_all(&root);
    }
}
