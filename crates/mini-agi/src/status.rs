//! Run-state index (D6): a rebuildable view of every run, the journal
//! tail, and live detached workers — the machine-readable surface for
//! supervisors and the future files-first UI (D4).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

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
    /// run.json mtime as epoch milliseconds (freshness rendering).
    pub modified_at_ms: u64,
    /// Repository-relative run.json path for exact replay commands.
    pub run_file: String,
    /// Immutable-run-bound verification state (claim vs proof).
    pub verification: RunVerification,
}

/// One verifier attribution row that matches a run, rendered for the
/// supervision surface.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunVerificationEvidence {
    /// ISO timestamp of the verifier execution.
    pub at: String,
    /// Verifier status (`verified` | `verified-failed` | `disagrees`).
    pub status: String,
    /// The executed command.
    pub command: String,
    /// Target repo the command ran in.
    pub target: String,
    /// Run fingerprint the evidence was produced against, when the row
    /// carried one.
    pub run_sha256: Option<String>,
    /// True when the fingerprint matches the CURRENT run bytes.
    pub current_run: bool,
    /// Evidence log location.
    pub log: String,
}

/// Verification truth for one run: the run's own `achieved` is a CLAIM;
/// only fingerprint-bound verifier evidence is proof.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunVerification {
    /// `verified` | `verified_failed` | `disagrees` | `required` |
    /// `unverified`.
    pub state: String,
    /// The run declares both a verifier command and a target.
    pub declared: bool,
    /// Declared verifier command (`verify_command`).
    pub command: Option<String>,
    /// Declared target repo (`verify_target`).
    pub target: Option<String>,
    /// The declared target directory no longer exists, so the verifier
    /// cannot run from disk (the claim can never gain proof).
    pub target_missing: bool,
    /// SHA-256 of the current run.json bytes.
    pub run_sha256: String,
    /// Exact pasteable command the human can run.
    pub command_text: String,
    /// Same-origin execute route when the case is a plain path segment.
    pub execute_path: Option<String>,
    /// Fingerprint-bound evidence for the CURRENT run bytes, if any.
    pub evidence: Option<RunVerificationEvidence>,
    /// Older case-level evidence that does not bind current bytes.
    pub legacy_evidence: Option<RunVerificationEvidence>,
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
    /// Workdir basename, or `supervisor` when absent.
    pub id: String,
    /// The `.supervisor` handle path.
    pub handle: String,
    pub workdir: Option<String>,
    /// The report path from launch.json (may not exist yet).
    pub report: Option<String>,
    pub report_ready: bool,
    pub alive: bool,
    /// `working` | `finished` | `crashed`.
    pub state: String,
    /// Alive but without recent artifact activity.
    pub stale: bool,
    /// Idle threshold the `stale` flag was evaluated against.
    pub stale_after_seconds: u64,
    pub started_at_ms: Option<u64>,
    /// Newest activity across launch.json/run.out/progress.md/report.
    pub updated_at_ms: Option<u64>,
    /// progress.md tail when readable.
    pub progress_tail: Option<String>,
}

/// A path argument must be a single plain segment — ASCII alphanumerics
/// and `-`/`_`/`.` only — so `join` cannot escape the intended directory
/// and query delimiters/quote characters can never reach a URL.
#[must_use]
pub fn plain_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// `SystemTime` as epoch milliseconds (`0` before the Unix epoch).
#[must_use]
pub fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

/// File mtime as epoch milliseconds, `None` when unreadable.
#[must_use]
fn modified_ms(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(system_time_ms)
}

fn evidence_view(
    row: &mini_agi_core::verifier::VerifyAttribution,
    current_run: bool,
) -> RunVerificationEvidence {
    RunVerificationEvidence {
        at: row.at.clone(),
        status: row.status.clone(),
        command: row.command.clone(),
        target: row.target.clone(),
        run_sha256: row.run_sha256.clone(),
        current_run,
        log: "memory/episodic/verify.log".to_string(),
    }
}

