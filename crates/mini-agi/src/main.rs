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

mod init;
mod mcp;

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
    /// stdio MCP server (tools over JSON-RPC 2.0).
    Mcp,
    /// Scaffold a repo: layout, gate scripts, AGENTS.md, MCP config.
    Init,
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
        Command::Mcp => match mcp::run_stdio_server() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("mcp server error: {e}")),
        },
        Command::Init => cmd_init(),
    }
}

fn cmd_init() -> ExitCode {
    let root = root();
    match init::init(&root) {
        Ok(created) => {
            println!("initialized: {}", root.display());
            for item in &created {
                println!("  created: {item}");
            }
            println!("next: add facts (mini-agi mem consolidate), then mini-agi derive");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("init failed: {e}")),
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
    match validate_doc_text(contract_name, document) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(msg) => fail(&msg),
    }
}

fn validate_doc_text(contract_name: &str, document: &Path) -> Result<String, String> {
    let contract = match contract_name {
        "eval-run" => contract::Contract::EvalRun,
        "ticket" => contract::Contract::Ticket,
        "spec" => contract::Contract::Spec,
        "verdict" => contract::Contract::Verdict,
        other => {
            return Err(format!(
                "unknown contract '{other}' (eval-run|ticket|spec|verdict)"
            ));
        }
    };
    let text = std::fs::read_to_string(document)
        .map_err(|e| format!("cannot read {}: {e}", document.display()))?;
    let value = contract::parse_document(&text)
        .map_err(|e| format!("invalid JSON in {}: {e}", document.display()))?;
    match contract::validate_contract_value(contract, &value) {
        Ok(()) => Ok(format!(
            "ok: {} validates against {contract_name}",
            document.display()
        )),
        Err(err) => Err(format!("{} does not validate: {err}", document.display())),
    }
}

fn cmd_checkpoint_audit() -> ExitCode {
    match checkpoint_audit_text(&root()) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            println!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn checkpoint_audit_text(root: &Path) -> Result<String, String> {
    let journal = root.join("memory").join("episodic").join("checkpoints.log");
    let text = std::fs::read_to_string(&journal).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "FAIL: journal missing: memory/episodic/checkpoints.log".to_string()
        } else {
            format!("cannot read journal: {e}")
        }
    })?;
    let events = journal::parse_journal(&text);
    let mut lines = Vec::new();
    let mut failed = false;
    let v = journal::violations(&events, journal::GATE_SINCE);
    for h in &v.historical {
        lines.push(format!("historical (pre-gate, not failing): {h}"));
    }
    for b in &v.bad {
        lines.push(format!("VIOLATION: {b}"));
        failed = true;
    }
    let audit = journal::audit_journal(&events);
    for a in &audit.historical {
        lines.push(format!("WARNING: historical anomaly: {}", a.message));
    }
    for a in &audit.bad {
        lines.push(format!("ANOMALY (line {}): {}", a.line_no, a.message));
        failed = true;
    }
    if failed {
        return Err(format!(
            "{}\nFAIL: checkpoint cascade incomplete",
            lines.join("\n")
        ));
    }
    Ok(format!(
        "{}\nok: checkpoint cascade complete (every VERIFY has BEGIN)",
        lines.join("\n")
    ))
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
    match eval_gate_text(&root(), tolerance, write_baseline) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            println!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn eval_gate_text(root: &Path, tolerance: f64, write_baseline: bool) -> Result<String, String> {
    let cases_dir = root.join("evals/cases");
    let golden_dir = root.join("evals/golden");
    let baseline_path = root.join("evals/results/baseline.json");
    let entries = eval::score_all_cases(&cases_dir, root, &golden_dir)
        .map_err(|e| format!("eval gate: {e}"))?;
    if entries.is_empty() {
        // A fresh repo has nothing to regress: the gate passes trivially,
        // so an init'd repo is gate-green from the first commit.
        return Ok("PASS: 0 cases, 0 regressions — no cases in evals/cases/".to_string());
    }
    if write_baseline {
        if let Some(parent) = baseline_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&entries).unwrap();
        if std::fs::write(&baseline_path, json).is_err() {
            return Err("baseline write failed".to_string());
        }
        return Ok(format!(
            "baseline written: {} ({} cases)",
            baseline_path.display(),
            entries.len()
        ));
    }
    let text = std::fs::read_to_string(&baseline_path).map_err(|_| {
        "baseline missing: run `mini-agi eval gate --write-baseline` once — \
         the gate never re-baselines silently"
            .to_string()
    })?;
    let baseline: Vec<eval::GateEntry> =
        serde_json::from_str(&text).map_err(|_| "baseline malformed".to_string())?;
    let result = eval::run_gate(&entries, &baseline, tolerance);
    let mut lines = result.messages.clone();
    let verdict = if result.failures == 0 { "PASS" } else { "FAIL" };
    lines.push(format!(
        "{verdict}: {} cases, {} regressions",
        result.case_count, result.failures
    ));
    if result.failures == 0 {
        Ok(lines.join("\n"))
    } else {
        Err(lines.join("\n"))
    }
}

