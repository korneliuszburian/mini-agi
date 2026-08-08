//! Run-state index (D6): a rebuildable view of every run, the journal
//! tail, and live detached workers — the machine-readable surface for
//! supervisors and the future files-first UI (D4).

use std::path::Path;
use std::time::SystemTime;

/// One indexed run.json row.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunRow {
    /// Case name (directory under `evals/cases/`).
    pub case: String,
    /// The run's goal, truncated for the index.
    pub goal: String,
    /// Worker name from the run (D1 telemetry).
    pub worker: Option<String>,
    /// USD cost from the run (D1 telemetry; 0.0 when unreported).
    pub cost_usd: f64,
    /// Total tokens (D1 telemetry).
    pub tokens_total: u64,
    /// Wall latency seconds when recorded.
    pub latency_seconds: Option<u64>,
    /// `outcome.achieved` truth.
    pub achieved: bool,
    /// Multi-dimensional quality composite (Beads/Stamps-ledger
    /// adoption): re-scored from the run when the run is scorable,
    /// `None` when it is not (e.g. malformed or never scored).
    pub composite: Option<f64>,
    /// Transcript step count.
    pub n_steps: usize,
    /// run.json mtime (newest first ordering).
    pub modified: SystemTime,
}

/// The aggregated index.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunIndex {
    pub rows: Vec<RunRow>,
    pub total_runs: usize,
    pub achieved_runs: usize,
    pub total_cost_usd: f64,
    pub total_tokens: u64,
}

/// One discovered detached-worker handle (a `.supervisor` dir).
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkerStatus {
    pub handle: String,
    pub workdir: Option<String>,
    pub report_ready: bool,
    pub alive: bool,
}

