//! Background dispatch for the AFK supervisor (AFK v3, MCP bridge).
//!
//! `loop run --detach` spawns the same binary as a detached child and
//! returns immediately with a run handle — the launch-and-poll pattern
//! that survives MCP tool timeouts. The handle directory holds:
//! - `run.pid` — the detached supervisor process id
//! - `launch.json` — the resolved run args (`run_status` uses it to
//!   find the workdir, the report path, the run kind)
//! - `run.out` — the supervisor's stdout/stderr (redirected)
//!
//! The child is a plain `std::process::Command` spawn: no double-fork,
//! no daemonization — the run survives the MCP server's exit because it
//! is a separate process owned by the kernel's process tree. On Linux
//! the liveness check reads `/proc/<pid>`; on other targets a launched
//! run reports alive=false (documented limitation — the sandbox worker
//! is Linux-only anyway).

use std::path::{Path, PathBuf};

/// Resolved launch arguments persisted at the handle.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LaunchInfo {
    /// The goal text or case name.
    pub goal_or_case: String,
    /// Worker workdir.
    pub workdir: String,
    /// The verifier command (resolved by the parent).
    pub verify: String,
    /// The verifier target (resolved by the parent).
    pub target: String,
    /// Iteration count.
    pub iterate: usize,
    /// Wall cap per attempt.
    pub max_wall: Option<u64>,
    /// Idle cap per attempt.
    pub max_idle: Option<u64>,
    /// Blind-worker mode.
    pub blind_worker: bool,
    /// Hidden-suite dir.
    pub hidden_dir: Option<String>,
    /// On-done hook.
    pub on_done: Option<String>,
    /// Report path (absolute).
    pub report: String,
    /// Loop template.
    pub template: Option<String>,
    /// Disable session resume.
    pub no_resume: bool,
    /// Skip the sandbox.
    pub no_sandbox: bool,
}

/// The run handle (the `.supervisor` dir inside the workdir).
pub fn handle_for(workdir: &Path) -> PathBuf {
    workdir.join(".supervisor")
}

/// Spawn the detached supervisor run for a case/ad-hoc goal.
///
/// The parent validates first (spec resolution, template, blind-worker
/// pairing) so the child starts clean; the child re-resolves cheaply
/// and runs the NORMAL `loop run` path (supervisor, report, on-done —
/// unchanged). Returns the handle path.
///
/// # Errors
///
/// Returns the launch error when the child cannot be spawned or the
/// handle cannot be written.
// The params mirror the CLI flags one-to-one; bundling would obscure
// the CLI mapping.
#[allow(clippy::too_many_arguments)]
pub fn spawn_detached(
    goal_or_case: &str,
    workdir: &Path,
    verify: &str,
    target: &Path,
    iterate: usize,
    max_wall: Option<u64>,
    max_idle: Option<u64>,
    blind_worker: bool,
    hidden_dir: Option<&Path>,
    on_done: Option<&str>,
    report: &Path,
    template: Option<&str>,
    no_resume: bool,
    no_sandbox: bool,
) -> std::io::Result<PathBuf> {
    let handle = handle_for(workdir);
    std::fs::create_dir_all(&handle)?;
    let launch = LaunchInfo {
        goal_or_case: goal_or_case.to_string(),
        workdir: workdir.to_string_lossy().into_owned(),
        verify: verify.to_string(),
        target: target.to_string_lossy().into_owned(),
        iterate,
        max_wall,
        max_idle,
        blind_worker,
        hidden_dir: hidden_dir.map(|p| p.to_string_lossy().into_owned()),
        on_done: on_done.map(str::to_string),
        report: report.to_string_lossy().into_owned(),
        template: template.map(str::to_string),
        no_resume,
        no_sandbox,
    };
    let exe = std::env::current_exe()?;
    // One run per workdir (F4): refuse when a run is already active.
    if is_alive(&handle) {
        return Err(std::io::Error::other(
            "a detached run is already active in this workdir",
        ));
    }
    let out = std::fs::File::create(handle.join("run.out"))?;
    let err = out.try_clone()?;
    let mut args: Vec<String> = vec![
        "loop".into(),
        "run".into(),
        goal_or_case.to_string(),
        "--workdir".into(),
        workdir.to_string_lossy().into_owned(),
        "--verify".into(),
        verify.to_string(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
        "--iterate".into(),
        iterate.to_string(),
        "--report".into(),
        report.to_string_lossy().into_owned(),
    ];
    if let Some(w) = max_wall {
        args.push("--max-wall".into());
        args.push(w.to_string());
    }
    if let Some(i) = max_idle {
        args.push("--max-idle".into());
        args.push(i.to_string());
    }
    if blind_worker {
        args.push("--blind-worker".into());
        if let Some(d) = hidden_dir {
            args.push("--hidden-dir".into());
            args.push(d.to_string_lossy().into_owned());
        }
    }
    if let Some(c) = on_done {
        args.push("--on-done".into());
        args.push(c.to_string());
    }
    if let Some(t) = template {
        args.push("--template".into());
        args.push(t.to_string());
    }
    if no_resume {
        args.push("--no-resume".into());
    }
    if no_sandbox {
        args.push("--no-sandbox".into());
    }
    let mut child = std::process::Command::new(&exe)
        .args(&args)
        // The child must NOT inherit the parent's stdin: a codex exec
        // would block reading it (no EOF on an MCP server's pipe) and
        // the run would hang forever (the e2e caught this — the first
        // 'fix' attempt was silently lost to a pkill that killed its
        // own shell).
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(out))
        .stderr(std::process::Stdio::from(err))
        .spawn()?;
    // Atomic-ish launch protocol (F5): pid + identity first, launch.json
    // LAST — a failure after the spawn kills the child and removes the
    // handle so no unrecorded run lingers.
    let pid = child.id();
    let mut cleanup = || {
        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&handle);
    };
    if std::fs::write(handle.join("run.pid"), pid.to_string()).is_err() {
        cleanup();
        return Err(std::io::Error::other("cannot write run.pid"));
    }
    // Process identity (F2): the Linux start-time from /proc/<pid>/stat
    // guards against pid reuse; a zombie (state Z) is NOT alive.
    let start = process_starttime(pid);
    if std::fs::write(handle.join("run.start"), start.to_string()).is_err() {
        cleanup();
        return Err(std::io::Error::other("cannot write run.start"));
    }
    let launch_json =
        serde_json::to_string_pretty(&launch).map_err(|e| std::io::Error::other(e.to_string()))?;
    if std::fs::write(handle.join("launch.json"), launch_json).is_err() {
        cleanup();
        return Err(std::io::Error::other("cannot write launch.json"));
    }
    Ok(handle)
}

