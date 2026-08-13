#![allow(missing_docs)]
//! mini-agi — condensed CLI.
//!
//! Business model: research -> knowledge -> patterns -> implementation.
//! Commands cover the KNOWLEDGE core (mem/dream/skill/checkpoint/init/
//! ticket) and the LOOP (gap -> ticket -> dispatch -> gate-verify).
//! The worker execution (codex/harness/sandbox) stays, minus the
//! over-verification machinery.

mod clifmt;
mod init;
mod mcp;
mod sandbox;
mod supervisor;
pub mod worker;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

/// Repository root: `AGENTIC_ROOT` env var, else current directory.
fn root() -> PathBuf {
    std::env::var_os("AGENTIC_ROOT").map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        PathBuf::from,
    )
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(1)
}

#[derive(Parser)]
#[command(name = "mini-agi", version, about = "knowledge + loop kernel")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Episodic-buffer -> canonical memory.
    Mem(MemArgs),
    /// Canonical -> derived views.
    Derive,
    /// Print the canonical fingerprint.
    Provenance,
    /// Skills/patterns registry.
    Skill(SkillArgs),
    /// Checkpoint journal audit (T008).
    Checkpoint,
    /// Scaffold a repo.
    Init(InitArgs),
    /// Ticket lifecycle.
    Ticket(TicketArgs),
    /// Compounding report.
    Insights,
    /// Proactive loop: status/dispatch/objective/verify.
    Loop(LoopArgs),
    /// Distill research material into staged knowledge.
    Dream(DreamArgs),
    /// Codex worker run.
    Codex(CodexArgs),
    /// Harness counterfactual gate.
    Harness(HarnessArgs),
    /// Landlock worker sandbox wrapper.
    ExecSandbox(ExecSandboxArgs),
    /// stdio MCP server.
    Mcp,
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
        buffer: String,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        require_signoff: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        approve: Option<String>,
    },
    /// Promote one contested fact from the review queue.
    Signoff {
        queue: String,
        index: usize,
        domain: Option<String>,
        #[arg(long)]
        approve: Option<String>,
    },
    /// Regenerate derived views.
    Derive {
        #[arg(long)]
        brief_only: bool,
        #[arg(long)]
        approve: Option<String>,
    },
    /// Query canonical facts.
    Query {
        keyword: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        raw: bool,
    },
}

#[derive(Args, Debug)]
struct SkillArgs {
    #[command(subcommand)]
    action: SkillAction,
}
#[derive(Subcommand, Debug)]
enum SkillAction {
    List,
    Show {
        name: String,
    },
    Add {
        source: String,
        #[arg(long)]
        approve: Option<String>,
    },
}

#[derive(Args, Debug)]
struct TicketArgs {
    #[command(subcommand)]
    action: TicketAction,
}
#[derive(Subcommand, Debug)]
enum TicketAction {
    List,
    Show { id: String },
}

#[derive(Args, Debug)]
struct InitArgs {
    /// Create the CLAUDE.md import-shim (opt-in).
    #[arg(long)]
    claude_shim: bool,
}

#[derive(Args, Debug)]
struct LoopArgs {
    #[command(subcommand)]
    action: LoopAction,
}
#[derive(Subcommand, Debug)]
enum LoopAction {
    Status,
    Dispatch {
        case: Option<String>,
        #[arg(long)]
        claimant: String,
    },
    Objective {
        max_cases: usize,
        #[arg(long)]
        claimant: String,
        #[arg(long)]
        budget_cost: Option<String>,
    },
    Verify {
        case: String,
        #[arg(long)]
        claimant: String,
        #[arg(long)]
        allow_unverified: bool,
    },
}

#[derive(Args, Debug)]
struct DreamArgs {
    /// Source file to distill.
    #[arg(long)]
    source: Option<String>,
    /// HITL approval reason (ADR-0010): canonical writes are refused
    /// without it — dream --source writes canonical directly.
    #[arg(long)]
    approve: Option<String>,
    /// Apply a persisted auditor verdicts manifest
    /// (`memory/staging/<date>/<seq>.verdicts.json`): the audited
    /// promote half of the dream-loop.
    #[arg(long)]
    promote: Option<String>,
}