/// Derive the run verification state from immutable bytes + the verifier
/// attribution log. Only a fingerprint-bound row confers `verified`.
fn run_verification(
    case: &str,
    run_file: &str,
    run_sha256: &str,
    command: Option<String>,
    target: Option<String>,
    attribution: &[mini_agi_core::verifier::VerifyAttribution],
) -> RunVerification {
    let declared = command.is_some() && target.is_some();
    let target_missing = target
        .as_deref()
        .is_some_and(|target_path| !Path::new(target_path).is_dir());
    let matching: Vec<&mini_agi_core::verifier::VerifyAttribution> = attribution
        .iter()
        .rev()
        .filter(|row| {
            row.case == case
                && command.as_deref() == Some(row.command.as_str())
                && target.as_deref() == Some(row.target.as_str())
        })
        .collect();
    let exact = matching
        .iter()
        .copied()
        .find(|row| row.run_sha256.as_deref() == Some(run_sha256));
    let legacy = matching
        .iter()
        .copied()
        .find(|row| row.run_sha256.as_deref() != Some(run_sha256));
    let state = if !declared {
        "unverified"
    } else if let Some(row) = exact {
        match row.status.as_str() {
            "verified" => "verified",
            "verified-failed" => "verified_failed",
            "disagrees" => "disagrees",
            _ => "required",
        }
    } else if target_missing {
        // The verifier can never run — a claim that cannot gain proof is
        // unverified, not a pending action. `required` (and its attention
        // item) must mean "verification is expected of you".
        "unverified"
    } else {
        "required"
    };
    RunVerification {
        state: state.to_string(),
        declared,
        command,
        target,
        target_missing,
        run_sha256: run_sha256.to_string(),
        command_text: format!("mini-agi run verify {run_file}"),
        execute_path: (declared && !target_missing && plain_path_segment(case))
            .then(|| format!("/api/act/run-verify?case={case}")),
        evidence: exact.map(|row| evidence_view(row, true)),
        legacy_evidence: legacy.map(|row| evidence_view(row, false)),
    }
}

