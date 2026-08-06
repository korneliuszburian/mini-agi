//! Parallel-planner manifest + fail-closed validation (AFK v4).
//! The batch coordinator (S2-S5) consumes this module; until then the
//! binary build sees it as dead code.
#![allow(dead_code)]
//!
//! The planner pass (a read-only codex session) emits a STRICT versioned
//! JSON manifest; the kernel parses it FAIL-CLOSED — any violation
//! (unknown field, duplicate id, empty goal/scope, absolute or
//! traversing paths, protected paths in scope, overlapping scopes,
//! out-of-shape verifier) fails the whole batch before anything is
//! dispatched. This is deliberately NOT the tolerant review parser:
//! a review verdict is a recorded disposition, a dispatch manifest is
//! authority (second opinion, finding 3).

use std::path::Path;

/// Manifest schema version (bump on any breaking shape change).
pub const MANIFEST_VERSION: u32 = 1;

/// Paths the parallel template NEVER lets a ticket touch: the gate and
/// its inputs must stay immutable across a batch, or the final truth
/// could be self-modified (second opinion, finding 1).
pub const PROTECTED_PATHS: &[&str] = &[
    "scripts/verify.sh",
    "scripts/gate-lib.sh",
    "evals",
    "memory",
    "tickets",
    "docs/adr",
];

/// Files that are EVIDENCE or discipline artifacts, never repo
/// content: the supervisor's reports/transcripts in the worktree root
/// and the worker's own checkpoint journal (a codex worker in a
/// worktree follows the repo's AGENTS.md and runs checkpoint.sh, which
/// journals the worktree-local memory/episodic/checkpoints.log — that
/// trail stays out of the merge and out of the containment check).
const EVIDENCE_PATHS: &[&str] = &[
    "REPORT.md",
    "progress.md",
    "run.json",
    "codex.log",
    "memory/episodic/checkpoints.log",
];

/// One ticket of a parallel batch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerTicket {
    /// Unique ticket id (the merge order is the id order).
    pub id: String,
    /// The ticket goal (the worker prompt).
    pub goal: String,
    /// Repo-relative paths the ticket may touch (dirs or files),
    /// mutually disjoint across the batch.
    pub scope: Vec<String>,
    /// The deterministic verifier command, WORKTREE-RELATIVE (runs
    /// with cwd = the ticket worktree root).
    pub verify: String,
    /// Worktree-relative verify target (default ".").
    #[serde(default = "default_target")]
    pub verify_target: String,
}

fn default_target() -> String {
    ".".to_string()
}

/// The strict planner manifest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerManifest {
    pub version: u32,
    pub tickets: Vec<PlannerTicket>,
}

/// A relative path is valid when it stays inside the repo: not
/// absolute, no drive letters, no `..`, no leading `~`, and never the
/// empty path.
fn valid_relative_path(p: &str) -> bool {
    if p.is_empty() || p.starts_with('/') || p.starts_with('~') || p.starts_with('\\') {
        return false;
    }
    if p.split(['/', '\\']).any(|seg| seg == "..") {
        return false;
    }
    if p.contains(':') {
        return false;
    }
    true
}

/// Verifier-shape allowlist (second opinion, finding 5): the command
/// must not reference absolute paths, home-relative paths, traversal,
/// or protected paths — it runs inside the ticket worktree only.
pub fn valid_verifier_shape(verify: &str) -> bool {
    if verify.trim().is_empty() {
        return false;
    }
    // Tokenize on whitespace and shell quotes; every token that looks
    // like a path must be relative and safe.
    for token in verify.split_whitespace() {
        let token = token.trim_matches(['\'', '"', '(', ')', ';', '&', '|', '$', '`', '>', '<']);
        if token.is_empty() {
            continue;
        }
        if token.starts_with('/') || token.starts_with('~') || token.starts_with("$HOME") {
            return false;
        }
        if token.split('/').any(|seg| seg == "..") {
            return false;
        }
        if PROTECTED_PATHS
            .iter()
            .any(|p| token == *p || token.starts_with(&format!("{p}/")))
        {
            return false;
        }
    }
    true
}