#[derive(Args, Debug)]
struct CodexArgs {
    /// Slice spec file.
    spec: String,
    /// Workdir.
    workdir: String,
    #[arg(long)]
    verify: Option<String>,
    #[arg(long)]
    target: Option<String>,
    #[arg(long)]
    iterate: Option<usize>,
    /// HITL approval reason (ADR-0014): when config `require_approval` is
    /// set, a run without this is refused.
    #[arg(long)]
    approve: Option<String>,
}

#[derive(Args, Debug)]
struct HarnessArgs {
    #[command(subcommand)]
    action: HarnessAction,
}
#[derive(Subcommand, Debug)]
enum HarnessAction {
    Snapshot,
    Verify {
        target: String,
        candidate: String,
        #[arg(long)]
        claims: Option<String>,
    },
}

#[derive(Args, Debug)]
struct ExecSandboxArgs {
    #[arg(long)]
    allow_write: Vec<PathBuf>,
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Mem(args) => cmd_mem(args),
        Command::Derive => match mini_agi_core::memory::derive(&root(), false) {
            Ok(_) => {
                println!("derived: views regenerated");
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("derive: {e}")),
        },
        Command::Provenance => cmd_provenance(),
        Command::Skill(args) => cmd_skill(args),
        Command::Checkpoint => cmd_checkpoint(),
        Command::Init(args) => cmd_init(&args),
        Command::Ticket(args) => cmd_ticket(args),
        Command::Insights => cmd_insights(),
        Command::Loop(args) => cmd_loop(args),
        Command::Dream(args) => cmd_dream(&args),
        Command::Codex(args) => cmd_codex(&args),
        Command::Harness(args) => cmd_harness(&args),
        Command::ExecSandbox(args) => cmd_exec_sandbox(&args),
        Command::Mcp => match mcp::run_stdio_server() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("mcp server error: {e}")),
        },
    }
}

fn cmd_provenance() -> ExitCode {
    let root = root();
    println!(
        "canonical_sha256: {}",
        mini_agi_core::memory::canonical_fingerprint(&root)
    );
    ExitCode::SUCCESS
}

