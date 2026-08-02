//! mini-agi — single-binary agent kernel: CLI + MCP server shell.
//!
//! Phase 0 CLI: memory consolidate/signoff, derive, provenance. Ports `PoC`
//! (`scripts/consolidate.py`, `scripts/derive.py`) stdout + exit codes 1:1
//! (behavioral contract, tag `v1-spec-reference`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use mini_agi_core::memory::{self, ConsolidateOptions, ENTRIES_REL, MemoryError};

/// Repository root: `AGENTIC_ROOT` env var, else current directory.
fn root() -> PathBuf {
    std::env::var_os("AGENTIC_ROOT").map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        PathBuf::from,
    )
}

/// Print an `error: ...` line (`PoC` contract) and exit 1.
fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::from(1)
}

#[derive(Parser, Debug)]
#[command(
    name = "mini-agi",
    version,
    about = "agent kernel: memory, evals, skills, orchestration"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Episodic-buffer -> canonical memory (port of `PoC` consolidate.py).
    Mem(MemArgs),
    /// Canonical -> derived views (port of `PoC` derive.py).
    Derive(DeriveArgs),
    /// Print the canonical fingerprint for the provenance gate.
    Provenance,
}

#[derive(Args, Debug)]
struct MemArgs {
    #[command(subcommand)]
    action: MemAction,
}

#[derive(Subcommand, Debug)]
enum MemAction {
    /// Consolidate an episodic buffer into canonical facts.
    Consolidate {
        /// Episodic buffer file (markdown).
        episodic: PathBuf,
        /// Domain assigned to new facts.
        #[arg(long, default_value = "general")]
        domain: String,
        /// Route wording-variants of known facts to the review queue.
        #[arg(long)]
        require_signoff: bool,
        /// Report without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Promote ONE contested fact from the queue into canonical.
    Signoff {
        /// Contested queue file (memory/review/contested-<date>.md).
        queue: PathBuf,
        /// 1-based fact index in the queue.
        index: usize,
        /// Domain assigned to the promoted fact.
        #[arg(long, default_value = "general")]
        domain: String,
    },
}

#[derive(Args, Debug)]
struct DeriveArgs {
    /// Skip per-domain fragment regeneration.
    #[arg(long)]
    brief_only: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Mem(MemArgs { action }) => match action {
            MemAction::Consolidate {
                episodic,
                domain,
                require_signoff,
                dry_run,
            } => cmd_consolidate(&episodic, &domain, require_signoff, dry_run),
            MemAction::Signoff {
                queue,
                index,
                domain,
            } => cmd_signoff(&queue, index, &domain),
        },
        Command::Derive(DeriveArgs { brief_only }) => cmd_derive(brief_only),
        Command::Provenance => {
            let root = root();
            println!("canonical_sha256: {}", memory::canonical_fingerprint(&root));
            ExitCode::SUCCESS
        }
    }
}

fn cmd_consolidate(
    episodic: &PathBuf,
    domain: &str,
    require_signoff: bool,
    dry_run: bool,
) -> ExitCode {
    let root = root();
    let Ok(text) = std::fs::read_to_string(episodic) else {
        return fail(&format!("{} not found", episodic.display()));
    };
    let source = episodic
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("buffer")
        .to_string();
    let opts = ConsolidateOptions {
        domain: domain.to_string(),
        require_signoff,
        dry_run,
    };
    match memory::consolidate(&root, &text, &source, &opts) {
        Ok(outcome) => {
            let entry_line = outcome.entry.as_ref().map(|entry| {
                let rel = entry.path.strip_prefix(&root).unwrap_or(&entry.path);
                format!("entry: {}", rel.display())
            });
            if dry_run {
                println!(
                    "dry-run: would write {} new facts (skipped {} duplicates)",
                    outcome.new_facts, outcome.skipped
                );
            } else {
                println!(
                    "consolidated {} new facts (skipped {} duplicates)",
                    outcome.new_facts, outcome.skipped
                );
            }
            if let Some(line) = entry_line {
                println!("{line}");
            }
            if outcome.new_facts > 0 && !dry_run {
                println!("next: make derive && make provenance");
            }
            ExitCode::SUCCESS
        }
        Err(MemoryError::NoFacts) => fail("no facts found in episodic buffer"),
        Err(MemoryError::Io(e)) => fail(&format!("entry write failed: {e}")),
        Err(_) => fail("unexpected memory error"),
    }
}

fn cmd_signoff(queue: &Path, index: usize, domain: &str) -> ExitCode {
    let root = root();
    match memory::signoff(&root, queue, index, domain) {
        Ok(entry) => {
            let rel = entry.path.strip_prefix(&root).unwrap_or(&entry.path);
            println!("signed off 1 fact");
            println!("entry: {}", rel.display());
            ExitCode::SUCCESS
        }
        Err(MemoryError::BadSignoff) => {
            fail("signoff requires an existing queue file and positive fact index")
        }
        Err(MemoryError::IndexNotFound) => fail("contested fact index not found"),
        Err(MemoryError::FactKnown) => fail("fact already known"),
        Err(MemoryError::Io(e)) => fail(&format!("entry write failed: {e}")),
        Err(_) => fail("unexpected memory error"),
    }
}

fn cmd_derive(brief_only: bool) -> ExitCode {
    let root = root();
    match memory::derive(&root, brief_only) {
        Ok((facts, fragments)) => {
            println!("derived: context-brief.md ({facts} facts)");
            println!("derived: {fragments} per-domain fragments");
            ExitCode::SUCCESS
        }
        Err(MemoryError::NoCanonical) => fail("no canonical facts yet — run ingest first"),
        Err(MemoryError::Io(e)) => fail(&format!("derive failed: {e}")),
        Err(_) => fail("unexpected memory error"),
    }
}

/// Re-export used in tests to assert entry layout.
#[allow(dead_code)]
const fn _entries_rel() -> &'static str {
    ENTRIES_REL
}
