//! mini-agi — single-binary agent kernel: CLI + MCP server shell.
//!
//! Phase 0 CLI: memory consolidate/signoff, derive, provenance. Ports `PoC`
//! (`scripts/consolidate.py`, `scripts/derive.py`) stdout + exit codes 1:1
//! (behavioral contract, tag `v1-spec-reference`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use mini_agi_core::contract;
use mini_agi_core::eval::{self, EvalError};
use mini_agi_core::journal;
use mini_agi_core::memory::{self, ConsolidateOptions, ENTRIES_REL, MemoryError};
use mini_agi_core::metrics;
use mini_agi_core::skills;

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
    /// Four-dimensional eval scoring + regression gate (`PoC` harness).
    Eval(EvalArgs),
    /// Verifiable skills registry (`.agents/skills/`, ADR-0002).
    Skill(SkillArgs),
    /// Checkpoint journal audit (T008 semantics).
    Checkpoint(CheckpointArgs),
    /// Typed handoff contract validation (ADR-0007).
    Validate(ValidateArgs),
    /// Canonical-memory inventory by domain (port of `PoC` stats.py).
    Stats,
    /// Context budget report (port of `PoC` budget.py).
    Budget,
}

#[derive(Args, Debug)]
struct ValidateArgs {
    /// Contract name: eval-run, ticket, spec, or verdict.
    contract: String,
    /// JSON document file to validate.
    document: PathBuf,
}

#[derive(Args, Debug)]
struct CheckpointArgs {
    #[command(subcommand)]
    action: CheckpointAction,
}

#[derive(Subcommand, Debug)]
enum CheckpointAction {
    /// Completeness audit of memory/episodic/checkpoints.log; exit
    /// non-zero when a VERIFY has no earlier BEGIN since the gate boundary.
    Audit,
}

#[derive(Args, Debug)]
struct SkillArgs {
    #[command(subcommand)]
    action: SkillAction,
}

#[derive(Subcommand, Debug)]
enum SkillAction {
    /// List all discovered skills with their verify hooks.
    List,
    /// Show one skill's frontmatter summary.
    Show {
        /// Skill name.
        name: String,
    },
    /// Run a skill's verify hook; exit non-zero on failure.
    Verify {
        /// Skill name.
        name: String,
    },
    /// Install skills from a git source (repo with `.agents/skills/`, or a
    /// repo that is itself a skill).
    Add {
        /// Git URL, `owner/repo` GitHub shorthand, or local path.
        source: String,
    },
}

#[derive(Args, Debug)]
struct EvalArgs {
    #[command(subcommand)]
    action: EvalAction,
}

#[derive(Subcommand, Debug)]
enum EvalAction {
    /// Score one `run.json` (`PoC` `score.py`; report JSON on stdout).
    Score {
        /// Path to the run file (evals/cases/<case>/run.json).
        run: PathBuf,
    },
    /// Regression gate over all cases vs the committed baseline.
    Gate {
        /// Max allowed composite drop.
        #[arg(long, default_value_t = 0.05)]
        tolerance: f64,
        /// Snapshot current results as the new baseline.
        #[arg(long)]
        write_baseline: bool,
    },
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
        Command::Eval(EvalArgs { action }) => match action {
            EvalAction::Score { run } => cmd_eval_score(&run),
            EvalAction::Gate {
                tolerance,
                write_baseline,
            } => cmd_eval_gate(tolerance, write_baseline),
        },
        Command::Skill(SkillArgs { action }) => match action {
            SkillAction::List => cmd_skill_list(),
            SkillAction::Show { name } => cmd_skill_show(&name),
            SkillAction::Verify { name } => cmd_skill_verify(&name),
            SkillAction::Add { source } => cmd_skill_add(&source),
        },
        Command::Checkpoint(CheckpointArgs { action }) => match action {
            CheckpointAction::Audit => cmd_checkpoint_audit(),
        },
        Command::Validate(ValidateArgs { contract, document }) => {
            cmd_validate(&contract, &document)
        }
        Command::Stats => cmd_stats(),
        Command::Budget => cmd_budget(),
    }
}