/// Index every `evals/cases/<case>/run.json` (including `-rerun` dirs),
/// newest first.
#[must_use]
pub fn index_runs(cases_dir: &Path, root: &Path) -> RunIndex {
    let mut rows: Vec<RunRow> = Vec::new();
    let Ok(entries) = std::fs::read_dir(cases_dir) else {
        return RunIndex {
            rows,
            total_runs: 0,
            achieved_runs: 0,
            total_cost_usd: 0.0,
            total_tokens: 0,
        };
    };
    for e in entries.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let run_path = dir.join("run.json");
        let Ok(text) = std::fs::read_to_string(&run_path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let case = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let goal = v
            .get("goal")
            .and_then(|g| g.as_str())
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        let worker = v.get("worker").and_then(|w| w.as_str()).map(str::to_owned);
        let cost_usd = v
            .get("cost_usd")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let tokens_total = v
            .get("tokens_total")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let latency_seconds = v.get("latency_seconds").and_then(serde_json::Value::as_u64);
        let achieved = v
            .get("outcome")
            .and_then(|o| o.get("achieved"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let n_steps = usize::try_from(
            v.get("n_steps")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
        )
        .unwrap_or(usize::MAX);
        // Re-score for the composite quality axis; a run that cannot be
        // scored (malformed, missing golden) is `None`, never a crash.
        let composite = crate::eval::score_run(&run_path, root, &root.join("evals/golden"))
            .ok()
            .map(|s| s.composite);
        let modified = std::fs::metadata(&run_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        rows.push(RunRow {
            case,
            goal,
            worker,
            cost_usd,
            tokens_total,
            latency_seconds,
            achieved,
            composite,
            n_steps,
            modified,
        });
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.modified));
    let total_runs = rows.len();
    let achieved_runs = rows.iter().filter(|r| r.achieved).count();
    let total_cost_usd: f64 = rows.iter().map(|r| r.cost_usd).sum();
    let total_tokens: u64 = rows.iter().map(|r| r.tokens_total).sum();
    RunIndex {
        rows,
        total_runs,
        achieved_runs,
        total_cost_usd,
        total_tokens,
    }
}

/// Number of respawn events recorded in `.batch/respawns.log` (D6
/// durable audit trail); `0` when no respawns were ever recorded.
#[must_use]
pub fn respawn_summary(root: &Path) -> usize {
    std::fs::read_to_string(root.join(".batch/respawns.log"))
        .map_or(0, |t| t.lines().filter(|l| !l.trim().is_empty()).count())
}

/// Last `n` lines of the checkpoint journal.
#[must_use]
pub fn journal_tail(root: &Path, n: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("memory/episodic/checkpoints.log")) else {
        return Vec::new();
    };
    text.lines()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(n)
        .rev()
        .map(str::to_owned)
        .collect()
}

/// Discover detached-worker handles: the repo root and every git
/// worktree may carry a `.supervisor` dir (batch tickets, detached
/// loop runs). Each handle's liveness comes from the bg machinery.
#[must_use]
pub fn live_workers(root: &Path) -> Vec<WorkerStatus> {
    let mut dirs: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
    if let Ok(out) = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(root)
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                dirs.push(std::path::PathBuf::from(path.trim()));
            }
        }
    }
    let mut workers = Vec::new();
    for dir in dirs {
        let handle = dir.join(".supervisor");
        if !handle.join("launch.json").is_file() {
            continue;
        }
        let st = crate::bg::run_status(&handle);
        workers.push(WorkerStatus {
            handle: handle.to_string_lossy().into_owned(),
            workdir: st.workdir,
            report_ready: st.report_ready,
            alive: st.alive,
        });
    }
    workers.sort_by_key(|w| std::cmp::Reverse(w.alive));
    workers
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("mag-status-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn index_aggregates_runs_and_totals() {
        let root = tmp_root("index");
        let cases = root.join("cases");
        // Distinct worker per run and EXPLICIT distinct mtimes: files
        // written back-to-back on a fast fs can share a nanosecond mtime,
        // and stable sort keeps read_dir order then -> order-dependent
        // asserts flake. set_modified pins the order deterministically.
        let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for (i, (name, cost, tokens, achieved, worker)) in [
            ("a-ok", 0.02, 100u64, true, "w-old"),
            ("a-ok-rerun", 0.03, 200u64, true, "w-mid"),
            ("b-fail", 0.01, 50u64, false, "w-new"),
        ]
        .into_iter()
        .enumerate()
        {
            let dir = cases.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let run = serde_json::json!({
                "goal": format!("goal {name}"),
                "worker": worker,
                "scope": ["x"],
                "cost_usd": cost,
                "tokens_total": tokens,
                "outcome": {"achieved": achieved},
                "n_steps": 3,
                "trajectory": [{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}],
            });
            let path = dir.join("run.json");
            std::fs::write(&path, serde_json::to_string_pretty(&run).unwrap()).unwrap();
            std::fs::File::open(&path)
                .unwrap()
                .set_modified(base + std::time::Duration::from_secs(i as u64))
                .unwrap();
        }
        let idx = index_runs(&cases, &root);
        assert_eq!(idx.total_runs, 3);
        assert_eq!(idx.achieved_runs, 2);
        assert!((idx.total_cost_usd - 0.06).abs() < 1e-9);
        assert_eq!(idx.total_tokens, 350);
        // Newest first by mtime.
        let times: Vec<SystemTime> = idx.rows.iter().map(|r| r.modified).collect();
        let mut sorted = times.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(times, sorted);
        assert_eq!(
            idx.rows[0].worker.as_deref(),
            Some("w-new"),
            "newest mtime must sort first"
        );
        // Composite quality axis is populated when the run is scorable.
        assert!(
            idx.rows[0].composite.is_some(),
            "a scorable run must carry a composite"
        );
        let achieved_ok = idx
            .rows
            .iter()
            .find(|r| r.achieved)
            .expect("fixture has an achieved run");
        assert!(
            achieved_ok.composite.is_some_and(|c| c > 0.0),
            "an achieved run must have a positive composite"
        );
    }

    #[test]
    fn index_ignores_missing_and_malformed() {
        let root = tmp_root("empty");
        std::fs::create_dir_all(root.join("cases")).unwrap();
        assert_eq!(index_runs(&root.join("cases"), &root).total_runs, 0);
        let dir = root.join("cases/x");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("run.json"), "not json").unwrap();
        assert_eq!(index_runs(&root.join("cases"), &root).total_runs, 0);
    }

    #[test]
    fn journal_tail_returns_last_lines() {
        let root = tmp_root("journal");
        std::fs::create_dir_all(root.join("memory/episodic")).unwrap();
        let mut f = std::fs::File::create(root.join("memory/episodic/checkpoints.log")).unwrap();
        for i in 0..5 {
            writeln!(f, "line-{i}").unwrap();
        }
        assert_eq!(
            journal_tail(&root, 2),
            vec!["line-3".to_string(), "line-4".to_string()]
        );
        assert!(journal_tail(&root.join("nope"), 2).is_empty());
    }

    #[test]
    fn respawn_summary_counts_recorded_events() {
        let root = std::env::temp_dir().join(format!(
            "mag-status-ev-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join(".batch")).unwrap();
        assert_eq!(respawn_summary(&root), 0, "no file yet -> 0");
        std::fs::write(root.join(".batch/respawns.log"), "t1: respawned 1x\n\n").unwrap();
        assert_eq!(respawn_summary(&root), 1, "blank lines ignored");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn live_workers_finds_handles_and_reports_liveness() {
        let root = tmp_root("workers");
        std::fs::create_dir_all(root.join(".supervisor")).unwrap();
        // A launch.json without a live process: discovered, not alive.
        std::fs::write(
            root.join(".supervisor/launch.json"),
            r#"{"goal_or_case":"x","workdir":"/tmp","verify":"true"}"#,
        )
        .unwrap();
        let workers = live_workers(&root);
        assert_eq!(workers.len(), 1);
        assert!(!workers[0].alive);
        assert!(workers[0].handle.ends_with(".supervisor"));
    }
}