fn cmd_consolidate(
    episodic: &Path,
    domain: &str,
    require_signoff: bool,
    dry_run: bool,
) -> ExitCode {
    match consolidate_text(episodic, domain, require_signoff, dry_run, &root()) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(msg) => fail(&msg),
    }
}

fn consolidate_text(
    episodic: &Path,
    domain: &str,
    require_signoff: bool,
    dry_run: bool,
    root: &Path,
) -> Result<String, String> {
    let text = std::fs::read_to_string(episodic)
        .map_err(|_| format!("{} not found", episodic.display()))?;
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
    match memory::consolidate(root, &text, &source, &opts) {
        Ok(outcome) => {
            let mut lines = Vec::new();
            if let Some(entry) = &outcome.entry {
                let rel = entry.path.strip_prefix(root).unwrap_or(&entry.path);
                lines.push(format!("entry: {}", rel.display()));
            }
            if dry_run {
                lines.push(format!(
                    "dry-run: would write {} new facts (skipped {} duplicates)",
                    outcome.new_facts, outcome.skipped
                ));
            } else {
                lines.push(format!(
                    "consolidated {} new facts (skipped {} duplicates)",
                    outcome.new_facts, outcome.skipped
                ));
            }
            if outcome.new_facts > 0 && !dry_run {
                lines.push("next: mini-agi derive && mini-agi provenance".to_string());
            }
            Ok(lines.join("\n"))
        }
        Err(MemoryError::NoFacts) => Err("no facts found in episodic buffer".to_string()),
        Err(MemoryError::Io(e)) => Err(format!("entry write failed: {e}")),
        Err(_) => Err("unexpected memory error".to_string()),
    }
}

fn cmd_signoff(queue: &Path, index: usize, domain: &str) -> ExitCode {
    match signoff_text(queue, index, domain, &root()) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(msg) => fail(&msg),
    }
}

fn signoff_text(queue: &Path, index: usize, domain: &str, root: &Path) -> Result<String, String> {
    match memory::signoff(root, queue, index, domain) {
        Ok(entry) => {
            let rel = entry.path.strip_prefix(root).unwrap_or(&entry.path);
            Ok(format!("signed off 1 fact\nentry: {}", rel.display()))
        }
        Err(MemoryError::BadSignoff) => {
            Err("signoff requires an existing queue file and positive fact index".to_string())
        }
        Err(MemoryError::IndexNotFound) => Err("contested fact index not found".to_string()),
        Err(MemoryError::FactKnown) => Err("fact already known".to_string()),
        Err(MemoryError::Io(e)) => Err(format!("entry write failed: {e}")),
        Err(_) => Err("unexpected memory error".to_string()),
    }
}

fn cmd_derive(brief_only: bool) -> ExitCode {
    match derive_text(brief_only, &root()) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(msg) => fail(&msg),
    }
}

fn derive_text(brief_only: bool, root: &Path) -> Result<String, String> {
    match memory::derive(root, brief_only) {
        Ok((facts, fragments)) => Ok(format!(
            "derived: context-brief.md ({facts} facts)\nderived: {fragments} per-domain fragments"
        )),
        Err(MemoryError::NoCanonical) => {
            Err("no canonical facts yet — run ingest first".to_string())
        }
        Err(MemoryError::Io(e)) => Err(format!("derive failed: {e}")),
        Err(_) => Err("unexpected memory error".to_string()),
    }
}

/// Re-export used in tests to assert entry layout.
#[allow(dead_code)]
const fn _entries_rel() -> &'static str {
    ENTRIES_REL
}
