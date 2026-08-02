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
Deterministic gate: `scripts/verify.sh` (fmt, clippy, tests, eval gate,
checkpoint audit, provenance, stats, budget) — a silent target is a
failing target.

- Memory: `memory/canonical/` is the only hand-written source of truth
  (append-only, provenance on every entry). Derived views regenerate via
  `mini-agi derive`; on conflict canonical wins. Read
  `memory/derived/context-brief.md` before working; never re-research what
  canonical already knows.
- Checkpointing: `scripts/checkpoint.sh begin <label>` before every edit
  step, `scripts/checkpoint.sh verify <label>` after gates pass. The
  journal is audited by the gate (every VERIFY needs an earlier BEGIN).
- Skills: `.agents/skills/<name>/SKILL.md` (frontmatter: name, description,
  optional verify hook). `mini-agi skill list | verify | add`.
- Review: rubric in `.agents/checks/review-rubric.md`; verdicts must end
  with an `Anchors:` line of canonical fact ids (16-hex).
- Communication: facts and next actions, no filler.
";

const RUBRIC_MD: &str = r"# Review rubric

Evidence-first: cite the changed file and line, a reproducer, or verifier
output for every finding and score. Do not infer a pass from an unrun check.

Score each dimension from 0-2:

| Dimension | 0 | 1 | 2 |
| --- | --- | --- | --- |
| Correctness | Broken contract | Material concern | Contract satisfied |
| Security | Vulnerability | Unresolved risk | No material risk found |
| Tests | Missing or unconvincing | Partial coverage | Regression coverage and relevant gates |
| Scope | Unauthorised change | Minor drift | Ticket scope only |

Total the four scores (0-8): APPROVE >=7; FIX-MINOR 5-6; REWORK <5.

## Memory-anchor rule (ADR-0003)

A verdict must end with an `Anchors:` line listing the canonical fact ids
(16-hex, from `memory/canonical/index.md`) the review relies on. A verdict
with zero anchors fails the gate, whatever the score.
";

const GITIGNORE: &str = "target/\n";

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
        if path.exists() {
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

    let opencode_path = root.join("opencode.json");
    if !opencode_path.exists() {
        let exe = std::env::current_exe().unwrap_or_else(|_| Path::new("mini-agi").to_path_buf());
        let config = format!(
            "{{\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \"mcp\": {{\n    \"mini-agi\": {{\n      \"type\": \"local\",\n      \"command\": [\"{}\", \"mcp\"],\n      \"enabled\": true\n    }}\n  }}\n}}\n",
            exe.display()
        );
        fs::write(&opencode_path, config)?;
        created.push("opencode.json".to_string());
    }

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