fn cmd_stats() -> ExitCode {
    let root = root();
    match metrics::stats(&root) {
        Ok(report) => {
            println!("canonical entries: {}", report.entries);
            println!("canonical facts: {}", report.facts);
            println!("derived views: {}", report.derived_views);
            for (domain, count) in &report.per_domain {
                if *count > 0 {
                    println!("{domain}: {count}");
                }
            }
            println!("gate: PASS");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot compute stats: {e}")),
    }
}

fn cmd_budget() -> ExitCode {
    let report = metrics::budget(&root());
    println!("CONTEXT BUDGET REPORT");
    println!(
        "  AGENTS chain:    {}B ({}% of 32KiB cap)",
        report.agents_chain_bytes, report.chain_pct_of_32k
    );
    if report.chain_over_cap {
        println!("  WARN: AGENTS chain exceeds 32KiB cap");
    }
    println!(
        "  Skills list:     {}B for {} skills ({}% of 2% budget)",
        report.skills_list_bytes, report.skills_count, report.skills_pct_of_budget
    );
    if report.skills_over_budget {
        println!("  WARN: skills list exceeds 2% budget");
    }
    println!(
        "  Memory leverage: canonical {}B -> brief {}B (x{} compression into working set)",
        report.canonical_bytes, report.brief_bytes, report.leverage_ratio
    );
    ExitCode::SUCCESS
}

fn cmd_validate(contract_name: &str, document: &Path) -> ExitCode {
    let contract = match contract_name {
        "eval-run" => contract::Contract::EvalRun,
        "ticket" => contract::Contract::Ticket,
        "spec" => contract::Contract::Spec,
        "verdict" => contract::Contract::Verdict,
        other => {
            return fail(&format!(
                "unknown contract '{other}' (eval-run|ticket|spec|verdict)"
            ));
        }
    };
    let text = match std::fs::read_to_string(document) {
        Ok(text) => text,
        Err(e) => return fail(&format!("cannot read {}: {e}", document.display())),
    };
    let value = match contract::parse_document(&text) {
        Ok(value) => value,
        Err(e) => return fail(&format!("invalid JSON in {}: {e}", document.display())),
    };
    match contract::validate_contract_value(contract, &value) {
        Ok(()) => {
            println!(
                "ok: {} validates against {contract_name}",
                document.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => fail(&format!("{} does not validate: {err}", document.display())),
    }
}

fn cmd_checkpoint_audit() -> ExitCode {
    let root = root();
    let journal = root.join("memory").join("episodic").join("checkpoints.log");
    let text = match std::fs::read_to_string(&journal) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return fail("FAIL: journal missing: memory/episodic/checkpoints.log");
        }
        Err(e) => return fail(&format!("cannot read journal: {e}")),
    };
    let events = journal::parse_journal(&text);
    let mut failed = false;
    let v = journal::violations(&events, journal::GATE_SINCE);
    for h in &v.historical {
        println!("historical (pre-gate, not failing): {h}");
    }
    for b in &v.bad {
        println!("VIOLATION: {b}");
        failed = true;
    }
    let audit = journal::audit_journal(&events);
    for a in &audit.historical {
        println!("WARNING: historical anomaly: {}", a.message);
    }
    for a in &audit.bad {
        println!("ANOMALY (line {}): {}", a.line_no, a.message);
        failed = true;
    }
    if failed {
        return fail("FAIL: checkpoint cascade incomplete");
    }
    println!("ok: checkpoint cascade complete (every VERIFY has BEGIN)");
    ExitCode::SUCCESS
}

fn cmd_skill_add(source: &str) -> ExitCode {
    let root = root();
    match skills::install_skills(&root, source) {
        Ok(installed) => {
            for name in &installed {
                println!("installed: {name}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot install from '{source}': {e}")),
    }
}

fn cmd_skill_list() -> ExitCode {
    let root = root();
    match skills::discover_skills(&root) {
        Ok(reg) => {
            for skill in &reg {
                let hook = if skill.verify.is_some() {
                    "verify"
                } else {
                    "ref"
                };
                println!("{}  [{hook}]  {}", skill.name, skill.description);
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot discover skills: {e}")),
    }
}

fn cmd_skill_show(name: &str) -> ExitCode {
    let root = root();
    match skills::find_skill(&root, name) {
        Ok(skill) => {
            println!("name: {}", skill.name);
            println!("description: {}", skill.description);
            println!(
                "verify: {}",
                skill.verify.as_deref().unwrap_or("(none — reference only)")
            );
            println!(
                "disable-model-invocation: {}",
                skill.disable_model_invocation
            );
            if let Some(hint) = &skill.argument_hint {
                println!("argument-hint: {hint}");
            }
            println!("path: {}", skill.path.display());
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_skill_verify(name: &str) -> ExitCode {
    let root = root();
    match skills::find_skill(&root, name) {
        Ok(skill) => match skills::verify_skill(&skill, &root) {
            Ok(result) => {
                if result.passed {
                    println!("PASS: {name}");
                    ExitCode::SUCCESS
                } else {
                    eprintln!("FAIL: {name} (exit {:?})", result.exit_code);
                    eprintln!("{}", result.output);
                    ExitCode::from(1)
                }
            }
            Err(e) => fail(&e.to_string()),
        },
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_eval_score(run: &Path) -> ExitCode {
    let root = root();
    match eval::score_run(run, &root, &root.join("evals/golden")) {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
            ExitCode::SUCCESS
        }
        Err(EvalError::Read(e)) => fail(&format!("cannot read run file: {e}")),
        Err(EvalError::Json(e)) => fail(&format!("invalid run json: {e}")),
        Err(EvalError::InvalidField(f)) => fail(&format!("invalid run field '{f}'")),
        Err(EvalError::Metadata(m)) => fail(&m),
    }
}

fn cmd_eval_gate(tolerance: f64, write_baseline: bool) -> ExitCode {
    let root = root();
    let cases_dir = root.join("evals/cases");
    let golden_dir = root.join("evals/golden");
    let baseline_path = root.join("evals/results/baseline.json");
    let entries = match eval::score_all_cases(&cases_dir, &root, &golden_dir) {
        Ok(entries) => entries,
        Err(e) => return fail(&format!("eval gate: {e}")),
    };
    if entries.is_empty() {
        return fail("no eval cases found in evals/cases/");
    }
    if write_baseline || !baseline_path.exists() {
        if let Some(parent) = baseline_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&entries).unwrap();
        if std::fs::write(&baseline_path, json).is_err() {
            return fail("baseline write failed");
        }
        println!(
            "baseline written: {} ({} cases)",
            baseline_path.display(),
            entries.len()
        );
        return ExitCode::SUCCESS;
    }
    let Ok(text) = std::fs::read_to_string(&baseline_path) else {
        return fail("baseline unreadable");
    };
    let Ok(baseline) = serde_json::from_str::<Vec<eval::GateEntry>>(&text) else {
        return fail("baseline malformed");
    };
    let result = eval::run_gate(&entries, &baseline, tolerance);
    for message in &result.messages {
        println!("{message}");
    }
    let verdict = if result.failures == 0 { "PASS" } else { "FAIL" };
    println!(
        "{verdict}: {} cases, {} regressions",
        result.case_count, result.failures
    );
    if result.failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
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