fn cmd_checkpoint() -> ExitCode {
    let root = root();
    let path = root.join("memory/episodic/checkpoints.log");
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!("checkpoint: absent (no journal)");
        return ExitCode::SUCCESS;
    };
    let events = mini_agi_core::journal::parse_journal(&text);
    let audit = mini_agi_core::journal::audit_journal(&events);
    println!(
        "checkpoint: {} events, {} bad, {} historical",
        events.len(),
        audit.bad.len(),
        audit.historical.len()
    );
    for a in &audit.bad {
        eprintln!("ANOMALY (line {}): {}", a.line_no, a.message);
    }
    if audit.bad.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn cmd_mem(args: MemArgs) -> ExitCode {
    let root = root();
    match args.action {
        MemAction::Consolidate {
            buffer,
            domain,
            require_signoff,
            dry_run,
            approve,
        } => {
            if !dry_run && approve.is_none() {
                return fail("mem consolidate requires --approve <reason> (HITL) unless --dry-run");
            }
            let text = match std::fs::read_to_string(&buffer) {
                Ok(t) => t,
                Err(e) => return fail(&format!("consolidate: cannot read buffer: {e}")),
            };
            let opts = mini_agi_core::memory::ConsolidateOptions {
                domain: domain.unwrap_or_else(|| "general".into()),
                require_signoff,
                dry_run,
            };
            match mini_agi_core::memory::consolidate(&root, &text, &buffer, &opts) {
                Ok(out) => {
                    println!(
                        "consolidated {} new facts, {} skipped",
                        out.new_facts, out.skipped
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&format!("consolidate: {e}")),
            }
        }
        MemAction::Signoff {
            queue,
            index,
            domain,
            approve,
        } => {
            if approve.is_none() {
                return fail("mem signoff requires --approve <reason> (HITL)");
            }
            match mini_agi_core::memory::signoff(
                &root,
                Path::new(&queue),
                index,
                &domain.unwrap_or_else(|| "general".into()),
            ) {
                Ok(entry) => {
                    println!("promoted: {}", entry.path.display());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&format!("signoff: {e}")),
            }
        }
        MemAction::Derive {
            brief_only,
            approve,
        } => {
            if approve.is_none() {
                return fail("mem derive requires --approve <reason> (HITL)");
            }
            match mini_agi_core::memory::derive(&root, brief_only) {
                Ok(_) => {
                    println!("derived: views regenerated");
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&format!("derive: {e}")),
            }
        }
        MemAction::Query {
            keyword,
            domain,
            raw,
        } => {
            let facts =
                mini_agi_core::memory::query_facts(&root, domain.as_deref(), keyword.as_deref());
            if facts.is_empty() {
                println!("no matching facts");
                return ExitCode::SUCCESS;
            }
            for f in &facts {
                if raw {
                    println!("{f:?}");
                } else {
                    println!("- {f:?}");
                }
            }
            ExitCode::SUCCESS
        }
    }
}

fn cmd_skill(args: SkillArgs) -> ExitCode {
    let root = root();
    match args.action {
        SkillAction::List => match mini_agi_core::skills::discover_skills(&root) {
            Ok(skills) => {
                for s in skills {
                    println!(
                        "{}  [{}]  {}",
                        s.name,
                        if s.verify.is_some() { "verify" } else { "ref" },
                        s.description
                    );
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("skills: {e}")),
        },
        SkillAction::Show { name } => match mini_agi_core::skills::find_skill(&root, &name) {
            Ok(s) => {
                println!("{}: {}", s.name, s.description);
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e.to_string()),
        },
        SkillAction::Add { source, approve } => {
            if approve.is_none() {
                return fail("skill add requires --approve <reason> (HITL)");
            }
            match mini_agi_core::skills::install_skills(&root, &source) {
                Ok(installed) => {
                    println!("installed: {}", installed.join(", "));
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&e.to_string()),
            }
        }
    }
}

fn cmd_init(args: &InitArgs) -> ExitCode {
    let root = root();
    match init::init(&root, args.claude_shim) {
        Ok(created) => {
            println!("initialized: {}", root.display());
            for item in &created {
                println!("  created: {item}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("init failed: {e}")),
    }
}

fn cmd_ticket(args: TicketArgs) -> ExitCode {
    let root = root();
    match args.action {
        TicketAction::List => {
            for t in mini_agi_core::ticket::list_tickets(&root).unwrap_or_default() {
                println!("{} {} ({})", t.id, t.title, t.status);
            }
            ExitCode::SUCCESS
        }
        TicketAction::Show { id } => match mini_agi_core::ticket::find_ticket(&root, &id) {
            Ok(t) => {
                println!("{} {} — {}", t.id, t.title, t.goal);
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e.to_string()),
        },
    }
}

fn cmd_insights() -> ExitCode {
    let root = root();
    let facts = mini_agi_core::memory::canonical_facts(&root);
    println!("{} facts in canonical memory", facts.len());
    ExitCode::SUCCESS
}

fn cmd_loop(args: LoopArgs) -> ExitCode {
    let root = root();
    match args.action {
        LoopAction::Status => match mini_agi_core::loopcmd::status(&root) {
            Ok(s) => {
                for r in s.cases {
                    println!("{} attempts={} {:?}", r.case, r.attempts, r.status);
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("loop status: {e}")),
        },
        LoopAction::Dispatch { case, claimant } => {
            if let Some(reason) = mini_agi_core::loopcmd::dispatch_no_work(&root, 0.5) {
                println!("loop dispatch: STOP — {reason}");
                return ExitCode::SUCCESS;
            }
            match mini_agi_core::loopcmd::dispatch(&root, case.as_deref(), 0.5, &claimant) {
                Ok(out) => {
                    println!(
                        "dispatched: {} -> {} — spec: {}",
                        out.case,
                        out.ticket,
                        out.spec.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&format!("loop dispatch: {e}")),
            }
        }
        LoopAction::Objective {
            max_cases,
            claimant,
            budget_cost,
        } => {
            let budget = match budget_cost.as_deref() {
                None => None,
                Some(c) => match c.parse::<f64>() {
                    Ok(v) if v >= 0.0 => Some(v),
                    _ => return fail(&format!("loop objective: invalid --budget-cost '{c}'")),
                },
            };
            match mini_agi_core::loopcmd::objective(&root, max_cases, &claimant, budget) {
                Ok(out) => {
                    println!(
                        "dispatched {} case(s), spent ${:.2}",
                        out.dispatched.len(),
                        out.budget_spent
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&format!("loop objective: {e}")),
            }
        }
        LoopAction::Verify {
            case,
            claimant,
            allow_unverified,
        } => match mini_agi_core::loopcmd::verify(&root, &case, &claimant, allow_unverified) {
            Ok((text, closed)) => {
                println!("{text}");
                if closed {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                }
            }
            Err(e) => fail(&format!("loop verify: {e}")),
        },
    }
}

fn cmd_dream(args: &DreamArgs) -> ExitCode {
    use std::fmt::Write as _;
    let root = root();
    if let Some(manifest) = &args.promote {
        if args.approve.is_none() {
            return fail("dream --promote requires --approve <reason> (HITL, ADR-0010)");
        }
        // Containment (parity with signoff): the manifest must live under
        // memory/staging/ — a caller-supplied path must not read an
        // arbitrary file or write arbitrary `- source:` metadata.
        let staging_root = root.join("memory/staging");
        let manifest_path = Path::new(manifest);
        let candidate = if manifest_path.is_absolute() {
            manifest_path.to_path_buf()
        } else {
            root.join(manifest_path)
        };
        let manifest_canon = match candidate.canonicalize() {
            Ok(c) if c.starts_with(&staging_root) => c,
            _ => {
                return fail(&format!(
                    "dream --promote: {manifest} is outside memory/staging/"
                ));
            }
        };
        // Use the CANONICAL path for every read (containment verified on
        // the path that is actually opened — no TOCTOU mismatch).
        let verdicts = mini_agi_core::dream::read_verdicts(&manifest_canon);
        if verdicts.is_empty() {
            return fail(&format!("dream --promote: no verdicts in {manifest}"));
        }
        // The staged facts sit next to the manifest (`<seq>.md`).
        let staged_path = manifest_canon.with_extension("md");
        // Idempotency receipt: a manifest whose staged file was already
        // promoted must NOT be re-applied (documented D2 contract).
        if let Some(receipt) = mini_agi_core::dream::read_promotion_receipt(&staged_path)
            && mini_agi_core::dream::receipt_matches_staged(&staged_path, &receipt)
        {
            return fail(&format!(
                "dream --promote: {} was already promoted (receipt {}); nothing re-applied",
                staged_path.display(),
                receipt.staged
            ));
        }
        // The staged facts sit next to the manifest (`<seq>.md`).
        let staged_path = Path::new(manifest).with_extension("md");
        let staged = match std::fs::read_to_string(&staged_path) {
            Ok(t) => mini_agi_core::dream::parse_distilled_facts(&t),
            Err(e) => {
                return fail(&format!(
                    "dream --promote: cannot read {}: {e}",
                    staged_path.display()
                ));
            }
        };
        match mini_agi_core::dream::apply_verdicts(
            &root,
            &staged,
            &verdicts,
            &manifest_canon.to_string_lossy(),
            false,
        ) {
            Ok((promoted, queued, skipped)) => {
                println!(
                    "dream --promote: {promoted} promoted, {queued} queued, {skipped} skipped"
                );
                return ExitCode::SUCCESS;
            }
            Err(e) => return fail(&format!("dream --promote: {e}")),
        }
    }
    match &args.source {
        Some(source) => {
            if args.approve.is_none() {
                return fail(
                    "dream requires --approve <reason> (HITL, ADR-0010) — it writes canonical directly",
                );
            }
            let source_path = Path::new(source);
            let candidate = if source_path.is_absolute() {
                source_path.to_path_buf()
            } else {
                root.join(source_path)
            };
            let source_canon = match candidate.canonicalize() {
                Ok(c) if c.starts_with(&root) => c,
                _ => {
                    return fail(&format!("dream: {source} is outside the repo root"));
                }
            };
            let text = match std::fs::read_to_string(&source_canon) {
                Ok(t) => t,
                Err(e) => return fail(&format!("dream: {e}")),
            };
            let staged = mini_agi_core::dream::parse_distilled_facts(&text);
            // ADR-0010 D2 (parity with the MCP dream tool): enforcement-
            // bound facts ALWAYS route to the human queue, never straight
            // into canonical — even with --approve.
            let mut buffer = String::new();
            let mut queued = 0usize;
            for f in &staged {
                if f.body.contains("enforced_by") {
                    let h = mini_agi_core::hash::fact_id(&f.body);
                    let flat_h = mini_agi_core::memory::fact_digest_stored(&f.body);
                    let q = root.join("memory/review").join(format!(
                        "contested-{}.md",
                        mini_agi_core::memory::utc_now_date()
                    ));
                    let already = mini_agi_core::memory::queued_facts(&q)
                        .iter()
                        .any(|(d, _)| *d == flat_h);
                    if !already {
                        let _ = mini_agi_core::memory::append_contested(
                            &root,
                            &f.body,
                            &h,
                            source,
                            "0000000000000000",
                        );
                    }
                    queued += 1;
                    continue;
                }
                let _ = writeln!(buffer, "- {}", f.body);
            }
            let opts = mini_agi_core::memory::ConsolidateOptions {
                domain: "knowledge".into(),
                require_signoff: false,
                dry_run: false,
            };
            match mini_agi_core::memory::consolidate(&root, &buffer, source, &opts) {
                Ok(out) => {
                    println!(
                        "dream: {} facts distilled into canonical ({} enforcement-bound queued for human review)",
                        out.new_facts, queued
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&format!("dream: {e}")),
            }
        }
        None => fail("dream requires --source <file>"),
    }
}

fn cmd_codex(args: &CodexArgs) -> ExitCode {
    worker::cmd_codex(&worker::CodexRunArgs {
        spec: Path::new(&args.spec),
        workdir: Path::new(&args.workdir),
        run_out: None,
        verify: args.verify.as_deref(),
        target: args.target.as_deref(),
        max_wall: None,
        max_steps: None,
        no_sandbox: false,
        worker_name: None,
        approve: args.approve.clone(),
        iterate: args.iterate.unwrap_or(1),
        blind_worker: false,
        hidden_dir: None,
    })
}

fn cmd_harness(args: &HarnessArgs) -> ExitCode {
    let root = root();
    match &args.action {
        HarnessAction::Snapshot => match mini_agi_core::harness::snapshot(&root) {
            Ok((name, verdict)) => {
                println!("{name}: {verdict}");
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("harness: {e}")),
        },
        HarnessAction::Verify {
            target,
            candidate,
            claims,
        } => {
            match mini_agi_core::harness::verify_candidate(
                &root,
                Path::new(&target),
                Path::new(&candidate),
                claims.as_deref(),
            ) {
                Ok(verdict) => {
                    println!("{verdict}");
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&format!("harness: {e}")),
            }
        }
    }
}

fn cmd_exec_sandbox(args: &ExecSandboxArgs) -> ExitCode {
    worker::cmd_exec_sandbox(&args.allow_write, &args.command)
}
