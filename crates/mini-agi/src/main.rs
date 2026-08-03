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
use mini_agi_core::insights;
use mini_agi_core::journal;
use mini_agi_core::memory::{self, ConsolidateOptions, ENTRIES_REL, MemoryError};
use mini_agi_core::metrics;
use mini_agi_core::skills;
use mini_agi_core::ticket;

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
    /// Ticket lifecycle: list, show, validate (ADR-0007 contracts).
    Ticket(TicketArgs),
    /// Runs compound into the world model (ADR-0005).
    Run(RunArgs),
    /// Compounding report: runs, memory, tickets, gaps.
    Insights,
    /// Failure signal -> roadmap: gaps become tickets (ADR-0005).
    Backlog,
    /// Resume block for a fresh session.
    Resume,
    /// Runtime observability: load, memory, process zoo, journal, claims.
    Health,
    /// Repo invariants: provenance drift, baseline freshness, tree state.
    Audit,
    /// Proactive composition loop (Phase 6.4): status/dispatch/verify.
    Loop(LoopArgs),
    /// Codex integration (Phase 8 slice 4, EXP-003): run codex on a
    /// slice spec, capture the transcript, emit a truthful run.json.
    Codex(CodexArgs),
    /// Harness evolution (Phase 8 slice 7): versioned spec + ledger.
    Harness,
}