/// Parse + validate the planner manifest fail-closed.
///
/// # Errors
///
/// Returns the first violation as a message; the batch must NOT start.
pub fn parse_manifest(text: &str) -> Result<PlannerManifest, String> {
    // Typed deserialization with deny_unknown_fields: unknown fields AND
    // duplicate JSON keys are rejected by the derive (serde errors on a
    // duplicated struct field) — the authority boundary is strict.
    let manifest: PlannerManifest =
        serde_json::from_str(text).map_err(|e| format!("manifest is not valid: {e}"))?;
    if manifest.version != MANIFEST_VERSION {
        return Err(format!(
            "manifest.version {} != supported {MANIFEST_VERSION}",
            manifest.version
        ));
    }
    if manifest.tickets.is_empty() {
        return Err("manifest.tickets must not be empty".to_string());
    }
    if manifest.tickets.len() > 16 {
        return Err(format!(
            "manifest.tickets has {} tickets; the batch admission cap is 16",
            manifest.tickets.len()
        ));
    }
    let mut seen_ids = std::collections::HashSet::new();
    let mut all_scopes: Vec<(String, Vec<String>)> = Vec::new();
    let mut tickets = Vec::new();
    for ticket in &manifest.tickets {
        let id = ticket.id.clone();
        if id.trim().is_empty() {
            return Err("a ticket id must not be empty".to_string());
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "ticket id '{id}' must match [A-Za-z0-9_-]+ (it names the branch and worktree)"
            ));
        }
        if !seen_ids.insert(id.clone()) {
            return Err(format!("duplicate ticket id '{id}'"));
        }
        if ticket.goal.trim().is_empty() {
            return Err(format!("ticket {id}: goal must not be empty"));
        }
        if ticket.scope.is_empty() {
            return Err(format!("ticket {id}: scope must not be empty"));
        }
        let mut scope_paths = Vec::new();
        for s in &ticket.scope {
            if !valid_relative_path(s) {
                return Err(format!(
                    "ticket {id}: scope entry '{s}' must be a safe repo-relative path"
                ));
            }
            if touches_protected(s) {
                return Err(format!(
                    "ticket {id}: scope entry '{s}' touches a protected path (the gate must stay immutable)"
                ));
            }
            scope_paths.push(s.clone());
        }
        if !valid_verifier_shape(&ticket.verify) {
            return Err(format!(
                "ticket {id}: verifier must be worktree-relative with no absolute/traversing/protected paths"
            ));
        }
        if !valid_relative_path(&ticket.verify_target) {
            return Err(format!(
                "ticket {id}: verify_target must be a safe relative path"
            ));
        }
        tickets.push(PlannerTicket {
            id,
            goal: ticket.goal.clone(),
            scope: scope_paths.clone(),
            verify: ticket.verify.clone(),
            verify_target: ticket.verify_target.clone(),
        });
        all_scopes.push((tickets.last().unwrap().id.clone(), scope_paths));
    }
    // Scope disjointness across the batch (second opinion, finding 4):
    // overlapping scopes are refused BEFORE dispatch.
    for (i, (id_a, scopes_a)) in all_scopes.iter().enumerate() {
        for (id_b, scopes_b) in all_scopes.iter().skip(i + 1) {
            for a in scopes_a {
                for b in scopes_b {
                    if a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
                    {
                        return Err(format!(
                            "tickets {id_a} and {id_b} have overlapping scopes ('{a}' vs '{b}')"
                        ));
                    }
                }
            }
        }
    }
    Ok(PlannerManifest {
        version: manifest.version,
        tickets,
    })
}

pub fn touches_protected(path: &str) -> bool {
    PROTECTED_PATHS
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")))
}

/// The ticket worktree root for a batch: `<base>/.batch/<id>`.
#[must_use]
pub fn ticket_worktree(base: &Path, id: &str) -> std::path::PathBuf {
    base.join(".batch").join(id)
}

/// A provisioned batch: one worktree per ticket, each at `base_sha`.
#[derive(Debug)]
pub struct BatchProvision {
    /// The base commit the batch was cut from.
    pub base_sha: String,
    /// One worktree per ticket (id order).
    pub worktrees: Vec<std::path::PathBuf>,
}

/// Provision the batch: create one git worktree per ticket at
/// `base_sha` (branch `batch/<short-sha>/<id>`), then PRE-FLIGHT each
/// ticket: the verifier must pass in its worktree (dry-run) and must
/// NOT pass an empty counterfactual (vacuity — a vacuous verifier
/// would let garbage through). Any failure removes everything created
/// so far and fails the batch (no half-provisioned state).
///
/// # Errors
///
/// Returns the first failure; all created worktrees are removed.
pub fn provision_batch(
    repo: &Path,
    manifest: &PlannerManifest,
    base_sha: &str,
) -> Result<BatchProvision, String> {
    let short = &base_sha[..base_sha.len().min(10)];
    let mut created: Vec<std::path::PathBuf> = Vec::new();
    let cleanup = |created: &[std::path::PathBuf]| {
        for wt in created {
            let _ = std::process::Command::new("git")
                .args(["worktree", "remove", "--force"])
                .arg(wt)
                .current_dir(repo)
                .status();
        }
    };
    for ticket in &manifest.tickets {
        let wt = ticket_worktree(repo, &ticket.id);
        if wt.exists() {
            cleanup(&created);
            return Err(format!(
                "ticket {}: worktree path {} already exists (a batch may already be in progress)",
                ticket.id,
                wt.display()
            ));
        }
        let branch = format!("batch/{short}/{}", ticket.id);
        // A rerun after a failed batch may find the branch (evidence
        // preserved) — attach to it instead of failing.
        let mut status = std::process::Command::new("git")
            .args(["worktree", "add", "-b"])
            .arg(&branch)
            .arg(&wt)
            .arg(base_sha)
            .current_dir(repo)
            .status()
            .map_err(|e| {
                cleanup(&created);
                format!("git worktree add failed: {e}")
            })?;
        if !status.success() {
            status = std::process::Command::new("git")
                .args(["worktree", "add"])
                .arg(&wt)
                .arg(&branch)
                .current_dir(repo)
                .status()
                .map_err(|e| {
                    cleanup(&created);
                    format!("git worktree attach failed: {e}")
                })?;
        }
        if !status.success() {
            cleanup(&created);
            return Err(format!(
                "git worktree add/attach failed for ticket {} (existing branch?)",
                ticket.id
            ));
        }
        created.push(wt.clone());
        // Dry-run (executability): the verifier is a POST-condition —
        // it legitimately fails BEFORE the worker runs. Pre-flight only
        // rejects commands that cannot execute at all (spawn error or
        // cap abort); a real exit (0 or 1) means the gate runs.
        let target = wt.join(&ticket.verify_target);
        let dry =
            mini_agi_core::worker::run_capped("sh", &["-c", &ticket.verify], &target, Some(120));
        let dry_ok = dry.is_ok_and(|r| !r.aborted && r.status.is_some_and(|c| c == 0 || c == 1));
        if !dry_ok {
            cleanup(&created);
            return Err(format!(
                "ticket {}: verifier dry-run did not EXECUTE (spawn/abort, or exit outside {{0,1}} — a missing executable?)",
                ticket.id
            ));
        }
        // Vacuity: the verifier must NOT pass an empty counterfactual.
        let audit = mini_agi_core::verifier::audit_verifier_vacuous(&wt, &ticket.verify)
            .map_err(|e| format!("ticket {}: verify-audit: {e}", ticket.id))?;
        if audit.is_vacuous {
            cleanup(&created);
            return Err(format!(
                "ticket {}: verifier is VACUOUS (passes an empty target) — refuse",
                ticket.id
            ));
        }
    }
    Ok(BatchProvision {
        base_sha: base_sha.to_string(),
        worktrees: created,
    })
}

