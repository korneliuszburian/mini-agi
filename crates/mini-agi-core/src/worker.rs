//! Worker execution with enforced budgets (hardening audit P0-1).
//!
//! The codex/hitl worker runs arbitrary commands in a workdir. Before
//! this module it ran to completion with no kernel-enforced cap — the
//! "max retries / max steps" rules were procedural (AGENTS.md), not
//! enforced. This module makes the caps real: a wall-time cap kills the
//! worker live; step/cost caps are enforced after capture. Std-only
//! (no async): spawn + poll + kill.

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
}

/// Run `command` to completion in `cwd`, killing it if it exceeds
/// `max_wall_seconds` (None = unlimited). Std-only: spawn, poll with a
/// short sleep, kill at the deadline, then collect output.
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
    let start = Instant::now();
    let mut child = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(max) = max_wall_seconds {
        let deadline = start + Duration::from_secs(max);
        loop {
            if let Some(status) = child.try_wait()? {
                let out = child.wait_with_output()?;
                return Ok(WorkerResult {
                    status: status.code(),
                    wall_seconds: start.elapsed().as_secs(),
                    aborted: false,
                    output: combine(&out),
                });
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let out = child.wait_with_output()?;
                return Ok(WorkerResult {
                    status: None,
                    wall_seconds: start.elapsed().as_secs(),
                    aborted: true,
                    output: combine(&out),
                });
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let out = child.wait_with_output()?;
    Ok(WorkerResult {
        status: out.status.code(),
        wall_seconds: start.elapsed().as_secs(),
        aborted: false,
        output: combine(&out),
    })
}

fn combine(out: &std::process::Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
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
}