#[derive(Args, Debug)]
struct CodexArgs {
    /// Slice spec path (artifacts/<ticket>/spec.md).
    spec: PathBuf,
    /// Scratch workdir for codex.
    workdir: PathBuf,
    /// Where to write the captured run.json (default: workdir/run.json).
    #[arg(long)]
    run_out: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct RunArgs {
    #[command(subcommand)]
    action: RunAction,
}

#[derive(Subcommand, Debug)]
enum RunAction {
    /// Ingest a scored run.json (+ optional retro) into canonical memory.
    Ingest {
        /// Path to the run file (evals/cases/<case>/run.json).
        run: PathBuf,
        /// Optional retro markdown (bullets become facts).
        #[arg(long)]
        retro: Option<PathBuf>,
    },
    /// Deterministically verify a run's outcome in its target repo
    /// (ADR-0011 verifiable reward layer).
    Verify {
        /// Path to the run file (evals/cases/<case>/run.json).
        run: PathBuf,
    },
    /// Register repeated failing actions from a run.json (Reflexion).
    Failures {
        /// Path to the run file (evals/cases/<case>/run.json).
        run: PathBuf,
    },
}

#[derive(Args, Debug)]
struct TicketArgs {
    #[command(subcommand)]
    action: TicketAction,
}

#[derive(Subcommand, Debug)]
enum TicketAction {
    /// List all tickets in `tickets/`.
    List,
    /// Show one ticket (by `TICKET-<n>` or number).
    Show {
        /// Ticket id.
        id: String,
    },
    /// Validate one ticket against the ADR-0007 contract.
    Validate {
        /// Ticket id.
        id: String,
    },
    /// Validate the dependency graph (edges resolve, no cycles).
    ValidateGraph,
    /// Print the dependency graph (ADR-0008 work graph).
    Graph,
    /// Claim a ticket (lease; fails if blocked by an open ticket).
    Claim {
        /// Ticket id.
        id: String,
        /// Claimant name (default: current user).
        #[arg(long, default_value = "local")]
        claimant: String,
        /// Claim even with open `blocked_by` deps.
        #[arg(long)]
        force: bool,
    },
    /// Release a claim (only the holder can release).
    Release {
        /// Ticket id.
        id: String,
        /// Claimant name.
        #[arg(long, default_value = "local")]
        claimant: String,
    },
    /// List all held claims.
    Claims,
}

#[derive(Args, Debug)]
struct ValidateArgs {
    /// Contract name: eval-run, ticket, spec, or verdict.
    contract: String,
    /// JSON document file to validate.
    document: PathBuf,
}

#[derive(Args, Debug)]
struct LoopArgs {
    #[command(subcommand)]
    action: LoopAction,
}

#[derive(Subcommand, Debug)]
enum LoopAction {
    /// Cases below the loop target with tickets and claims.
    Status,
    /// Pick the worst open case, claim it, and write its slice spec.
    Dispatch {
        /// Case name (default: worst open case below the target).
        case: Option<String>,
        /// Composite floor for dispatchability.
        #[arg(long, default_value_t = 0.5)]
        below: f64,
        /// Claimant name.
        #[arg(long, default_value = "local")]
        claimant: String,
    },
    /// Score + ingest a rerun; at the target, release the claim.
    Verify {
        /// Rerun case name (e.g. real-ticket-001-v2-rerun).
        case: String,
        /// Claimant name.
        #[arg(long, default_value = "local")]
        claimant: String,
    },
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
    /// Process supervision: per-step verdicts + suspicious steps where
    /// the step-level signal contradicts the outcome claim.
    Steps {
        /// Path to the run file (evals/cases/<case>/run.json).
        run: PathBuf,
    },
    /// Regression gate over all cases vs the committed baseline.
    Gate {
        /// Max allowed composite drop.
        #[arg(long, default_value_t = 0.05)]
        tolerance: f64,
        /// Max allowed tool-mismatch growth per case vs baseline.
        #[arg(long, default_value_t = 1)]
        mismatch_tolerance: usize,
        /// Snapshot current results as the new baseline.
        #[arg(long)]
        write_baseline: bool,
    },
    /// Record tool mismatches vs the golden into the mismatch register
    /// (one run, or all cases under evals/cases when omitted).
    Mismatches {
        /// Path to the run file (defaults to every case).
        run: Option<PathBuf>,
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
            EvalAction::Steps { run } => cmd_eval_steps(&run),
            EvalAction::Gate {
                tolerance,
                mismatch_tolerance,
                write_baseline,
            } => cmd_eval_gate(tolerance, mismatch_tolerance, write_baseline),
            EvalAction::Mismatches { run } => cmd_eval_mismatches(run.as_deref()),
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
        Command::Ticket(TicketArgs { action }) => match action {
            TicketAction::List => cmd_ticket_list(),
            TicketAction::Show { id } => cmd_ticket_show(&id),
            TicketAction::Validate { id } => cmd_ticket_validate(&id),
            TicketAction::ValidateGraph => cmd_ticket_validate_graph(),
            TicketAction::Graph => cmd_ticket_graph(),
            TicketAction::Claim {
                id,
                claimant,
                force,
            } => cmd_ticket_claim(&id, &claimant, force),
            TicketAction::Release { id, claimant } => cmd_ticket_release(&id, &claimant),
            TicketAction::Claims => cmd_ticket_claims(),
        },
        Command::Run(RunArgs { action }) => match action {
            RunAction::Ingest { run, retro } => cmd_run_ingest(&run, retro.as_deref()),
            RunAction::Verify { run } => cmd_run_verify(&run),
            RunAction::Failures { run } => cmd_run_failures(&run),
        },
        Command::Insights => cmd_insights(),
        Command::Backlog => cmd_backlog(),
        Command::Resume => cmd_resume(),
        Command::Health => cmd_health(),
        Command::Audit => cmd_audit(),
        Command::Codex(CodexArgs {
            spec,
            workdir,
            run_out,
        }) => cmd_codex(&spec, &workdir, run_out.as_deref()),
        Command::Harness => cmd_harness(),
        Command::Loop(LoopArgs { action }) => match action {
            LoopAction::Status => cmd_loop_status(),
            LoopAction::Dispatch {
                case,
                below,
                claimant,
            } => cmd_loop_dispatch(case.as_deref(), below, &claimant),
            LoopAction::Verify { case, claimant } => cmd_loop_verify(&case, &claimant),
        },
    }
}

fn cmd_health() -> ExitCode {
    match mini_agi_core::health::health(&root()) {
        Ok(report) => {
            println!("HEALTH CHECK — {}", report.verdict());
            if let Some(load1) = report.load1 {
                println!("  load1: {load1:.2} on {} cores", report.nproc);
            }
            if let Some(frac) = report.mem_available_frac {
                println!("  memory available: {:.0}%", frac * 100.0);
            }
            if let Some(frac) = report.swap_used_frac {
                println!("  swap used: {:.0}%", frac * 100.0);
            }
            if let Some(largest) = report.zoo_largest {
                println!("  largest process zoo: {largest} processes per command");
            }
            if let Some(j) = report.journal {
                println!(
                    "  journal: {} begins, {} passes, {} fails, {} status",
                    j[0], j[1], j[2], j[3]
                );
            }
            if report.findings.is_empty() {
                println!("  no findings");
            }
            for finding in &report.findings {
                println!("  [{}] {}", finding.severity, finding.message);
            }
            match report.verdict() {
                "OK" => ExitCode::SUCCESS,
                "WARN" => ExitCode::from(1),
                _ => ExitCode::from(2),
            }
        }
        Err(e) => fail(&format!("health: {e}")),
    }
}

fn cmd_audit() -> ExitCode {
    match mini_agi_core::audit::audit(&root()) {
        Ok(report) => {
            println!("AUDIT CHECK — {}", report.verdict());
            for line in &report.passed {
                println!("  [ok] {line}");
            }
            for finding in &report.findings {
                println!("  [{}] {}", finding.severity, finding.message);
            }
            match report.verdict() {
                "OK" => ExitCode::SUCCESS,
                "WARN" => ExitCode::from(1),
                _ => ExitCode::from(2),
            }
        }
        Err(e) => fail(&format!("audit: {e}")),
    }
}

fn cmd_codex(spec: &Path, workdir: &Path, run_out: Option<&Path>) -> ExitCode {
    use mini_agi_core::capture;
    let spec_text = match std::fs::read_to_string(spec) {
        Ok(t) => t,
        Err(e) => return fail(&format!("cannot read spec {}: {e}", spec.display())),
    };
    std::fs::create_dir_all(workdir).unwrap_or(());
    let goal = spec_text
        .lines()
        .find_map(|l| l.strip_prefix("- goal: "))
        .unwrap_or("(goal not parsed from spec)")
        .to_string();
    let scope = spec_text
        .lines()
        .find_map(|l| l.strip_prefix("- scope: "))
        .unwrap_or("")
        .to_string();
    let prompt = format!(
        "{spec_text}\n\nIMPLEMENTATION PROTOCOL (binding): plan first, tests first, never repeat a failing action. When the work is done and your own gate passes, END YOUR FINAL MESSAGE with:\n<promise>COMPLETE</promise>\n<result>{{\"summary\": \"one sentence\"}}</result>\n"
    );
    let log_path = workdir.join("codex.log");
    let output = match std::process::Command::new("codex")
        .args([
            "exec",
            "-s",
            "workspace-write",
            "--skip-git-repo-check",
            &prompt,
        ])
        .current_dir(workdir)
        .output()
    {
        Ok(o) => o,
        Err(e) => return fail(&format!("codex not available: {e}")),
    };
    let transcript = String::from_utf8_lossy(&output.stdout);
    std::fs::write(
        &log_path,
        format!("{transcript}{}", String::from_utf8_lossy(&output.stderr)),
    )
    .unwrap_or(());
    let outcome = capture::CaptureOutcome {
        log_path: log_path.clone(),
        steps: capture::parse_transcript(&transcript),
        completed: capture::completed(&transcript),
        result: capture::extract_result(&transcript),
    };
    println!(
        "codex: exit {}, completed={}, {} captured steps, log: {}",
        output
            .status
            .code()
            .map_or_else(|| "-".into(), |c| c.to_string()),
        outcome.completed,
        outcome.steps.len(),
        log_path.display()
    );
    if let Some(result) = &outcome.result {
        println!("  result: {result}");
    }
    for step in &outcome.steps {
        println!(
            "  [{}] {}",
            step.tool,
            step.action.chars().take(100).collect::<String>()
        );
    }
    let trajectory: Vec<serde_json::Value> = outcome
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            serde_json::json!({
                "step": i + 1,
                "action": s.action,
                "tool": s.tool,
                "ok": true,
                "goal_aligned": true,
                "tokens": 0,
                "output_tokens": 0,
                "note": format!("captured from codex transcript line {}", s.line),
                "paths": s.paths,
            })
        })
        .collect();
    let scope_list: Vec<&str> = scope
        .split(',')
        .map(|s| s.trim().trim_matches('`'))
        .filter(|s| !s.is_empty())
        .collect();
    let run = serde_json::json!({
        "goal": goal,
        "scope": scope_list,
        "outcome": {"achieved": false, "tests": false, "typecheck": false},
        "tokens_total": 0,
        "cost_usd": 0.0,
        "golden": null,
        "trajectory": trajectory,
    });
    let out_path = run_out.unwrap_or(&workdir.join("run.json")).to_path_buf();
    match std::fs::write(
        &out_path,
        serde_json::to_string_pretty(&run).unwrap_or_default(),
    ) {
        Ok(()) => println!("  run draft: {}", out_path.display()),
        Err(e) => return fail(&format!("cannot write run draft: {e}")),
    }
    ExitCode::SUCCESS
}