/// Linux process start-time (field 22 of `/proc/<pid>/stat`); 0 when
/// unreadable. Combined with the pid it identifies a process instance
/// (guards against pid reuse; zombies keep their start-time but report
/// state Z).
fn process_starttime(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| stat.split_whitespace().nth(21).and_then(|f| f.parse().ok()))
        .unwrap_or(0)
}

/// Liveness of a launched run: the process exists AND its Linux
/// start-time matches the recorded identity AND it is not a zombie
/// (state Z). A stale pid (reused or zombie) reports dead.
#[must_use]
// Consumed by the MCP tools (AFK v3 S2).
#[allow(dead_code)]
pub fn is_alive(handle: &Path) -> bool {
    let pid: u32 = match std::fs::read_to_string(handle.join("run.pid")) {
        Ok(p) => p.trim().parse().unwrap_or(0),
        Err(_) => return false,
    };
    let recorded: u64 = std::fs::read_to_string(handle.join("run.start"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    #[cfg(target_os = "linux")]
    {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let fields: Vec<&str> = stat.split_whitespace().collect();
        let state = fields.get(2).copied().unwrap_or("?");
        let starttime = fields
            .get(21)
            .and_then(|f| f.parse::<u64>().ok())
            .unwrap_or(0);
        state != "Z" && starttime == recorded && recorded != 0
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pid, recorded);
        false
    }
}

/// Handle authority (F3): a valid handle is `<workdir>/.supervisor`
/// whose launch.json names that same workdir. Anything else (an
/// arbitrary path the client points at) is invalid.
#[must_use]
// Consumed by the MCP tools (AFK v3 S2).
#[allow(dead_code)]
pub fn valid_handle(handle: &Path) -> bool {
    if !matches!(handle.file_name(), Some(n) if n == ".supervisor") {
        return false;
    }
    let Some(parent) = handle.parent() else {
        return false;
    };
    std::fs::read_to_string(handle.join("launch.json")).is_ok_and(|text| {
        serde_json::from_str::<LaunchInfo>(&text).is_ok_and(|l| Path::new(&l.workdir) == parent)
    })
}

/// The report path of a launch, constrained to the workdir (F3): the
/// auto-approved read tools must never follow a path outside the
/// authorized workdir.
#[must_use]
// Consumed by the MCP tools (AFK v3 S2).
#[allow(dead_code)]
pub fn launch_report(handle: &Path) -> Option<std::path::PathBuf> {
    let launch: LaunchInfo = std::fs::read_to_string(handle.join("launch.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    let workdir = Path::new(&launch.workdir);
    let report = Path::new(&launch.report);
    // The report must live inside the workdir.
    if report.starts_with(workdir) {
        Some(report.to_path_buf())
    } else {
        None
    }
}

/// A background run's status snapshot.
// Consumed by the MCP tools (AFK v3 S2).
#[allow(dead_code)]
pub struct BgStatus {
    /// Whether the supervisor process is still running.
    pub alive: bool,
    /// The workdir (from launch.json).
    pub workdir: Option<String>,
    /// The report path (from launch.json).
    pub report: Option<String>,
    /// Whether the report file exists yet.
    pub report_ready: bool,
    /// The progress.md tail (last ~20 lines) when readable.
    pub progress_tail: Option<String>,
}

/// Read the status of a launched run.
#[must_use]
// Consumed by the MCP tools (AFK v3 S2).
#[allow(dead_code)]
pub fn run_status(handle: &Path) -> BgStatus {
    let launch: Option<LaunchInfo> = std::fs::read_to_string(handle.join("launch.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let workdir = launch.as_ref().map(|l| l.workdir.clone());
    let report = launch.as_ref().map(|l| l.report.clone());
    let report_ready = report.as_ref().is_some_and(|r| Path::new(r).is_file());
    let progress_tail = workdir.as_ref().and_then(|w| {
        let path = Path::new(w).join("progress.md");
        let text = std::fs::read_to_string(&path).ok()?;
        let lines: Vec<&str> = text.lines().collect();
        Some(
            lines
                .iter()
                .rev()
                .take(20)
                .rev()
                .copied()
                .collect::<Vec<_>>()
                .join("\n"),
        )
    });
    BgStatus {
        alive: is_alive(handle),
        workdir,
        report,
        report_ready,
        progress_tail,
    }
}

/// The run report text when ready.
#[must_use]
// Consumed by the MCP tools (AFK v3 S2).
#[allow(dead_code)]
pub fn run_report_text(handle: &Path) -> Option<String> {
    let report = std::fs::read_to_string(handle.join("launch.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<LaunchInfo>(&t).ok())
        .map(|l| l.report)?;
    std::fs::read_to_string(&report).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-bg-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn launch_json_roundtrip() {
        let root = tmp_root("roundtrip");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        let launch = LaunchInfo {
            goal_or_case: "afk-max-idle".into(),
            workdir: root.to_string_lossy().into_owned(),
            verify: "make verify".into(),
            target: root.to_string_lossy().into_owned(),
            iterate: 3,
            max_wall: Some(60),
            max_idle: None,
            blind_worker: true,
            hidden_dir: Some("hidden".into()),
            on_done: None,
            report: root.join("REPORT.md").to_string_lossy().into_owned(),
            template: Some("sequential-reviewer".into()),
            no_resume: false,
            no_sandbox: true,
        };
        std::fs::write(
            handle.join("launch.json"),
            serde_json::to_string(&launch).unwrap(),
        )
        .unwrap();
        let read: LaunchInfo =
            serde_json::from_str(&std::fs::read_to_string(handle.join("launch.json")).unwrap())
                .unwrap();
        assert_eq!(read.goal_or_case, "afk-max-idle");
        assert_eq!(read.iterate, 3);
        assert!(read.blind_worker);
        assert_eq!(read.template.as_deref(), Some("sequential-reviewer"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_handle_reports_dead() {
        let root = tmp_root("dead");
        assert!(!is_alive(&handle_for(&root)));
        assert!(!run_status(&handle_for(&root)).alive);
        assert!(run_report_text(&handle_for(&root)).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn live_process_reports_alive() {
        let root = tmp_root("alive");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        let mut child = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        std::fs::write(handle.join("run.pid"), pid.to_string()).unwrap();
        std::fs::write(handle.join("run.start"), process_starttime(pid).to_string()).unwrap();
        assert!(is_alive(&handle), "a running process must report alive");
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn zombie_reports_dead_and_is_reaped_by_wait() {
        // A real zombie: the child exits, we do NOT reap it -> state Z.
        let root = tmp_root("zombie");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        let mut child = std::process::Command::new("sh")
            .args(["-c", "true"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        std::fs::write(handle.join("run.pid"), pid.to_string()).unwrap();
        std::fs::write(handle.join("run.start"), process_starttime(pid).to_string()).unwrap();
        // Give the child a moment to exit and become a zombie.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let state = stat.split_whitespace().nth(2).unwrap();
        assert_eq!(state, "Z", "the test must actually observe a zombie");
        assert!(!is_alive(&handle), "a zombie is NOT alive");
        let _ = child.wait(); // reap
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pid_reuse_with_wrong_starttime_reports_dead() {
        let root = tmp_root("reuse");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        let child = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        std::fs::write(handle.join("run.pid"), pid.to_string()).unwrap();
        // A start-time that does not match the process.
        std::fs::write(handle.join("run.start"), "1").unwrap();
        assert!(
            !is_alive(&handle),
            "start-time identity must guard pid reuse"
        );
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn handle_authority_and_report_path_are_enforced() {
        let root = tmp_root("authority");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        let workdir = root.join("wd");
        std::fs::create_dir_all(&workdir).unwrap();
        let launch = LaunchInfo {
            goal_or_case: "x".into(),
            workdir: workdir.to_string_lossy().into_owned(),
            verify: "true".into(),
            target: workdir.to_string_lossy().into_owned(),
            iterate: 1,
            max_wall: None,
            max_idle: None,
            blind_worker: false,
            hidden_dir: None,
            on_done: None,
            report: "/etc/passwd".into(),
            template: None,
            no_resume: false,
            no_sandbox: true,
        };
        std::fs::write(
            handle.join("launch.json"),
            serde_json::to_string(&launch).unwrap(),
        )
        .unwrap();
        // The workdir mismatch (handle parent != launch workdir) invalidates.
        assert!(
            !valid_handle(&handle),
            "workdir must match the handle parent"
        );
        // A report path outside the workdir is refused.
        assert!(
            launch_report(&handle).is_none(),
            "no reads outside the workdir"
        );
        // Fix the workdir to the handle parent -> valid, but the report
        // path is still outside.
        let launch2 = LaunchInfo {
            workdir: root.to_string_lossy().into_owned(),
            ..launch
        };
        std::fs::write(
            handle.join("launch.json"),
            serde_json::to_string(&launch2).unwrap(),
        )
        .unwrap();
        assert!(valid_handle(&handle));
        assert!(launch_report(&handle).is_none(), "/etc/passwd is outside");
        // An in-workdir report is readable.
        let launch3 = LaunchInfo {
            report: root.join("REPORT.md").to_string_lossy().into_owned(),
            ..launch2
        };
        std::fs::write(
            handle.join("launch.json"),
            serde_json::to_string(&launch3).unwrap(),
        )
        .unwrap();
        assert_eq!(launch_report(&handle).unwrap(), root.join("REPORT.md"));
        // A non-.supervisor name is invalid even with a launch.json.
        let fake = root.join("other");
        std::fs::create_dir_all(&fake).unwrap();
        std::fs::copy(handle.join("launch.json"), fake.join("launch.json")).unwrap();
        assert!(!valid_handle(&fake));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn status_reads_progress_tail_and_report_ready() {
        let root = tmp_root("status");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        let workdir = root.join("wd");
        std::fs::create_dir_all(&workdir).unwrap();
        let report = root.join("REPORT.md");
        let launch = LaunchInfo {
            goal_or_case: "x".into(),
            workdir: workdir.to_string_lossy().into_owned(),
            verify: "true".into(),
            target: workdir.to_string_lossy().into_owned(),
            iterate: 1,
            max_wall: None,
            max_idle: None,
            blind_worker: false,
            hidden_dir: None,
            on_done: None,
            report: report.to_string_lossy().into_owned(),
            template: None,
            no_resume: false,
            no_sandbox: true,
        };
        std::fs::write(
            handle.join("launch.json"),
            serde_json::to_string(&launch).unwrap(),
        )
        .unwrap();
        let mut progress = String::new();
        for i in 0..30 {
            use std::fmt::Write as _;
            let _ = writeln!(progress, "line {i}");
        }
        std::fs::write(workdir.join("progress.md"), &progress).unwrap();
        let st = run_status(&handle);
        assert!(!st.report_ready);
        let tail = st.progress_tail.unwrap();
        assert!(tail.contains("line 29"), "tail must show the newest lines");
        assert!(!tail.contains("line 0"), "tail is the last 20 lines");
        std::fs::write(
            &report,
            "# report\n- final outcome: PASSED (review approved)\n",
        )
        .unwrap();
        assert!(run_status(&handle).report_ready);
        assert!(run_report_text(&handle).unwrap().contains("PASSED"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