/// Index every `evals/cases/<case>/run.json` (including `-rerun` dirs),
/// newest first.
#[must_use]
pub fn index_runs(cases_dir: &Path, root: &Path) -> RunIndex {
    let mut rows: Vec<RunRow> = Vec::new();
    let attribution = mini_agi_core::verifier::read_attribution(root).unwrap_or_default();
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
        // Immutable-run verification: the fingerprint is bound to the
        // EXACT bytes the verifier executed against; a modified run can
        // never inherit old evidence (claim stays a claim).
        let run_file_path = run_path.strip_prefix(root).unwrap_or(&run_path);
        let run_file = run_file_path.to_string_lossy().into_owned();
        let run_sha256 = mini_agi_core::hash::source_sha256_bytes(text.as_bytes());
        let verify_command = v
            .get("verify_command")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let verify_target = v
            .get("verify_target")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let verification = run_verification(
            &case,
            &run_file,
            &run_sha256,
            verify_command,
            verify_target,
            &attribution,
        );
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
            modified_at_ms: system_time_ms(modified),
            run_file,
            verification,
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
    let stale_after_seconds = mini_agi_core::config::Config::load(root)
        .max_idle_seconds
        .unwrap_or(300);
    let now_ms = system_time_ms(SystemTime::now());
    for dir in dirs {
        let handle = dir.join(".supervisor");
        if !handle.join("launch.json").is_file() {
            continue;
        }
        let st = crate::bg::run_status(&handle);
        let started_at_ms = modified_ms(&handle.join("launch.json"));
        let mut activity = vec![
            modified_ms(&handle.join("launch.json")),
            modified_ms(&handle.join("run.out")),
        ];
        if let Some(workdir) = st.workdir.as_deref() {
            activity.push(modified_ms(&Path::new(workdir).join("progress.md")));
        }
        if let Some(report) = st.report.as_deref() {
            activity.push(modified_ms(Path::new(report)));
        }
        let updated_at_ms = activity.into_iter().flatten().max();
        let state = if st.alive {
            "working"
        } else if st.report_ready {
            "finished"
        } else {
            "crashed"
        };
        let is_stale = st.alive
            && updated_at_ms.is_some_and(|updated| {
                now_ms.saturating_sub(updated) > stale_after_seconds.saturating_mul(1000)
            });
        let id = st
            .workdir
            .as_deref()
            .and_then(|workdir| Path::new(workdir).file_name())
            .map_or_else(
                || "supervisor".to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
        workers.push(WorkerStatus {
            id,
            handle: handle.to_string_lossy().into_owned(),
            workdir: st.workdir,
            report: st.report,
            report_ready: st.report_ready,
            alive: st.alive,
            state: state.to_string(),
            stale: is_stale,
            stale_after_seconds,
            started_at_ms,
            updated_at_ms,
            progress_tail: st.progress_tail,
        });
    }
    // Severity first (crashed > stale-working > working > finished),
    // freshness second: the human sees the broken before the rest.
    workers.sort_by(|left, right| {
        fn rank(state: &str, is_stale: bool) -> u8 {
            match (state, is_stale) {
                ("crashed", _) => 0,
                ("working", true) => 1,
                ("working", false) => 2,
                _ => 3,
            }
        }
        rank(&left.state, left.stale)
            .cmp(&rank(&right.state, right.stale))
            .then_with(|| right.updated_at_ms.cmp(&left.updated_at_ms))
    });
    workers
}

/// Typed checkpoint-journal supervision: BEGIN/VERIFY resolution, audit
/// anomalies and in-progress state — the browser renders classified rows,
/// never raw lines.
#[derive(Debug, Clone, serde::Serialize)]
pub struct JournalCounts {
    pub begin: usize,
    pub verify_pass: usize,
    pub verify_fail: usize,
    pub status: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JournalEventView {
    /// 1-based journal line.
    pub line_no: usize,
    /// ISO timestamp.
    pub at: String,
    /// `begin` | `verify_pass` | `verify_fail` | `status` | `end`.
    pub kind: String,
    /// The checkpoint label.
    pub label: String,
    /// `resolved_pass` | `resolved_fail` | `in_progress` | `anomaly` |
    /// `historical` | `event`.
    pub state: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JournalAnomalyView {
    pub line_no: usize,
    /// `bad` (fails the gate) | `historical` (warning).
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JournalSnapshot {
    /// `ok` | `in_progress` | `anomaly` | `absent`.
    pub state: String,
    pub counts: JournalCounts,
    /// Newest-first event window.
    pub events: Vec<JournalEventView>,
    pub anomalies: Vec<JournalAnomalyView>,
}

/// Supervision view of the checkpoint journal; no editing, no pairing
/// logic in the browser.
#[must_use]
pub fn journal_snapshot(root: &Path, limit: usize) -> JournalSnapshot {
    use mini_agi_core::journal::{JournalKind, audit_journal, parse_journal};

    let path = root.join("memory/episodic/checkpoints.log");
    let Ok(text) = std::fs::read_to_string(path) else {
        return JournalSnapshot {
            state: "absent".to_string(),
            counts: JournalCounts {
                begin: 0,
                verify_pass: 0,
                verify_fail: 0,
                status: 0,
            },
            events: Vec::new(),
            anomalies: Vec::new(),
        };
    };
    let parsed = parse_journal(&text);
    let audit = audit_journal(&parsed);
    let bad_lines: HashSet<usize> = audit.bad.iter().map(|item| item.line_no).collect();
    let historical_lines: HashSet<usize> =
        audit.historical.iter().map(|item| item.line_no).collect();
    let mut resolution: HashMap<usize, &str> = HashMap::new();
    let mut open: HashMap<String, usize> = HashMap::new();
    let mut counts = JournalCounts {
        begin: 0,
        verify_pass: 0,
        verify_fail: 0,
        status: 0,
    };
    for event in &parsed {
        match event.kind {
            JournalKind::Begin => {
                counts.begin += 1;
                open.insert(event.label.clone(), event.line_no);
            }
            JournalKind::VerifyPass => {
                counts.verify_pass += 1;
                if let Some(line) = open.remove(&event.label) {
                    resolution.insert(line, "resolved_pass");
                }
            }
            JournalKind::VerifyFail => {
                counts.verify_fail += 1;
                if let Some(line) = open.remove(&event.label) {
                    resolution.insert(line, "resolved_fail");
                }
            }
            JournalKind::Status => counts.status += 1,
            JournalKind::End => {}
        }
    }
    let last_line = parsed.last().map(|event| event.line_no);
    for line in open.values() {
        if Some(*line) == last_line {
            resolution.insert(*line, "in_progress");
        }
    }
    let events = parsed
        .iter()
        .rev()
        .take(limit)
        .rev()
        .map(|event| {
            let state = if bad_lines.contains(&event.line_no) {
                "anomaly"
            } else if historical_lines.contains(&event.line_no) {
                "historical"
            } else if let Some(state) = resolution.get(&event.line_no) {
                state
            } else {
                "event"
            };
            let kind = match event.kind {
                JournalKind::Begin => "begin",
                JournalKind::VerifyPass => "verify_pass",
                JournalKind::VerifyFail => "verify_fail",
                JournalKind::Status => "status",
                JournalKind::End => "end",
            };
            JournalEventView {
                line_no: event.line_no,
                at: event.ts.clone(),
                kind: kind.to_string(),
                label: event.label.clone(),
                state: state.to_string(),
            }
        })
        .collect();
    let mut anomalies: Vec<JournalAnomalyView> = audit
        .bad
        .into_iter()
        .map(|item| JournalAnomalyView {
            line_no: item.line_no,
            severity: "bad".to_string(),
            message: item.message,
        })
        .chain(audit.historical.into_iter().map(|item| JournalAnomalyView {
            line_no: item.line_no,
            severity: "historical".to_string(),
            message: item.message,
        }))
        .collect();
    anomalies.sort_by_key(|item| item.line_no);
    let state = if !bad_lines.is_empty() {
        "anomaly"
    } else if last_line.is_some_and(|line| resolution.get(&line) == Some(&"in_progress")) {
        "in_progress"
    } else {
        "ok"
    };
    JournalSnapshot {
        state: state.to_string(),
        counts,
        events,
        anomalies,
    }
}

/// Bounded in-process cache TTL for the read-only integrity scan.
const MEMORY_SCAN_TTL: Duration = Duration::from_mins(1);

/// Live memory-integrity signal: the same deterministic findings
/// `mem verify` reports, computed read-only with a short cache.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryHealth {
    /// `ok` | `findings` | `unknown`.
    pub state: String,
    pub checked_at_ms: u64,
    pub cache_ttl_seconds: u64,
    pub findings: Vec<String>,
    pub command: String,
    pub execute_path: String,
}

struct CachedMemoryHealth {
    root: PathBuf,
    checked: Instant,
    value: MemoryHealth,
}

static MEMORY_HEALTH_CACHE: OnceLock<Mutex<Vec<CachedMemoryHealth>>> = OnceLock::new();

fn memory_cache() -> &'static Mutex<Vec<CachedMemoryHealth>> {
    MEMORY_HEALTH_CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Read-only integrity scan with a 60-second in-process cache; never
/// spawns a shell and never writes.
#[must_use]
pub fn memory_health(root: &Path) -> MemoryHealth {
    {
        let cache = memory_cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(hit) = cache
            .iter()
            .find(|entry| entry.root == root && entry.checked.elapsed() < MEMORY_SCAN_TTL)
        {
            return hit.value.clone();
        }
    }
    let canonical = root.join("memory/canonical/entries");
    let findings = if canonical.is_dir() {
        mini_agi_core::memory::integrity_findings(root)
    } else {
        vec![format!(
            "canonical entries directory is absent: {}",
            canonical.display()
        )]
    };
    let state = if !canonical.is_dir() {
        "unknown"
    } else if findings.is_empty() {
        "ok"
    } else {
        "findings"
    };
    let value = MemoryHealth {
        state: state.to_string(),
        checked_at_ms: system_time_ms(SystemTime::now()),
        cache_ttl_seconds: MEMORY_SCAN_TTL.as_secs(),
        findings,
        command: "mini-agi mem verify".to_string(),
        execute_path: "/api/act/mem-verify".to_string(),
    };
    let mut cache = memory_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.retain(|entry| entry.root != root);
    cache.push(CachedMemoryHealth {
        root: root.to_path_buf(),
        checked: Instant::now(),
        value: value.clone(),
    });
    value
}

/// Drop the cached scan for one root so the next read is fresh (called
/// after an action that mutates canonical memory).
pub fn invalidate_memory_health(root: &Path) {
    let mut cache = memory_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.retain(|entry| entry.root != root);
}

/// Repository context: two bounded read-only Git child processes, never
/// shelled strings.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RepositoryStatus {
    pub root: String,
    pub name: String,
    /// `clean` | `dirty` | `unavailable`.
    pub state: String,
    pub branch: Option<String>,
    pub revision: Option<String>,
    pub changed_files: Option<usize>,
    pub target_composite: f64,
    pub max_rerun_attempts: Option<usize>,
    pub max_idle_seconds: Option<u64>,
    pub require_approval: bool,
}

#[must_use]
pub fn repository_status(root: &Path) -> RepositoryStatus {
    let config = mini_agi_core::config::Config::load(root);
    let status = std::process::Command::new("git")
        .args([
            "status",
            "--porcelain=v1",
            "--branch",
            "--untracked-files=normal",
        ])
        .current_dir(root)
        .output();
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty());
    let (state, branch, changed_files) = match status {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut lines = text.lines();
            let branch = lines
                .next()
                .and_then(|line| line.strip_prefix("## "))
                .map(|line| {
                    // Unborn repo: "## No commits yet on master" — the
                    // branch name follows the notice; the notice itself
                    // is not a branch.
                    let after = line.strip_prefix("No commits yet on ").unwrap_or(line);
                    after
                        .split("...")
                        .next()
                        .unwrap_or(after)
                        .split_whitespace()
                        .next()
                        .unwrap_or(after)
                        .to_string()
                });
            let changed = lines.filter(|line| !line.trim().is_empty()).count();
            (
                if changed == 0 { "clean" } else { "dirty" }.to_string(),
                branch,
                Some(changed),
            )
        }
        _ => ("unavailable".to_string(), None, None),
    };
    RepositoryStatus {
        root: root.to_string_lossy().into_owned(),
        name: root.file_name().map_or_else(
            || "repository".to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        state,
        branch,
        revision,
        changed_files,
        target_composite: config.target_composite,
        max_rerun_attempts: config.max_rerun_attempts,
        max_idle_seconds: config.max_idle_seconds,
        require_approval: config.require_approval,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn run_verification_with(target_path: &std::path::Path) -> RunVerification {
        run_verification(
            "case-a",
            "evals/cases/case-a/run.json",
            "abc",
            Some("make verify".to_string()),
            Some(target_path.to_string_lossy().into_owned()),
            &[],
        )
    }

    #[test]
    fn missing_target_is_unverified_not_required() {
        let root = tmp_root("missing-target");
        let gone = root.join("gone");
        let v = run_verification_with(&gone);
        assert_eq!(
            v.state, "unverified",
            "a claim that can never gain proof is unverified, not a pending action"
        );
        assert!(v.target_missing);
        assert!(
            v.execute_path.is_none(),
            "no run-verify route for an unstunnable verifier"
        );
    }

    #[test]
    fn live_target_stays_required_until_attribution() {
        let root = tmp_root("live-target");
        let v = run_verification_with(&root);
        assert_eq!(v.state, "required");
        assert!(!v.target_missing);
        assert_eq!(
            v.execute_path.as_deref(),
            Some("/api/act/run-verify?case=case-a")
        );
    }

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

    #[test]
    fn journal_snapshot_flags_unpaired_begin_as_anomaly() {
        let root = tmp_root("journal-anomaly");
        let log = root.join("memory/episodic/checkpoints.log");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        // Two unpaired BEGINs: the first is an orphan anomaly, the last
        // line is verification in progress (T008 semantics).
        std::fs::write(
            &log,
            "2026-08-11T00:00:00Z BEGIN a\n2026-08-11T00:00:01Z BEGIN b\n",
        )
        .unwrap();
        let snapshot = journal_snapshot(&root, 14);
        assert_eq!(snapshot.state, "anomaly");
        let bad: Vec<&JournalAnomalyView> = snapshot
            .anomalies
            .iter()
            .filter(|anomaly| anomaly.severity == "bad")
            .collect();
        assert_eq!(bad.len(), 1, "exactly one orphan BEGIN anomaly");
        assert_eq!(bad[0].line_no, 1);
        let last = snapshot.events.last().unwrap();
        assert_eq!(last.kind, "begin");
        assert_eq!(last.state, "in_progress");
        let first = snapshot.events.first().unwrap();
        assert_eq!(first.state, "anomaly");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn journal_snapshot_counts_resolutions_and_states() {
        let root = tmp_root("journal-counts");
        let log = root.join("memory/episodic/checkpoints.log");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(
            &log,
            "2026-08-11T00:00:00Z BEGIN a\n2026-08-11T00:00:01Z VERIFY-PASS a\n2026-08-11T00:00:02Z BEGIN b\n2026-08-11T00:00:03Z VERIFY-FAIL b\n2026-08-11T00:00:04Z BEGIN c\n",
        )
        .unwrap();
        let snapshot = journal_snapshot(&root, 14);
        assert_eq!(snapshot.state, "in_progress");
        assert!(snapshot.anomalies.is_empty(), "{:?}", snapshot.anomalies);
        assert_eq!(snapshot.counts.begin, 3);
        assert_eq!(snapshot.counts.verify_pass, 1);
        assert_eq!(snapshot.counts.verify_fail, 1);
        let states: Vec<&str> = snapshot
            .events
            .iter()
            .map(|event| event.state.as_str())
            .collect();
        assert_eq!(
            states,
            vec![
                "resolved_pass",
                "event",
                "resolved_fail",
                "event",
                "in_progress"
            ],
            "resolution rides the BEGIN line; VERIFY lines are plain events"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exact_attribution_conferes_verified_legacy_stays_required() {
        use mini_agi_core::verifier::VerifyAttribution;
        let root = tmp_root("exact-attribution");
        let row = |status: &str, run_sha256: Option<&str>| VerifyAttribution {
            at: "2026-08-11T00:00:00Z".to_string(),
            case: "case-a".to_string(),
            command: "make verify".to_string(),
            target: root.to_string_lossy().into_owned(),
            status: status.to_string(),
            run_sha256: run_sha256.map(str::to_owned),
        };
        let target = Some(root.to_string_lossy().into_owned());
        let v = run_verification(
            "case-a",
            "evals/cases/case-a/run.json",
            "abc",
            Some("make verify".to_string()),
            target.clone(),
            &[row("verified", Some("abc"))],
        );
        assert_eq!(
            v.state, "verified",
            "fingerprint-bound evidence confers verified"
        );
        assert!(v.evidence.is_some(), "exact evidence must be attached");
        assert!(v.legacy_evidence.is_none());
        let v = run_verification(
            "case-a",
            "evals/cases/case-a/run.json",
            "abc",
            Some("make verify".to_string()),
            target.clone(),
            &[row("verified", Some("deadbeef"))],
        );
        assert_eq!(
            v.state, "required",
            "evidence for different run bytes is not proof for the current ones"
        );
        assert!(
            v.legacy_evidence.is_some(),
            "legacy evidence must be disclosed"
        );
        assert!(v.evidence.is_none());
        let v = run_verification(
            "case-a",
            "evals/cases/case-a/run.json",
            "abc",
            Some("make verify".to_string()),
            target,
            &[row("disagrees", Some("abc"))],
        );
        assert_eq!(
            v.state, "disagrees",
            "a fingerprint-bound disagreement is binding"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn repository_status_parses_the_unborn_branch_name() {
        // "## No commits yet on master" — the unborn-branch form — used
        // to parse as branch "No" (the first word of the notice). The
        // actual branch name must survive.
        let root = tmp_root("unborn");
        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .output();
        if !init.is_ok_and(|o| o.status.success()) {
            return; // no git in this environment: nothing to assert
        }
        let st = repository_status(&root);
        assert!(
            st.branch.as_deref().is_some_and(|b| b != "No"),
            "the unborn branch must be its real name, got {:?}",
            st.branch
        );
        assert!(
            st.branch
                .as_deref()
                .is_some_and(|b| b == "master" || b == "main" || b == "trunk"),
            "unborn branch is a plain name, got {:?}",
            st.branch
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plain_path_segment_rejects_delimiters_and_dots() {
        assert!(plain_path_segment("2026-08-09"));
        assert!(plain_path_segment("a_b-c.d"));
        assert!(!plain_path_segment(""));
        assert!(!plain_path_segment("."));
        assert!(!plain_path_segment(".."));
        assert!(!plain_path_segment(".git"));
        assert!(!plain_path_segment("a/b"));
        assert!(!plain_path_segment("a%2Fb"));
        assert!(!plain_path_segment("a?b"));
        assert!(!plain_path_segment("a b"));
    }

    #[test]
    fn system_time_ms_pre_epoch_is_zero() {
        let before_epoch = std::time::SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(1);
        assert_eq!(system_time_ms(before_epoch), 0);
        assert_eq!(
            system_time_ms(std::time::SystemTime::UNIX_EPOCH),
            0,
            "the epoch itself is zero, not a negative wrap"
        );
        let one_sec = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
        assert_eq!(system_time_ms(one_sec), 1000);
    }
}