fn cmd_harness() -> ExitCode {
    match mini_agi_core::harness::snapshot(&root()) {
        Ok((name, verdict)) => {
            println!("harness snapshot: {name}");
            println!("  frozen suite: {verdict}");
            println!("  ledger: docs/harness/ledger.md");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("harness snapshot: {e}")),
    }
}

fn cmd_loop_status() -> ExitCode {
    match mini_agi_core::loopcmd::status(&root()) {
        Ok(report) => {
            println!(
                "loop status: {} runs, composite avg {:.4}, {} cases below target {:.2}",
                report.runs,
                report.composite_avg,
                report.cases.len(),
                mini_agi_core::loopcmd::TARGET_COMPOSITE
            );
            for row in &report.cases {
                let ticket = row.ticket.as_ref().map_or_else(
                    || "no ticket".to_string(),
                    |id| format!("{id} [{}]", row.status.as_deref().unwrap_or("?")),
                );
                let claim = row.claimant.as_deref().unwrap_or("unclaimed");
                let rerun = match row.rerun_composite {
                    Some(c) if c >= mini_agi_core::loopcmd::TARGET_COMPOSITE => {
                        format!("rerun {c:.4} — CLOSED")
                    }
                    Some(c) => format!("rerun {c:.4}"),
                    None => "no rerun".to_string(),
                };
                println!(
                    "  {:.4}  {:<24} {}  lease: {}  {}",
                    row.composite, row.case, ticket, claim, rerun
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("loop status: {e}")),
    }
}

fn cmd_loop_dispatch(case: Option<&str>, below: f64, claimant: &str) -> ExitCode {
    match mini_agi_core::loopcmd::dispatch(&root(), case, below, claimant) {
        Ok(outcome) => {
            println!(
                "dispatched: {} -> {} ({}claimed) — spec: {}",
                outcome.case,
                outcome.ticket,
                if outcome.ticket_created {
                    "ticket created, "
                } else {
                    ""
                },
                outcome.spec.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("loop dispatch: {e}")),
    }
}

fn cmd_loop_verify(case: &str, claimant: &str) -> ExitCode {
    match mini_agi_core::loopcmd::verify(&root(), case, claimant) {
        Ok((text, closed)) => {
            println!("{text}");
            if closed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => fail(&format!("loop verify: {e}")),
    }
}

fn cmd_backlog() -> ExitCode {
    match insights::backlog(&root()) {
        Ok(items) => {
            for item in &items {
                if item.created {
                    println!("created: {} — gap: {}", item.id, item.case);
                } else {
                    println!("exists: {} — gap: {}", item.id, item.case);
                }
            }
            if items.is_empty() {
                println!("no capability gaps — roadmap is clear");
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot generate backlog: {e}")),
    }
}

fn cmd_resume() -> ExitCode {
    match insights::resume(&root()) {
        Ok(block) => {
            print!("{block}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot resume: {e}")),
    }
}

fn cmd_run_ingest(run: &Path, retro: Option<&Path>) -> ExitCode {
    match ingest_text(&root(), run, retro) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(msg) => fail(&msg),
    }
}

fn cmd_run_verify(run: &Path) -> ExitCode {
    let root = root();
    match mini_agi_core::verifier::verify_run(&root, run) {
        Ok(v) => {
            println!(
                "verify {}: {} (exit {})",
                v.case,
                v.status,
                v.exit_code
                    .map_or_else(|| "-".to_string(), |c| c.to_string())
            );
            if let Some(cmd) = &v.command {
                println!("  command: {cmd}");
            }
            if let Some(target) = &v.target {
                println!("  target: {target}");
            }
            if !v.output_excerpt.is_empty() {
                println!("  last output: {}", v.output_excerpt);
            }
            if v.status == "disagrees" {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => fail(&format!("run verify: {e}")),
    }
}

fn cmd_run_failures(run: &Path) -> ExitCode {
    let root = root();
    match mini_agi_core::failure::analyze_run(run) {
        Ok((case, entries)) => {
            if entries.is_empty() {
                println!("no repeated failing actions in {case}");
                return ExitCode::SUCCESS;
            }
            for e in &entries {
                println!(
                    "`{}` tool={} action=\"{}\" count={} steps={:?} case={}",
                    e.hash, e.tool, e.action, e.count, e.steps, e.case
                );
            }
            match mini_agi_core::failure::update_register(&root, &entries) {
                Ok(total) => {
                    println!(
                        "recorded {} repeated failing actions (register: {}, {} total)",
                        entries.len(),
                        mini_agi_core::failure::register_path(&root).display(),
                        total
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&format!("cannot update failure register: {e}")),
            }
        }
        Err(msg) => fail(&msg),
    }
}

fn cmd_eval_mismatches(run: Option<&Path>) -> ExitCode {
    let root = root();
    let golden = root.join("evals/golden");
    let mut runs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(run) = run {
        runs.push(run.to_path_buf());
    } else {
        match std::fs::read_dir(root.join("evals/cases")) {
            Ok(entries) => {
                let mut dirs: Vec<_> = entries
                    .filter_map(Result::ok)
                    .map(|e| e.path().join("run.json"))
                    .filter(|p| p.exists())
                    .collect();
                dirs.sort();
                runs = dirs;
            }
            Err(e) => return fail(&format!("cannot list evals/cases: {e}")),
        }
    }
    if runs.is_empty() {
        return fail("no run.json files found");
    }
    let mut any = false;
    for run_path in runs {
        match mini_agi_core::mismatch::analyze_run(&run_path, &golden, &root) {
            Ok((case, entries)) => {
                if entries.is_empty() {
                    println!("no tool mismatches in {case}");
                    continue;
                }
                any = true;
                for e in &entries {
                    println!(
                        "{} step {}: golden expects {}, used {} ({})",
                        e.case, e.step, e.golden_tool, e.run_tool, e.hash
                    );
                }
                match mini_agi_core::mismatch::update_register(&root, &entries) {
                    Ok(total) => {
                        println!(
                            "recorded {} tool mismatches in {case} (register: {}, {} total)",
                            entries.len(),
                            mini_agi_core::mismatch::register_path(&root).display(),
                            total
                        );
                    }
                    Err(e) => return fail(&format!("cannot update mismatch register: {e}")),
                }
            }
            Err(msg) => return fail(&msg),
        }
    }
    if !any {
        println!("no tool mismatches in any case");
    }
    ExitCode::SUCCESS
}

/// Shared by the CLI and the MCP server (no stdout pollution in server
/// mode).
fn ingest_text(root: &Path, run: &Path, retro: Option<&Path>) -> Result<String, String> {
    let report = insights::ingest_run(root, run, retro)?;
    Ok(format!(
        "ingested: {} (composite {:.4}, {} tokens, {:.4} USD)\nworld model: {} new facts, {} known\nnext: mini-agi derive && mini-agi provenance",
        report.case,
        report.composite,
        report.tokens,
        report.cost_usd,
        report.new_facts,
        report.skipped
    ))
}

fn cmd_insights() -> ExitCode {
    match insights::insights(&root()) {
        Ok(report) => {
            println!("SYSTEM INTELLIGENCE REPORT");
            println!(
                "  runs: {} (composite avg {:.4} history | {:.4} effective, {} tokens, {:.4} USD)",
                report.runs,
                report.composite_avg,
                report.composite_avg_effective,
                report.tokens_total,
                report.cost_total
            );
            for case in &report.cases {
                println!("    {}: {:.4}", case.case, case.composite);
            }
            println!(
                "  memory: {} entries, {} facts",
                report.entries, report.facts
            );
            println!("  tickets: {}", report.tickets);
            println!(
                "  journal: {} begins, {} passes, {} fails, {} status",
                report.journal[0], report.journal[1], report.journal[2], report.journal[3]
            );
            if report.gaps.is_empty() {
                println!("  capability gaps: none — no failing runs");
            } else {
                println!("  capability gaps (roadmap, ADR-0005):");
                for gap in &report.gaps {
                    println!("    {gap}");
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot compute insights: {e}")),
    }
}

fn cmd_ticket_list() -> ExitCode {
    match ticket::list_tickets(&root()) {
        Ok(tickets) => {
            for t in &tickets {
                let deps = if t.blocked_by.is_empty() {
                    String::new()
                } else {
                    format!("  blocked_by: {}", t.blocked_by.join(", "))
                };
                let status = if t.status == "OPEN" {
                    String::new()
                } else {
                    format!("  [{}]", t.status)
                };
                println!(
                    "{}  {}{}  scope: {}{}",
                    t.id,
                    t.title,
                    status,
                    t.scope.join(", "),
                    deps
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot list tickets: {e}")),
    }
}

fn cmd_ticket_show(id: &str) -> ExitCode {
    match ticket::find_ticket(&root(), id) {
        Ok(t) => {
            println!("id: {}", t.id);
            println!("title: {}", t.title);
            println!("goal: {}", t.goal);
            println!("scope: {}", t.scope.join(", "));
            println!("status: {}", t.status);
            println!("blocked_by: {}", t.blocked_by.join(", "));
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_ticket_validate(id: &str) -> ExitCode {
    match ticket::find_ticket(&root(), id) {
        Ok(t) => {
            println!(
                "ok: {} ({}) validates against the ticket contract",
                t.id, t.title
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_ticket_validate_graph() -> ExitCode {
    let root = root();
    let mut problems = Vec::new();
    if let Ok(tickets) = ticket::list_tickets(&root) {
        for t in &tickets {
            if t.status != "OPEN" && t.status != "CLOSED" {
                problems.push(format!("{}: unknown status '{}'", t.id, t.status));
            }
        }
    }
    match ticket::validate_graph(&root) {
        Ok(graph_problems) => {
            problems.extend(graph_problems);
            if problems.is_empty() {
                println!("ok: dependency graph valid ({} tickets)", {
                    ticket::list_tickets(&root).map_or(0, |v| v.len())
                });
                ExitCode::SUCCESS
            } else {
                for p in &problems {
                    println!("problem: {p}");
                }
                ExitCode::from(1)
            }
        }
        Err(e) => fail(&format!("cannot validate graph: {e}")),
    }
}

fn cmd_ticket_graph() -> ExitCode {
    let root = root();
    match ticket::list_tickets(&root) {
        Ok(tickets) => {
            let mut edges = 0;
            for t in &tickets {
                for dep in &t.blocked_by {
                    println!("{dep} -> {}", t.id);
                    edges += 1;
                }
            }
            println!(
                "graph: {} tickets, {} edges{}",
                tickets.len(),
                edges,
                if edges == 0 { " (no dependencies)" } else { "" }
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot list tickets: {e}")),
    }
}

fn cmd_ticket_claim(id: &str, claimant: &str, force: bool) -> ExitCode {
    let root = root();
    match ticket::claim_ticket(&root, id, claimant, force) {
        Ok(claim) => {
            println!(
                "claimed: {} by {} since {}",
                claim.ticket, claim.claimant, claim.since
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_ticket_release(id: &str, claimant: &str) -> ExitCode {
    let root = root();
    match ticket::release_ticket(&root, id, claimant) {
        Ok(()) => {
            println!("released: {id}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_ticket_claims() -> ExitCode {
    let root = root();
    match ticket::read_claims(&root) {
        Ok(claims) => {
            if claims.is_empty() {
                println!("no claims held");
            } else {
                for c in &claims {
                    println!("{} claimed by {} since {}", c.ticket, c.claimant, c.since);
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot read claims: {e}")),
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
        Err(EvalError::GoldenRead(e)) => fail(&format!("cannot read golden file: {e}")),
        Err(EvalError::Json(e)) => fail(&format!("invalid run json: {e}")),
        Err(EvalError::InvalidField(f)) => fail(&format!("invalid run field '{f}'")),
        Err(EvalError::Metadata(m)) => fail(&m),
    }
}

fn cmd_eval_steps(run: &Path) -> ExitCode {
    let root = root();
    match eval::score_run(run, &root, &root.join("evals/golden")) {
        Ok(report) => {
            println!(
                "process supervision for {} (outcome {}):",
                report.case, report.dims.outcome
            );
            let text = std::fs::read_to_string(run).unwrap_or_default();
            let run: eval::Run =
                serde_json::from_str(&text).unwrap_or_else(|_| panic!("invalid run json: {text}"));
            let verdicts = eval::score_steps(&run);
            let suspicious: Vec<_> = verdicts.iter().filter(|v| v.suspicious).collect();
            for v in &verdicts {
                let flag = if v.suspicious {
                    "  <-- SUSPICIOUS (judge budget)"
                } else {
                    ""
                };
                println!(
                    "  step {} [{}] score {:.2}{}",
                    v.step, v.tool, v.score, flag
                );
            }
            println!(
                "{}",
                if suspicious.is_empty() {
                    "no suspicious steps — step-level signal agrees with the outcome".to_string()
                } else {
                    format!(
                        "{} suspicious step(s) — step/outcome divergence, allocate judge budget",
                        suspicious.len()
                    )
                }
            );
            ExitCode::SUCCESS
        }
        Err(EvalError::Read(e)) => fail(&format!("cannot read run file: {e}")),
        Err(EvalError::GoldenRead(e)) => fail(&format!("cannot read golden file: {e}")),
        Err(EvalError::Json(e)) => fail(&format!("invalid run json: {e}")),
        Err(EvalError::InvalidField(f)) => fail(&format!("invalid run field '{f}'")),
        Err(EvalError::Metadata(m)) => fail(&m),
    }
}

fn cmd_eval_gate(tolerance: f64, mismatch_tolerance: usize, write_baseline: bool) -> ExitCode {
    match eval_gate_text(&root(), tolerance, mismatch_tolerance, write_baseline) {
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

fn eval_gate_text(
    root: &Path,
    tolerance: f64,
    mismatch_tolerance: usize,
    write_baseline: bool,
) -> Result<String, String> {
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
    let result = eval::run_gate(&entries, &baseline, tolerance, mismatch_tolerance);
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