/// One ticket's outcome after the batch dispatch.
#[derive(Debug)]
pub struct BatchTicketResult {
    /// The ticket id.
    pub id: String,
    /// The bg run handle (evidence: run.out, run.pid, launch.json).
    pub handle: std::path::PathBuf,
    /// The worktree (the produced changes live here).
    pub worktree: std::path::PathBuf,
    /// The supervisor's final outcome (the report's final line).
    pub passed: bool,
    /// The report path when the run finished.
    pub report: Option<std::path::PathBuf>,
}

/// The batch outcome after dispatch+poll.
#[derive(Debug)]
pub struct BatchDispatchResult {
    pub results: Vec<BatchTicketResult>,
    /// Respawn events (D6): crashed tickets relaunched by the
    /// dispatcher, MAST-classified, never silent.
    pub respawns: Vec<String>,
}

/// Dispatch the batch: per-ticket detached runs (`loop run --detach`)
/// in each worktree, admission-capped at `max_parallel`, polled until
/// every ticket is done or the aggregate deadline passes. The marker
/// of each worker session carries the ticket identity via
/// `MINIAGI_SESSION_TAG` (parallel sessions cannot be misattributed).
///
/// # Errors
///
/// Returns when a launch fails (the batch stops).
/// Max respawns per crashed ticket (D6): a dead worker without a report
/// is relaunched this many times before the batch records it as failed.
const MAX_TICKET_RESPAWNS: usize = 2;

/// D6 crash classification: a dead ticket is a CRASH when no report
/// exists — a verdict always lands in a report, so "dead and silent" is
/// an abnormal termination, never a result.
const fn is_crash(report_ready: bool, report_text: Option<&str>) -> bool {
    !report_ready && report_text.is_none()
}

/// D6 respawn budget: a crashed ticket is relaunched while its respawn
/// count is below the bound; the batch then records it as failed with
/// the crash evidence (never silently).
const fn respawn_allowed(respawn_count: usize) -> bool {
    respawn_count < MAX_TICKET_RESPAWNS
}

