//! Background dispatch for the AFK supervisor (AFK v3, MCP bridge).
//!
//! `loop run --detach` spawns the same binary as a detached child and
//! returns immediately with a run handle — the launch-and-poll pattern
//! that survives MCP tool timeouts. The handle directory holds:
//! - `run.pid` — the detached supervisor process id
//! - `run.start` — the process start-time (identity against pid reuse)
//! - `launch.json` — the resolved run args (`run_status` uses it to
//!   find the workdir, the report path, the run kind)
//! - `launch.lock` — the one-run-per-workdir mutex (`create_new`)
//! - `run.out` — the supervisor's stdout/stderr (redirected)
//!
//! Liveness is identity-aware: alive = pid exists, state != Z (a
//! zombie reports dead), and the recorded start-time matches.
//!
//! Concurrency: the MCP server is single-threaded (requests serialize),
//! so launches through the bridge cannot race. Two concurrent CLI
//! `--detach` calls in the same workdir are serialized by the
//! `create_new` lock; the stale-lock recovery renames the stale lock
//! aside and re-creates it, then VERIFIES the lock content is still
//! the caller's before spawning — the residual theoretical window (a
//! second caller renaming the freshly-created lock between the
//! verification and the spawn) is documented, not exploitable through
//! the bridge.
//!
//! The child is a plain `std::process::Command` spawn: no double-fork,
//! no daemonization — the run survives the MCP server's exit because it
//! is a separate process owned by the kernel's process tree. When the
//! child exits while the (long-lived) MCP server still runs, it may
//! linger as a zombie record until the server exits (init reaps it
//! then); `is_alive` never misreports it. On non-Linux targets a
//! launched run reports alive=false (documented limitation — the
//! sandbox worker is Linux-only anyway).

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
    // One run per workdir (F4): the lock is taken atomically
    // (create_new). The lock CONTENT is the holder's identity
    // (pid + start-time); lock_holder_alive checks that identity, so a
    // crashed launch (dead launcher in the lock) is recovered. The
    // recovery renames the stale lock aside atomically — exactly one
    // concurrent caller wins the rename, so the create_new decides a
    // single holder (no remove-then-create double-acquire race).
    let lock = handle.join("launch.lock");
    let me = format!(
        "{} {}",
        std::process::id(),
        process_starttime(std::process::id())
    );
    // Claim the lock (read-back on BOTH paths): after any acquisition
    // the content is verified to still be ours before the spawn, so a
    // concurrent caller that renamed our freshly-created lock loses the
    // verification and refuses.
    claim_lock(&handle, &me)?;
    // Any failure BEFORE the spawn releases the lock (no leak); the
    // post-spawn cleanup removes the whole handle dir.
    let fail_before_spawn = |e: std::io::Error| -> std::io::Result<PathBuf> {
        let _ = std::fs::remove_file(&lock);
        Err(e)
    };
    let out = match std::fs::File::create(handle.join("run.out")) {
        Ok(f) => f,
        Err(e) => return fail_before_spawn(e),
    };
    let err = match out.try_clone() {
        Ok(f) => f,
        Err(e) => return fail_before_spawn(e),
    };
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
    let mut child = match std::process::Command::new(&exe)
        .args(&args)
        // The child must NOT inherit the parent's stdin: a codex exec
        // would block reading it (no EOF on an MCP server's pipe) and
        // the run would hang forever (the e2e caught this — the first
        // 'fix' attempt was silently lost to a pkill that killed its
        // own shell).
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(out))
        .stderr(std::process::Stdio::from(err))
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return fail_before_spawn(e),
    };
    // Atomic-ish launch protocol (F5): pid + identity first, launch.json
    // LAST — a failure after the spawn kills the child and removes the
    // handle so no unrecorded run lingers.
    let pid = child.id();
    let mut cleanup = || {
        let _ = child.kill();
        let _ = child.wait();
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
    // The lock now tracks the RUN's identity (the launcher exits next).
    if std::fs::write(&lock, format!("{pid} {start}")).is_err() {
        cleanup();
        return Err(std::io::Error::other("cannot update the launch lock"));
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
/// The launch lock is held while its recorded identity is alive: the
/// launcher's identity during the launch window, the run's identity
/// after the spawn. A crashed launcher (dead identity in the lock) is
/// stale and recovered by the next launch.
/// Claim the one-run-per-workdir lock, atomically and with read-back.
///
/// The identity content (`pid start`) makes the lock's holder
/// Atomically create a lock file containing `me` (fails when it exists).
fn create_lock_file(lock: &Path, me: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock)?;
    f.write_all(me.as_bytes())
}

/// verifiable; `lock_holder_alive` refuses while that identity lives.
/// A stale lock (dead/crashed holder, or a briefly-empty file) is
/// renamed aside and re-created; after EVERY acquisition the content
/// is read back and must still be ours, so a concurrent caller that
/// renamed the fresh lock loses and refuses. `me` is this caller's
/// identity.
fn claim_lock(handle: &Path, me: &str) -> std::io::Result<()> {
    let lock = handle.join("launch.lock");
    let create = || {
        create_lock_file(&lock, me)
            .map_err(|_| std::io::Error::other("a detached run is already active in this workdir"))
    };
    match create_lock_file(&lock, me) {
        Ok(()) => {}
        Err(_) if lock_holder_alive(handle) => {
            return Err(std::io::Error::other(
                "a detached run is already active in this workdir",
            ));
        }
        Err(_) => {
            // Stale (dead identity or a briefly-empty lock): rename
            // aside, then create.
            let gone = lock.with_extension(format!("stale-{}", std::process::id()));
            let _ = std::fs::rename(&lock, &gone);
            create()?;
        }
    }
    // Read-back on BOTH paths: the lock must still say OUR identity.
    let now = std::fs::read_to_string(&lock).unwrap_or_default();
    if now != me {
        return Err(std::io::Error::other(
            "a detached run is already active in this workdir",
        ));
    }
    Ok(())
}

fn lock_holder_alive(handle: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(handle.join("launch.lock")) else {
        return false;
    };
    let mut parts = content.split_whitespace();
    let (Some(pid), Some(start)) = (parts.next(), parts.next()) else {
        return false;
    };
    let (Ok(pid), Ok(start)) = (pid.parse::<u32>(), start.parse::<u64>()) else {
        return false;
    };
    process_live(pid, start)
}

/// Read `/proc/<pid>/stat` once: the process state (field 2) and the
/// start-time (field 21) — the two fields every liveness check needs.
fn proc_stat(pid: u32) -> Option<(String, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = stat.split_whitespace().collect();
    let state = fields.get(2).copied().unwrap_or("?").to_string();
    let proc_start = fields
        .get(21)
        .and_then(|f| f.parse::<u64>().ok())
        .unwrap_or(0);
    Some((state, proc_start))
}

/// A process instance is alive when it exists, is not a zombie, and its
/// start-time matches the recorded identity.
fn process_live(pid: u32, recorded_start: u64) -> bool {
    if recorded_start == 0 {
        return false;
    }
    let Some((state, starttime)) = proc_stat(pid) else {
        return false;
    };
    state != "Z" && starttime == recorded_start
}

/// Whether the report file for this (already parsed) launch exists.
fn report_ready_checked(launch: &LaunchInfo) -> bool {
    launch_report_from(launch).is_some_and(|p| p.is_file())
}

fn process_starttime(pid: u32) -> u64 {
    proc_stat(pid).map_or(0, |(_, start)| start)
}

/// Liveness of a launched run: the process exists AND its Linux
/// start-time matches the recorded identity AND it is not a zombie
/// (state Z). A stale pid (reused or zombie) reports dead.
#[must_use]
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
        process_live(pid, recorded)
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
/// Handle authority (F3): parse launch.json ONCE and validate it — the
/// handle must be named `.supervisor`, its parent must be the launch's
/// workdir, and the report path must live inside that workdir. Returns
/// the validated launch; `None` for anything else (an arbitrary path
/// the client points at).
fn validated_launch(handle: &Path) -> Option<LaunchInfo> {
    if !matches!(handle.file_name(), Some(n) if n == ".supervisor") {
        return None;
    }
    let parent = handle.parent()?;
    let launch: LaunchInfo = std::fs::read_to_string(handle.join("launch.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    if Path::new(&launch.workdir) != parent {
        return None;
    }
    launch_report_from(&launch).is_some().then_some(launch)
}

/// The report path of a launch, constrained to the workdir (F3): the
/// auto-approved read tools must never follow a path outside the
/// authorized workdir. Both sides are canonicalized so a symlink placed
/// inside the workdir (REPORT.md -> /etc/passwd) cannot escape it.
fn launch_report_from(launch: &LaunchInfo) -> Option<std::path::PathBuf> {
    let workdir = Path::new(&launch.workdir);
    let report = Path::new(&launch.report);
    let canon_report = std::fs::canonicalize(report).unwrap_or_else(|_| report.to_path_buf());
    let canon_workdir = std::fs::canonicalize(workdir).unwrap_or_else(|_| workdir.to_path_buf());
    if canon_report.starts_with(&canon_workdir) {
        Some(report.to_path_buf())
    } else {
        None
    }
}

/// Handle authority (F3): a valid handle is `<workdir>/.supervisor`
/// whose launch.json names that same workdir. Anything else (an
/// arbitrary path the client points at) is invalid.
#[must_use]
// Consumed by the MCP tools (AFK v3 S2).
#[cfg(test)]
pub fn valid_handle(handle: &Path) -> bool {
    validated_launch(handle).is_some()
}

/// The report path of a launch, constrained to the workdir (F3): the
/// auto-approved read tools must never follow a path outside the
/// authorized workdir.
#[must_use]
// Consumed by the MCP tools (AFK v3 S2).
#[cfg(test)]
pub fn launch_report(handle: &Path) -> Option<std::path::PathBuf> {
    validated_launch(handle).and_then(|l| launch_report_from(&l))
}

/// A background run's status snapshot.
// Consumed by the MCP tools (AFK v3 S2).
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
pub fn run_status(handle: &Path) -> BgStatus {
    // Single validated read (F3): launch.json is parsed ONCE and the
    // result drives everything — no guard-then-reread TOCTOU window.
    let Some(launch) = validated_launch(handle) else {
        return BgStatus {
            alive: false,
            workdir: None,
            report: None,
            report_ready: false,
            progress_tail: None,
        };
    };
    // Release the launch lock once the run is observably finished, so
    // the next launch does not need a stale-lock dance.
    if !is_alive(handle) && report_ready_checked(&launch) {
        let _ = std::fs::remove_file(handle.join("launch.lock"));
    }
    let workdir = launch.workdir.clone();
    let report = launch_report_from(&launch).map(|p| p.to_string_lossy().into_owned());
    let report_ready = report.as_ref().is_some_and(|r| Path::new(r).is_file());
    let progress_tail = Path::new(&workdir).join("progress.md").is_file().then(|| {
        let text =
            std::fs::read_to_string(Path::new(&workdir).join("progress.md")).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        lines
            .iter()
            .rev()
            .take(20)
            .rev()
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    });
    BgStatus {
        alive: is_alive(handle),
        workdir: Some(workdir),
        report,
        report_ready,
        progress_tail,
    }
}

/// The run report text when ready.
#[must_use]
// Consumed by the MCP tools (AFK v3 S2).
pub fn run_report_text(handle: &Path) -> Option<String> {
    let launch = validated_launch(handle)?;
    let report = launch_report_from(&launch)?;
    std::fs::read_to_string(&report).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        static CLEANUP: std::sync::Once = std::sync::Once::new();
        CLEANUP.call_once(|| {
            // Each cargo test run uses a fresh pid in the temp-dir name, so
            // bg temp roots pile up until the tmpfs inode quota is hit.
            // Remove stale ones (older than a day) once per process.
            let age = std::time::Duration::from_hours(24);
            let now = std::time::SystemTime::now();
            if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if !name.starts_with("mag-") {
                        continue;
                    }
                    let stale = e
                        .metadata()
                        .and_then(|m| m.modified())
                        .is_ok_and(|t| now.duration_since(t).unwrap_or_default() > age);
                    if stale {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            }
        });
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
        std::thread::sleep(std::time::Duration::from_millis(500));
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
        // Fix the workdir to the handle parent -> the handle is STILL
        // invalid while the report path sits outside the workdir (the
        // strengthened authority binds the whole launch).
        let launch2 = LaunchInfo {
            workdir: root.to_string_lossy().into_owned(),
            ..launch
        };
        std::fs::write(
            handle.join("launch.json"),
            serde_json::to_string(&launch2).unwrap(),
        )
        .unwrap();
        assert!(!valid_handle(&handle), "/etc/passwd invalidates the handle");
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
    fn claim_lock_fresh_and_stale_and_live() {
        let root = tmp_root("claim");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        let me = format!(
            "{} {}",
            std::process::id(),
            process_starttime(std::process::id())
        );
        // Fresh claim succeeds and the content is ours (read-back).
        claim_lock(&handle, &me).unwrap();
        assert_eq!(
            std::fs::read_to_string(handle.join("launch.lock")).unwrap(),
            me
        );
        // A live holder refuses.
        let mut holder = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let hme = format!("{} {}", holder.id(), process_starttime(holder.id()));
        std::fs::write(handle.join("launch.lock"), &hme).unwrap();
        assert!(claim_lock(&handle, &me).is_err(), "a live holder refuses");
        let _ = holder.kill();
        let _ = holder.wait();
        // A stale lock (dead identity) is recovered.
        std::fs::write(handle.join("launch.lock"), "999999999 1").unwrap();
        claim_lock(&handle, &me).unwrap();
        assert_eq!(
            std::fs::read_to_string(handle.join("launch.lock")).unwrap(),
            me
        );
        // A briefly-EMPTY lock (the race window) is stale, not live.
        std::fs::write(handle.join("launch.lock"), "").unwrap();
        assert!(!lock_holder_alive(&handle), "an empty lock is stale");
        claim_lock(&handle, &me).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lock_holder_liveness_semantics() {
        let root = tmp_root("lock");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        // No lock -> not held.
        assert!(!lock_holder_alive(&handle));
        // A live identity -> held.
        let mut child = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        std::fs::write(
            handle.join("launch.lock"),
            format!("{} {}", pid, process_starttime(pid)),
        )
        .unwrap();
        assert!(lock_holder_alive(&handle), "a live identity holds the lock");
        // A dead identity (reused start-time) -> stale.
        std::fs::write(handle.join("launch.lock"), format!("{pid} 1")).unwrap();
        assert!(!lock_holder_alive(&handle), "a dead identity is stale");
        // A zombie identity -> stale.
        let _ = child.kill();
        let _ = child.wait();
        let mut zombie = std::process::Command::new("sh")
            .args(["-c", "true"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let zpid = zombie.id();
        std::thread::sleep(std::time::Duration::from_millis(500));
        std::fs::write(
            handle.join("launch.lock"),
            format!("{} {}", zpid, process_starttime(zpid)),
        )
        .unwrap();
        assert!(
            !lock_holder_alive(&handle),
            "a zombie identity must be stale"
        );
        let _ = zombie.wait();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_lock_refuses_a_second_launch_before_spawn() {
        // With a live identity in the lock, spawn_detached must refuse
        // WITHOUT spawning anything (no .supervisor/run.out side effect
        // beyond the lock).
        let root = tmp_root("refuse");
        let workdir = root.join("wd");
        std::fs::create_dir_all(&workdir).unwrap();
        let handle = handle_for(&workdir);
        std::fs::create_dir_all(&handle).unwrap();
        let mut holder = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = holder.id();
        std::fs::write(
            handle.join("launch.lock"),
            format!("{} {}", pid, process_starttime(pid)),
        )
        .unwrap();
        let r = spawn_detached(
            "x",
            &workdir,
            "true",
            &workdir,
            1,
            None,
            None,
            false,
            None,
            None,
            &workdir.join("REPORT.md"),
            None,
            false,
            true,
        );
        assert!(r.is_err(), "an active lock must refuse");
        assert!(!workdir.join("run.out").exists(), "nothing spawned");
        let _ = holder.kill();
        let _ = holder.wait();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn status_reads_progress_tail_and_report_ready() {
        let root = tmp_root("status");
        let workdir = root.join("wd");
        std::fs::create_dir_all(&workdir).unwrap();
        // The handle must live INSIDE the workdir (authority guard).
        let handle = handle_for(&workdir);
        std::fs::create_dir_all(&handle).unwrap();
        let report = workdir.join("REPORT.md");
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

    #[test]
    fn symlinked_report_outside_workdir_is_rejected() {
        // F3 security: a REPORT.md symlink pointing outside the workdir
        // must not be readable through the auto-approved read path.
        let root = std::env::temp_dir().join(format!(
            "mag-bg-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workdir = root.join("wd");
        std::fs::create_dir_all(&workdir).unwrap();
        std::fs::write(root.join("secret.txt"), "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("secret.txt"), workdir.join("REPORT.md")).unwrap();
        let launch = LaunchInfo {
            goal_or_case: "g".into(),
            workdir: workdir.to_string_lossy().into_owned(),
            verify: "true".into(),
            target: String::new(),
            iterate: 1,
            max_wall: None,
            max_idle: None,
            blind_worker: false,
            hidden_dir: None,
            on_done: None,
            report: workdir.join("REPORT.md").to_string_lossy().into_owned(),
            template: None,
            no_resume: false,
            no_sandbox: false,
        };
        assert!(
            launch_report_from(&launch).is_none(),
            "symlink escaping the workdir must be rejected"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn is_alive_handles_garbage_and_empty_identity() {
        let root = tmp_root("alive-edges");
        let handle = handle_for(&root);
        std::fs::create_dir_all(&handle).unwrap();
        assert!(!is_alive(&handle), "no pid file means dead");
        std::fs::write(handle.join("run.pid"), "not-a-pid").unwrap();
        assert!(!is_alive(&handle), "an unparsable pid must report dead");
        std::fs::write(handle.join("run.pid"), "0").unwrap();
        assert!(!is_alive(&handle), "pid 0 must report dead");
        std::fs::write(handle.join("run.pid"), std::process::id().to_string()).unwrap();
        assert!(
            !is_alive(&handle),
            "a live pid with no recorded start-time must report dead"
        );
        let me = process_starttime(std::process::id()).to_string();
        std::fs::write(handle.join("run.start"), me).unwrap();
        assert!(
            is_alive(&handle),
            "a live identity with start-time is alive"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn process_live_requires_recorded_starttime() {
        let pid = std::process::id();
        assert!(
            !process_live(pid, 0),
            "recorded start 0 must never count as alive (pid-reuse guard)"
        );
        assert!(!process_live(0, 1), "pid 0 must never count as alive");
        assert!(
            !process_live(u32::MAX, 1),
            "a nonexistent pid must never count as alive"
        );
        let start = process_starttime(pid);
        assert!(
            start > 0 && process_live(pid, start),
            "our own identity must be live"
        );
    }

    #[test]
    fn proc_stat_unknown_pid_is_none() {
        assert!(
            proc_stat(u32::MAX).is_none(),
            "a nonexistent pid must not produce a stat row"
        );
        let pid = std::process::id();
        let (state, start) = proc_stat(pid).expect("our own pid has a stat row");
        assert!(
            matches!(state.as_str(), "R" | "S"),
            "we are running or sleeping, got state {state}"
        );
        assert!(start > 0, "the start-time field must parse");
    }
}
