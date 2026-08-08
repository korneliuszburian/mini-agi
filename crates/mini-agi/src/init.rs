//! `mini-agi init` — scaffold a repo so any agent plugs into the kernel.
//!
//! Creates the standard layout (memory/, .agents/skills/, scripts/,
//! evals/, tickets/, docs/adr/), embeds the gate scripts (verify.sh,
//! checkpoint.sh, hitl-loop.template.sh) copied from this binary's own
//! build, writes a lean AGENTS.md + CLAUDE.md shim + opencode.json MCP
//! config pointing at this binary. Idempotent: existing files are never
//! overwritten.

use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Scripts embedded from the mini-agi repo itself (same files the gate
/// runs here — the initialized repo gets exactly the tooling that keeps
/// this repo green).
const VERIFY_SH: &str = include_str!("../../../scripts/verify.sh");
const CHECKPOINT_SH: &str = include_str!("../../../scripts/checkpoint.sh");
const HITL_LOOP_TEMPLATE_SH: &str = include_str!("../../../scripts/hitl-loop.template.sh");

const AGENTS_MD: &str = r"# AGENTS.md — agent instructions (mini-agi powered)

This repo runs on the mini-agi kernel: enforcement-bound memory,
evaluation, verifiable skills, checkpoint journal. CLI: `mini-agi`.
Deterministic gate: `scripts/verify.sh` (fmt, clippy, tests, skills,
eval gate, checkpoint audit, provenance, mem-dedup/integrity, stats,
budget, insights, audit, sandbox) — a silent target is a failing target.

- Memory: `memory/canonical/` is the only hand-written source of truth
  (append-only, provenance on every entry). Derived views regenerate via
  `mini-agi derive`; on conflict canonical wins. Read
  `memory/derived/context-brief.md` before working; never re-research what
  canonical already knows. `mini-agi mem verify` audits memory integrity
  (duplicates, lineage, preservation); `mem supersede`/`preserve`/
  `unpreserve` evolve the lineage deliberately (never edit an entry).
- Checkpointing: `scripts/checkpoint.sh begin <label>` before every edit
  step, `scripts/checkpoint.sh verify <label>` after gates pass. The
  journal is audited by the gate (every VERIFY needs an earlier BEGIN).
- Skills: `.agents/skills/<name>/SKILL.md` (frontmatter: name, description,
  optional verify hook). `mini-agi skill list | verify | add`.
- Review: rubric in `.agents/checks/review-rubric.md`; verdicts must end
  with an `Anchors:` line of canonical fact ids (16-hex).
- Intelligence layer (run from a fresh session):
  `mini-agi resume` — what a new session must load (brief head, journal
  tail, failure and mismatch registers: do not repeat recorded failures).
  `mini-agi insights` — compounding report; capability gaps ARE the
  roadmap. `mini-agi loop status|dispatch|verify` — the gap -> ticket ->
  slice -> rerun loop without human routing (claims are leases: never
  start work on a ticket someone else holds). `mini-agi health` / `mini-agi
  audit` — machine and repo invariants before long sessions.
- MCP writes need HITL: every tool that changes the worker tree or
  canonical memory (`loop_dispatch`, `loop_objective`, `memory_signoff`,
  `memory_consolidate`, `memory_derive`, `run_ingest`, `ticket_claim`/
  `release`, `skill_add`, `harness`, `loop_run`) requires a non-empty
  `approve` reason in the kernel — a session cannot write without human
  signoff (`.codex/config.toml` mirrors this with `approval_mode = prompt`).
- Communication: facts and next actions, no filler.
";

/// The review rubric is embedded from the repo's canonical copy so a
/// fresh init ships the SAME rubric (posture + evidence requirements)
/// the gate enforces — never a hand-maintained stale duplicate.
const RUBRIC_MD: &str = include_str!("../../../.agents/checks/review-rubric.md");

const GITIGNORE: &str = "target/\n\
.krn/\n\
# kernel runtime artifacts (a fresh init produces these) — never commit\n\
.supervisor/\n\
.worker-*\n\
*.blind-hidden\n\
.batch/\n\
codex.log\n\
progress.md\n\
run.json\n\
memory/derived/snapshots/\n";

/// Codex onboarding: marking the repo trusted lets `codex exec` run
/// without `--skip-git-repo-check` (verified in EXP-001).
/// Full codex MCP registration (AFK-SUPERVISOR S4). Unlike the old
/// two-line template this wires the server with the explicit tool
/// allowlist and per-tool HITL approvals so a fresh init gets the same
/// write-gating the docs describe. `exe` is quoted as a basic TOML
/// string (backslashes escaped for Windows-style paths).
fn codex_config(exe: &Path) -> String {
    use std::fmt::Write as _;
    let exe = exe.to_string_lossy().replace('\\', "\\\\");
    let tools = [
        "audit",
        "backlog",
        "budget",
        "checkpoint_audit",
        "eval_gate",
        "eval_score",
        "eval_steps",
        "harness",
        "health",
        "insights",
        "loop_dispatch",
        "loop_objective",
        "loop_run",
        "loop_status",
        "loop_verify",
        "memory_consolidate",
        "memory_derive",
        "memory_query",
        "memory_signoff",
        "provenance",
        "resume",
        "run_failures",
        "run_ingest",
        "run_report",
        "run_status",
        "run_verify",
        "skill_add",
        "skill_list",
        "skill_show",
        "skill_verify",
        "stats",
        "ticket_claim",
        "ticket_claims",
        "ticket_graph",
        "ticket_list",
        "ticket_release",
        "ticket_show",
        "ticket_validate",
        "validate",
    ];
    let mut out = String::new();
    out.push_str(
        "# mini-agi as a codex MCP server (AFK-SUPERVISOR S4).\n\
         # The kernel is a first-class tool of every codex session in this repo:\n\
         # enforcement-bound memory, verified-iteration loop, provenance-bound\n\
         # results. See docs/CODEX-INTEGRATION.md.\n\
         [mcp_servers.mini-agi]\n\
         command = \"",
    );
    out.push_str(&exe);
    out.push_str("\"\nargs = [\"mcp\"]\ntrusted = true\ndefault_tools_approval_mode = \"auto\"\n");
    out.push_str("enabled_tools = [\n");
    for t in tools {
        let _ = writeln!(out, "  \"{t}\",");
    }
    out.push_str("]\n\n");
    out.push_str(
        "# Writes require a prompt (HITL). memory_signoff stays human by design\n\
         # (ADR-0010) — a session cannot silently merge its own memory.\n",
    );
    for t in [
        "harness",
        "loop_dispatch",
        "loop_run",
        "loop_objective",
        "memory_consolidate",
        "memory_derive",
        "memory_signoff",
        "run_ingest",
        "skill_add",
        "ticket_claim",
        "ticket_release",
    ] {
        let _ = writeln!(
            out,
            "\n[mcp_servers.mini-agi.tools.{t}]\napproval_mode = \"prompt\"\n"
        );
    }
    out
}

/// Directory skeleton created by `init`.
const DIRS: &[&str] = &[
    "memory/canonical/entries",
    "memory/episodic",
    "memory/derived/per-domain",
    "memory/review",
    ".agents/skills",
    ".agents/checks",
    "evals/cases",
    "evals/golden",
    "evals/results",
    "tickets",
    "scripts",
    "docs/adr",
    "knowledge/sources",
];

/// Scaffold a repo at `root`.
///
/// # Errors
///
/// Returns [`io::Error`] when a directory or file cannot be written.
/// Bootstrap the data-dir skeleton on first use (production-readiness
/// B.4 "single binary" story): create only the MISSING directories from
/// the seed layout — no files, no clobbering. Called best-effort at
/// startup so the binary works in an empty data dir; a no-op in a repo
/// that already has the layout.
///
/// # Errors
///
/// Returns the underlying I/O error when a directory cannot be created.
pub fn bootstrap(root: &Path) -> Result<Vec<String>, io::Error> {
    let mut created = Vec::new();
    for dir in DIRS {
        let path = root.join(dir);
        if path.exists() {
            continue;
        }
        fs::create_dir_all(&path)?;
        created.push(dir.to_string());
    }
    Ok(created)
}

pub fn init(root: &Path) -> Result<Vec<String>, io::Error> {
    let mut created = Vec::new();
    for dir in DIRS {
        fs::create_dir_all(root.join(dir))?;
    }
    created.push("directories".to_string());

    let files: Vec<(&str, &str, &str)> = vec![
        ("AGENTS.md", AGENTS_MD, "skip if exists"),
        (".gitignore", GITIGNORE, "skip if exists"),
        (
            ".agents/checks/review-rubric.md",
            RUBRIC_MD,
            "skip if exists",
        ),
        ("memory/episodic/checkpoints.log", "", "empty journal"),
        ("scripts/verify.sh", VERIFY_SH, "embedded"),
        ("scripts/checkpoint.sh", CHECKPOINT_SH, "embedded"),
        (
            "scripts/hitl-loop.template.sh",
            HITL_LOOP_TEMPLATE_SH,
            "embedded",
        ),
    ];
    for (rel, content, _note) in files {
        let path = root.join(rel);
        // symlink_metadata: a broken symlink still exists — do not clobber
        // it with a regular file.
        if fs::symlink_metadata(&path).is_ok() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        if rel.starts_with("scripts/") && Path::new(rel).extension().is_some_and(|e| e == "sh") {
            make_executable(&path)?;
        }
        created.push(rel.to_string());
    }

    let claude_path = root.join("CLAUDE.md");
    if !claude_path.exists() {
        fs::write(
            &claude_path,
            "# CLAUDE.md — import shim (generated by `mini-agi init`)\n\n\
             Canonical agent instructions: AGENTS.md. Context brief: \
             memory/derived/context-brief.md (regenerate with `mini-agi derive`).\n",
        )?;
        created.push("CLAUDE.md".to_string());
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| Path::new("mini-agi").to_path_buf());

    let codex_path = root.join(".codex/config.toml");
    if fs::symlink_metadata(&codex_path).is_err() {
        fs::create_dir_all(codex_path.parent().expect("parent dir"))?;
        fs::write(&codex_path, codex_config(&exe))?;
        created.push(".codex/config.toml".to_string());
    }

    let opencode_path = root.join("opencode.json");
    if fs::symlink_metadata(&opencode_path).is_err() {
        let config = serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "mcp": {
                "mini-agi": {
                    "type": "local",
                    "command": [exe.to_string_lossy(), "mcp"],
                    "enabled": true
                }
            }
        });
        fs::write(
            &opencode_path,
            serde_json::to_string_pretty(&config).map_err(io::Error::other)? + "\n",
        )?;
        created.push("opencode.json".to_string());
    }

    created.push(
        "ready: run scripts/verify.sh, then mini-agi health && mini-agi audit && mini-agi resume"
            .to_string(),
    );

    Ok(created)
}

/// Make a shell script executable (0o755 on unix).
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), io::Error> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
}

/// Non-unix: scripts are run via `sh`; nothing to do.
#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), io::Error> {
    Ok(())
}