pub fn dispatch_batch(
    repo: &Path,
    provision: &BatchProvision,
    manifest: &PlannerManifest,
    max_parallel: usize,
    iterate: usize,
    wall_cap: Option<u64>,
    no_sandbox: bool,
) -> Result<BatchDispatchResult, String> {
    let max_parallel = max_parallel.max(1);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut results: Vec<BatchTicketResult> = Vec::new();
    let mut active: Vec<(String, std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    let mut queue: Vec<&PlannerTicket> = manifest.tickets.iter().collect();
    let mut respawns: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut respawn_log: Vec<String> = Vec::new();
    let batch_deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(
            wall_cap
                .unwrap_or(1800)
                .saturating_mul(max_parallel as u64)
                .max(600),
        );
    let launch =
        |ticket: &PlannerTicket,
         active: &mut Vec<(String, std::path::PathBuf, std::path::PathBuf)>| {
            let wt = ticket_worktree(repo, &ticket.id);
            let target = wt.join(&ticket.verify_target);
            let tag = format!(
                "{}/{}",
                &provision.base_sha[..provision.base_sha.len().min(10)],
                ticket.id
            );
            // Batch discipline: the worker must ONLY touch the declared
            // scope — the repo's AGENTS.md would otherwise make it run
            // checkpoint.sh (mutating memory/) and the containment
            // check would (correctly) fail the batch.
            let goal = format!(
                "{}\n\nBATCH CONSTRAINT (binding): modify ONLY files under the declared scope. Do NOT run checkpoint.sh. Do NOT touch memory/, evals/, scripts/, tickets/, docs/adr/, codex.log, progress.md, run.json, REPORT.md. Do NOT commit anything. The supervised verifier is the gate.",
                ticket.goal
            );
            let mut cmd = std::process::Command::new(&exe);
            cmd.args([
                "loop",
                "run",
                &goal,
                "--workdir",
                &wt.to_string_lossy(),
                "--verify",
                &ticket.verify,
                "--target",
                &target.to_string_lossy(),
                "--iterate",
                &iterate.to_string(),
                "--detach",
            ]);
            if let Some(w) = wall_cap {
                cmd.args(["--max-wall", &w.to_string()]);
            }
            if no_sandbox {
                cmd.arg("--no-sandbox");
            }
            let status = cmd
                .env("MINIAGI_SESSION_TAG", &tag)
                .stdin(std::process::Stdio::null())
                .status()
                .map_err(|e| format!("cannot launch ticket {}: {e}", ticket.id))?;
            if !status.success() {
                return Err(format!("loop run --detach failed for ticket {}", ticket.id));
            }
            active.push((ticket.id.clone(), wt.join(".supervisor"), wt.clone()));
            Ok(())
        };
    // Admission: fill up to max_parallel, then poll; a finished ticket
    // is recorded and the next queued ticket launches.
    while !queue.is_empty() || !active.is_empty() {
        while !queue.is_empty() && active.len() < max_parallel {
            let ticket = queue.remove(0);
            launch(ticket, &mut active)?;
        }
        if active.is_empty() {
            break;
        }
        if std::time::Instant::now() >= batch_deadline {
            for (id, handle, wt) in &active {
                results.push(BatchTicketResult {
                    id: id.clone(),
                    handle: handle.clone(),
                    worktree: wt.clone(),
                    passed: false,
                    report: None,
                });
            }
            return Err(
                "batch aggregate deadline exceeded — tickets failed (evidence preserved)"
                    .to_string(),
            );
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
        let mut done: Vec<usize> = Vec::new();
        for (i, (id, handle, wt)) in active.iter().enumerate() {
            let st = crate::bg::run_status(handle);
            if !st.alive {
                // D6: a dead ticket WITHOUT a report is a crash, not a
                // verdict — the dispatcher respawns it (bounded,
                // MAST-classified FM-3.1 premature termination, never
                // silent). A dead ticket WITH a report is a normal
                // finish and is recorded as today.
                let report_text = crate::bg::run_report_text(handle);
                if is_crash(st.report_ready, report_text.as_deref()) {
                    let Some(ticket) = manifest.tickets.iter().find(|t| t.id == *id) else {
                        results.push(BatchTicketResult {
                            id: id.clone(),
                            handle: handle.clone(),
                            worktree: wt.clone(),
                            passed: false,
                            report: None,
                        });
                        done.push(i);
                        continue;
                    };
                    let n = respawns.entry(id.clone()).or_default();
                    if respawn_allowed(*n) {
                        *n += 1;
                        eprintln!(
                            "  [respawn] ticket {id} crashed without a report (FM-3.1                              premature termination), relaunching ({n}/{MAX_TICKET_RESPAWNS})"
                        );
                        respawn_log.push(format!(
                            "{id}: respawned {n}x (FM-3.1 premature termination)"
                        ));
                        queue.insert(0, ticket);
                        done.push(i);
                        continue;
                    }
                    eprintln!("  [respawn] ticket {id} crashed {n}x — respawn budget exhausted");
                    results.push(BatchTicketResult {
                        id: id.clone(),
                        handle: handle.clone(),
                        worktree: wt.clone(),
                        passed: false,
                        report: None,
                    });
                    done.push(i);
                    continue;
                }
                let passed = report_text
                    .as_deref()
                    .is_some_and(|r| r.contains("final outcome: PASSED"));
                results.push(BatchTicketResult {
                    id: id.clone(),
                    handle: handle.clone(),
                    worktree: wt.clone(),
                    passed,
                    report: report_text
                        .map(std::path::PathBuf::from)
                        .or_else(|| st.report.as_ref().map(std::path::PathBuf::from)),
                });
                done.push(i);
            }
        }
        for i in done.into_iter().rev() {
            active.remove(i);
        }
    }
    Ok(BatchDispatchResult {
        results,
        respawns: respawn_log,
    })
}

/// Finalize + merge (S4): for each PASSING ticket the kernel commits
/// the worktree mechanically (git add -A + commit '<ticket-id>' — the
/// worker protocol never commits), verifies CONTAINMENT (the
/// committed changed-path set must be inside the declared scope), then
/// merges in deterministic id order (--no-ff). Any violation (out-of-
/// scope change, merge conflict, missing commit) fails the batch
/// ATOMICALLY — no partial merge, all evidence preserved.
#[derive(Debug)]
pub struct BatchMergeResult {
    /// The merge commit on the target branch.
    pub merge_sha: String,
    /// Tickets merged (id order).
    pub merged: Vec<String>,
}

/// Finalize + merge the passing tickets.
///
/// # Errors
///
/// Returns the first violation; the batch must be considered FAILED
/// (worktrees and branches remain as evidence).
pub fn finalize_and_merge(
    repo: &Path,
    manifest: &PlannerManifest,
    provision: &BatchProvision,
    results: &[BatchTicketResult],
) -> Result<BatchMergeResult, String> {
    let git = |args: &[&str]| -> Result<std::process::Output, String> {
        std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            // Kernel-authored mechanical commits: an explicit identity so
            // the batch works on hosts without a git user config (CI).
            .env("GIT_AUTHOR_NAME", "mini-agi")
            .env("GIT_AUTHOR_EMAIL", "kernel@mini-agi.local")
            .env("GIT_COMMITTER_NAME", "mini-agi")
            .env("GIT_COMMITTER_EMAIL", "kernel@mini-agi.local")
            .output()
            .map_err(|e| format!("git {args:?} failed: {e}"))
    };
    let mut merged: Vec<String> = Vec::new();
    for ticket in &manifest.tickets {
        let result = results
            .iter()
            .find(|r| r.id == ticket.id)
            .ok_or_else(|| format!("ticket {}: no dispatch result", ticket.id))?;
        if !result.passed {
            return Err(format!(
                "ticket {}: not passed — the batch fails atomically (evidence preserved)",
                ticket.id
            ));
        }
        // Kernel-owned mechanical commit in the worktree. The run
        // EVIDENCE files (the supervisor's reports/transcripts in the
        // worktree root) are excluded from the merge — they are
        // evidence, not repo content, and they sit outside every
        // ticket's declared scope.
        let wt = &result.worktree;
        let status = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(wt)
            .status()
            .map_err(|e| format!("git add in {} failed: {e}", wt.display()))?;
        if !status.success() {
            return Err(format!("git add failed in {}", wt.display()));
        }
        let status = std::process::Command::new("git")
            .args(["reset", "-q", "--"])
            .args(EVIDENCE_PATHS)
            .current_dir(wt)
            .status()
            .map_err(|e| format!("git reset in {} failed: {e}", wt.display()))?;
        if !status.success() {
            return Err(format!("git reset failed in {}", wt.display()));
        }
        // The kernel-owned commit is OPTIONAL: a worker that ran the
        // repo's checkpoint discipline already committed its changes on
        // the branch. Commit only when there is an unstaged diff.
        let staged = std::process::Command::new("git")
            .args(["diff", "--cached", "--quiet"])
            .current_dir(wt)
            .status()
            .map_err(|e| format!("git diff in {} failed: {e}", wt.display()))?;
        if !staged.success() {
            let commit = std::process::Command::new("git")
                .args(["commit", "-qm", &format!("batch: {}", ticket.id)])
                .current_dir(wt)
                .env("GIT_AUTHOR_NAME", "mini-agi")
                .env("GIT_AUTHOR_EMAIL", "kernel@mini-agi.local")
                .env("GIT_COMMITTER_NAME", "mini-agi")
                .env("GIT_COMMITTER_EMAIL", "kernel@mini-agi.local")
                .status()
                .map_err(|e| format!("git commit in {} failed: {e}", wt.display()))?;
            if !commit.success() {
                return Err(format!(
                    "ticket {}: worktree commit failed (nothing committed?)",
                    ticket.id
                ));
            }
        }
        // Containment: the committed diff vs the base must be inside
        // the declared scope. The diff runs INSIDE the worktree — the
        // worktree's own HEAD is not in the main repo's history yet.
        let changed = std::process::Command::new("git")
            .args(["diff", "--name-only", &provision.base_sha, "HEAD"])
            .current_dir(wt)
            .output()
            .map_err(|e| format!("containment diff failed: {e}"))?;
        let changed = String::from_utf8_lossy(&changed.stdout).to_string();
        for path in changed.lines() {
            if EVIDENCE_PATHS
                .iter()
                .any(|e| path == *e || path.starts_with(&format!("{e}/")))
            {
                continue;
            }
            let in_scope = ticket
                .scope
                .iter()
                .any(|s| path == s || path.starts_with(&format!("{s}/")));
            if !in_scope {
                return Err(format!(
                    "ticket {}: CONTAINMENT VIOLATION — changed '{}' outside the declared scope",
                    ticket.id, path
                ));
            }
        }
        merged.push(ticket.id.clone());
    }
    // ATOMIC merge (F1): assemble the batch on a SCRATCH branch; only
    // when every ticket merged cleanly does the target branch move
    // (fast-forward). A conflict aborts the scratch — the target is
    // never left half-merged.
    let short = &provision.base_sha[..provision.base_sha.len().min(10)];
    let scratch = format!("batch/{short}/merge");
    let target_branch = git(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    let target_branch = String::from_utf8_lossy(&target_branch.stdout)
        .trim()
        .to_string();
    let co = git(&["checkout", "-q", "-b", &scratch, &provision.base_sha])?;
    if !co.status.success() {
        // A pre-existing scratch branch would leave the merges running
        // on the TARGET branch (git stays put on a failed checkout) —
        // refuse cleanly: the target stays untouched.
        return Err(format!(
            "scratch branch {scratch} already exists — the batch cannot assemble atomically; remove the residue (evidence preserved)"
        ));
    }
    for ticket in &manifest.tickets {
        let branch = format!("batch/{short}/{}", ticket.id);
        let merge = git(&[
            "merge",
            "--no-ff",
            "-m",
            &format!("batch: merge {}", ticket.id),
            &branch,
        ])?;
        if !merge.status.success() {
            let _ = git(&["merge", "--abort"]);
            let _ = git(&["checkout", "-q", &target_branch]);
            let _ = git(&["branch", "-D", &scratch]);
            return Err(format!(
                "ticket {}: merge CONFLICT — the batch fails atomically (the target branch is untouched; evidence preserved)",
                ticket.id
            ));
        }
    }
    git(&["checkout", "-q", &target_branch])?;
    let ff = git(&["merge", "--ff-only", &scratch])?;
    if !ff.status.success() {
        return Err(
            "batch merge fast-forward FAILED — the target branch is untouched (evidence preserved)"
                .to_string(),
        );
    }
    let _ = git(&["branch", "-D", &scratch]);
    let out = git(&["rev-parse", "HEAD"])?;
    Ok(BatchMergeResult {
        merge_sha: String::from_utf8_lossy(&out.stdout).trim().to_string(),
        merged,
    })
}

/// Protected final gate (S5): snapshot the gate inputs (the protected
/// paths) at the base sha; the merged tree must NOT have drifted on
/// them — otherwise the final truth could be self-modified.
pub fn protected_paths_unchanged(repo: &Path, base_sha: &str) -> Result<bool, String> {
    // Committed drift vs the base.
    let out = std::process::Command::new("git")
        .args(["diff", "--name-only", base_sha, "HEAD", "--"])
        .args(PROTECTED_PATHS)
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    let committed = String::from_utf8_lossy(&out.stdout).to_string();
    // Dirty/index drift (the final verifier reads the working tree).
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain", "--"])
        .args(PROTECTED_PATHS)
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    let dirty = String::from_utf8_lossy(&dirty.stdout).to_string();
    Ok(committed.trim().is_empty() && dirty.trim().is_empty())
}

/// The planner pass: an INDEPENDENT read-only codex session decomposes
/// the goal into tickets and emits a strict JSON manifest. The output
/// may carry prose/fences around the JSON — the manifest is extracted
/// between the first '{' and the last '}' and parsed FAIL-CLOSED.
pub fn run_planner_pass(
    goal: &str,
    workdir: &Path,
    no_sandbox: bool,
) -> Result<PlannerManifest, String> {
    let prompt = format!(
        "You are the PLANNER for a parallel verified-iteration batch. Decompose the goal into 2-4 independent tickets with DISJOINT file scopes (never overlapping paths; never touch scripts/verify.sh, scripts/gate-lib.sh, evals, memory, tickets, docs/adr). Each ticket's verify command must be a deterministic gate that RUNS FROM THE TICKET WORKTREE ROOT (relative paths only — no absolute paths, no ~, no ..) and must FAIL on an empty directory. Emit ONLY a JSON object, no prose, no markdown fences:\n{{\"version\":1,\"tickets\":[{{\"id\":\"t1\",\"goal\":\"...\",\"scope\":[\"relative/path\"],\"verify\":\"command\",\"verify_target\":\".\"}}]}}\n\nGOAL: {goal}",
    );
    let idle_cap = mini_agi_core::config::Config::load(workdir).max_idle_seconds;
    let planner = crate::worker::run_worker_sandboxed(
        "codex",
        workdir,
        no_sandbox,
        true,
        Some(600),
        idle_cap,
        &["exec", "-s", "read-only", "--skip-git-repo-check", &prompt],
    )
    .map_err(|e| format!("planner pass not available: {e}"))?;
    let out = planner.output;
    // Extract the FIRST balanced JSON object (brace-depth scan): the
    // planner's output may carry a summary + a second echo of the JSON
    // after it, and first-{-last-} would swallow both copies.
    let start = out
        .find('{')
        .ok_or_else(|| "planner emitted no JSON object".to_string())?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut end = None;
    for (i, c) in out[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| "planner emitted no balanced JSON object".to_string())?;
    parse_manifest(&out[start..=end])
}

/// Remove a provisioned batch's worktrees (evidence preserved — the
/// worktree content stays on the branch).
pub fn teardown_batch(repo: &Path, provision: &BatchProvision) {
    for wt in &provision.worktrees {
        let _ = std::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(wt)
            .current_dir(repo)
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_classification_and_respawn_budget() {
        // D6 contract: dead + no report = crash; dead + report = verdict.
        assert!(is_crash(false, None));
        assert!(!is_crash(true, None));
        assert!(!is_crash(false, Some("final outcome: PASSED")));
        assert!(!is_crash(true, Some("final outcome: FAILED")));
        // Bounded: 2 respawns then the budget gives up.
        assert!(respawn_allowed(0));
        assert!(respawn_allowed(1));
        assert!(!respawn_allowed(2));
        assert!(!respawn_allowed(5));
    }

    const VALID: &str = r#"{
        "version": 1,
        "tickets": [
            {"id": "t1", "goal": "g1", "scope": ["crates/a"], "verify": "cargo test -p a"},
            {"id": "t2", "goal": "g2", "scope": ["crates/b"], "verify": "cargo test -p b", "verify_target": "."}
        ]
    }"#;

    fn expect_ok(text: &str) -> PlannerManifest {
        match parse_manifest(text) {
            Ok(m) => m,
            Err(e) => panic!("expected a valid manifest, got: {e}"),
        }
    }

    fn expect_err(text: &str, needle: &str) {
        match parse_manifest(text) {
            Ok(_) => panic!("expected a violation containing '{needle}'"),
            Err(e) => assert!(e.contains(needle), "expected '{needle}' in error, got: {e}"),
        }
    }

    #[test]
    fn valid_manifest_parses() {
        let m = expect_ok(VALID);
        assert_eq!(m.version, 1);
        assert_eq!(m.tickets.len(), 2);
        assert_eq!(m.tickets[0].verify_target, ".");
    }

    #[test]
    fn unknown_fields_fail_closed() {
        expect_err(
            r#"{"version":1,"tickets":[],"extra":1}"#,
            "unknown field `extra`",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","goal":"g","scope":["a"],"verify":"x","bogus":1}]}"#,
            "unknown field `bogus`",
        );
    }

    #[test]
    fn duplicate_json_keys_fail_closed() {
        expect_err(
            r#"{"version":1,"version":2,"tickets":[{"id":"t","goal":"g","scope":["a"],"verify":"x"}]}"#,
            "duplicate field `version`",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","id":"t2","goal":"g","scope":["a"],"verify":"x"}]}"#,
            "duplicate field `id`",
        );
    }

    #[test]
    fn version_and_count_gates() {
        expect_err(
            r#"{"version":2,"tickets":[{"id":"t","goal":"g","scope":["a"],"verify":"x"}]}"#,
            "version 2",
        );
        expect_err(r#"{"version":1,"tickets":[]}"#, "must not be empty");
    }

    #[test]
    fn duplicate_and_empty_tickets_fail() {
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","goal":"g","scope":["a"],"verify":"x"},{"id":"t","goal":"g","scope":["b"],"verify":"x"}]}"#,
            "duplicate ticket id",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"","goal":"g","scope":["a"],"verify":"x"}]}"#,
            "id must not be empty",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","goal":"","scope":["a"],"verify":"x"}]}"#,
            "goal must not be empty",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","goal":"g","scope":[],"verify":"x"}]}"#,
            "scope must not be empty",
        );
    }

    #[test]
    fn unsafe_paths_fail_closed() {
        for bad in ["/abs", "~/home", "../up", "a/../../up", "a:b"] {
            let text = format!(
                r#"{{"version":1,"tickets":[{{"id":"t","goal":"g","scope":["{bad}"],"verify":"x"}}]}}"#
            );
            expect_err(&text, "safe repo-relative");
        }
    }

    #[test]
    fn protected_paths_are_refused() {
        for p in [
            "scripts/verify.sh",
            "evals",
            "evals/cases/x",
            "memory",
            "tickets",
        ] {
            let text = format!(
                r#"{{"version":1,"tickets":[{{"id":"t","goal":"g","scope":["{p}"],"verify":"x"}}]}}"#
            );
            expect_err(&text, "protected");
        }
    }

    #[test]
    fn overlapping_scopes_fail_before_dispatch() {
        expect_err(
            r#"{"version":1,"tickets":[{"id":"a","goal":"g","scope":["crates/x"],"verify":"x"},{"id":"b","goal":"g","scope":["crates/x/src"],"verify":"x"}]}"#,
            "overlapping scopes",
        );
    }

    #[test]
    fn verifier_shape_allowlist() {
        assert!(valid_verifier_shape("cargo test"));
        assert!(valid_verifier_shape("sh -c 'python3 tests/run.py'"));
        assert!(!valid_verifier_shape(""));
        assert!(!valid_verifier_shape("/usr/bin/make verify"));
        assert!(!valid_verifier_shape("make -C /abs verify"));
        assert!(!valid_verifier_shape("python3 ~/scripts/check.py"));
        assert!(!valid_verifier_shape("sh ../../evil.sh"));
        assert!(!valid_verifier_shape("sh scripts/verify.sh"));
    }

    #[test]
    fn provision_creates_worktrees_and_preflights_verifiers() {
        // A real fixture repo: two files, two tickets with disjoint
        // scopes and non-vacuous verifiers (test -f passes in the
        // worktree, fails in an empty dir).
        let root = std::env::temp_dir().join(format!("mag-pl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for f in ["a.txt", "b.txt"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        let git = |args: &[&str]| {
            let st = std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .env("GIT_AUTHOR_NAME", "mini-agi tests")
                .env("GIT_AUTHOR_EMAIL", "tests@mini-agi.local")
                .env("GIT_COMMITTER_NAME", "mini-agi tests")
                .env("GIT_COMMITTER_EMAIL", "tests@mini-agi.local")
                .status()
                .unwrap();
            assert!(st.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "-b", "master"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "base"]);
        let base = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap();
        let base_sha = String::from_utf8(base.stdout).unwrap().trim().to_string();
        let manifest = PlannerManifest {
            version: 1,
            tickets: vec![
                PlannerTicket {
                    id: "t1".into(),
                    goal: "g1".into(),
                    scope: vec!["a.txt".into()],
                    verify: "sh -c 'test -f a.txt'".into(),
                    verify_target: ".".into(),
                },
                PlannerTicket {
                    id: "t2".into(),
                    goal: "g2".into(),
                    scope: vec!["b.txt".into()],
                    verify: "sh -c 'test -f b.txt'".into(),
                    verify_target: ".".into(),
                },
            ],
        };
        let provision = provision_batch(&root, &manifest, &base_sha).unwrap();
        assert_eq!(provision.worktrees.len(), 2);
        assert!(provision.worktrees[0].join("a.txt").is_file());
        assert!(provision.worktrees[1].join("b.txt").is_file());
        // A second provision must fail (worktrees exist).
        assert!(provision_batch(&root, &manifest, &base_sha).is_err());
        teardown_batch(&root, &provision);
        assert!(!provision.worktrees[0].exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn provision_fails_closed_on_vacuous_verifier() {
        let root = std::env::temp_dir().join(format!("mag-plv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "x").unwrap();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(&root)
                    .env("GIT_AUTHOR_NAME", "mini-agi tests")
                    .env("GIT_AUTHOR_EMAIL", "tests@mini-agi.local")
                    .env("GIT_COMMITTER_NAME", "mini-agi tests")
                    .env("GIT_COMMITTER_EMAIL", "tests@mini-agi.local")
                    .status()
                    .unwrap()
                    .success()
            );
        };
        git(&["init", "-q", "-b", "master"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "base"]);
        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap();
        let base_sha = String::from_utf8(out.stdout).unwrap().trim().to_string();
        let manifest = PlannerManifest {
            version: 1,
            tickets: vec![PlannerTicket {
                id: "t1".into(),
                goal: "g1".into(),
                scope: vec!["a.txt".into()],
                verify: "true".into(),
                verify_target: ".".into(),
            }],
        };
        let err = provision_batch(&root, &manifest, &base_sha).unwrap_err();
        assert!(err.contains("VACUOUS"), "{err}");
        assert!(
            !root.join(".batch/t1").exists(),
            "the vacuous ticket must not leave a worktree"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Hermetic git runner: CI runners have no user.name/email — the
    /// identity is injected via env so fixture commits never depend on
    /// a host git config.
    fn git_run(dir: &Path, args: &[&str]) -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "mini-agi tests")
            .env("GIT_AUTHOR_EMAIL", "tests@mini-agi.local")
            .env("GIT_COMMITTER_NAME", "mini-agi tests")
            .env("GIT_COMMITTER_EMAIL", "tests@mini-agi.local")
            .status()
            .is_ok_and(|s| s.success())
    }

    fn fixture_repo(tag: &str) -> (std::path::PathBuf, String) {
        let root = std::env::temp_dir().join(format!("mag-plm-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for f in ["a.txt", "b.txt", "scripts/verify.sh", "evals/x.txt"] {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        let git = |args: &[&str]| {
            assert!(git_run(&root, args), "git {args:?}");
        };
        git(&["init", "-q", "-b", "master"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "base"]);
        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap();
        (
            root,
            String::from_utf8(out.stdout).unwrap().trim().to_string(),
        )
    }

    #[test]
    fn finalize_commits_merges_and_checks_containment() {
        let (root, base) = fixture_repo("ok");
        let manifest = PlannerManifest {
            version: 1,
            tickets: vec![PlannerTicket {
                id: "t1".into(),
                goal: "g1".into(),
                scope: vec!["a.txt".into()],
                verify: "sh -c 'test -f a.txt'".into(),
                verify_target: ".".into(),
            }],
        };
        let provision = provision_batch(&root, &manifest, &base).unwrap();
        // Simulate a passing worker: modify a.txt in the worktree.
        std::fs::write(provision.worktrees[0].join("a.txt"), "changed").unwrap();
        let results = vec![BatchTicketResult {
            id: "t1".into(),
            handle: provision.worktrees[0].join(".supervisor"),
            worktree: provision.worktrees[0].clone(),
            passed: true,
            report: None,
        }];
        let merged = finalize_and_merge(&root, &manifest, &provision, &results).unwrap();
        assert_eq!(merged.merged, vec!["t1".to_string()]);
        assert!(root.join("a.txt").is_file());
        assert!(protected_paths_unchanged(&root, &base).unwrap());
        // The protected paths must not have drifted.
        std::fs::write(root.join("scripts/verify.sh"), "tampered").unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-qm", "tamper"])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(!protected_paths_unchanged(&root, &base).unwrap());
        teardown_batch(&root, &provision);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn containment_violation_fails_the_batch() {
        let (root, base) = fixture_repo("viol");
        let manifest = PlannerManifest {
            version: 1,
            tickets: vec![PlannerTicket {
                id: "t1".into(),
                goal: "g1".into(),
                scope: vec!["a.txt".into()],
                verify: "sh -c 'test -f a.txt'".into(),
                verify_target: ".".into(),
            }],
        };
        let provision = provision_batch(&root, &manifest, &base).unwrap();
        // The worker touched a file OUTSIDE the declared scope.
        std::fs::write(provision.worktrees[0].join("b.txt"), "sneaky").unwrap();
        let results = vec![BatchTicketResult {
            id: "t1".into(),
            handle: provision.worktrees[0].join(".supervisor"),
            worktree: provision.worktrees[0].clone(),
            passed: true,
            report: None,
        }];
        let err = finalize_and_merge(&root, &manifest, &provision, &results).unwrap_err();
        assert!(err.contains("CONTAINMENT VIOLATION"), "{err}");
        teardown_batch(&root, &provision);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn not_passed_ticket_fails_atomically() {
        let (root, base) = fixture_repo("nopass");
        let manifest = PlannerManifest {
            version: 1,
            tickets: vec![PlannerTicket {
                id: "t1".into(),
                goal: "g1".into(),
                scope: vec!["a.txt".into()],
                verify: "sh -c 'test -f a.txt'".into(),
                verify_target: ".".into(),
            }],
        };
        let provision = provision_batch(&root, &manifest, &base).unwrap();
        let results = vec![BatchTicketResult {
            id: "t1".into(),
            handle: provision.worktrees[0].join(".supervisor"),
            worktree: provision.worktrees[0].clone(),
            passed: false,
            report: None,
        }];
        let err = finalize_and_merge(&root, &manifest, &provision, &results).unwrap_err();
        assert!(err.contains("fails atomically"), "{err}");
        teardown_batch(&root, &provision);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pre_existing_scratch_branch_fails_cleanly() {
        // The scratch assembly must refuse when the scratch branch
        // already exists (residue) — otherwise the merges would run on
        // the target branch. Assert the target HEAD is unchanged.
        let (root, base) = fixture_repo("scratch-residue");
        let manifest = PlannerManifest {
            version: 1,
            tickets: vec![PlannerTicket {
                id: "t1".into(),
                goal: "g1".into(),
                scope: vec!["a.txt".into()],
                verify: "sh -c 'test -f a.txt'".into(),
                verify_target: ".".into(),
            }],
        };
        let provision = provision_batch(&root, &manifest, &base).unwrap();
        std::fs::write(provision.worktrees[0].join("a.txt"), "changed").unwrap();
        let results = vec![BatchTicketResult {
            id: "t1".into(),
            handle: provision.worktrees[0].join(".supervisor"),
            worktree: provision.worktrees[0].clone(),
            passed: true,
            report: None,
        }];
        // Pre-create the scratch branch (residue from a previous batch).
        let short = &base[..10];
        assert!(
            std::process::Command::new("git")
                .args(["branch", &format!("batch/{short}/merge")])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        let head_before = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap();
        let head_before = String::from_utf8_lossy(&head_before.stdout)
            .trim()
            .to_string();
        let err = finalize_and_merge(&root, &manifest, &provision, &results).unwrap_err();
        assert!(err.contains("scratch branch"), "{err}");
        let head_after = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap();
        let head_after = String::from_utf8_lossy(&head_after.stdout)
            .trim()
            .to_string();
        assert_eq!(head_before, head_after, "the target must stay untouched");
        teardown_batch(&root, &provision);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ticket_worktree_is_scoped_under_batch() {
        let wt = ticket_worktree(Path::new("/repo"), "t1");
        assert_eq!(wt, Path::new("/repo/.batch/t1"));
    }
}
