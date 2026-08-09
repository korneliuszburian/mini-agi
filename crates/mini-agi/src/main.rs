//! mini-agi — single-binary agent kernel: CLI + MCP server shell.
//!
//! Phase 0 CLI: memory consolidate/signoff, derive, provenance. Ports `PoC`
//! (`scripts/consolidate.py`, `scripts/derive.py`) stdout + exit codes 1:1
//! (behavioral contract, tag `v1-spec-reference`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
mod bg;
mod clifmt;
mod planner;
mod research;
mod research_registry;
#[cfg(target_os = "linux")]
mod sandbox;
mod status;
mod supervisor;
mod ui;
pub(crate) mod worker;
use mini_agi_core::contract;
use mini_agi_core::eval::{self, EvalError};
use mini_agi_core::insights;
use mini_agi_core::journal;
use mini_agi_core::memory::{self, ConsolidateOptions, MemoryError};
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
    Backlog(BacklogArgs),
    /// Resume block for a fresh session.
    Resume,
    /// Runtime observability: load, memory, process zoo, journal, claims.
    Health,
    /// Repo invariants: provenance drift, baseline freshness, tree state.
    Audit,
    /// Run-state index (D6): every run, journal tail, live workers.
    Status(StatusArgs),
    /// Dream-loop (D2): distill episodic material into staged facts,
    /// audit them with a strong model, promote verdicts into canonical.
    Dream(DreamArgs),
    /// Live supervision dashboard (D4): std-only HTTP server serving a
    /// self-refreshing page over /api/status.
    Ui(UiArgs),
    /// Auto-researcher (Phase 2): opencode flash worker with a
    /// primary-source research contract; findings land in research/.
    Research(ResearchArgs),
    /// Proactive composition loop (Phase 6.4): status/dispatch/verify.
    Loop(LoopArgs),
    /// Codex integration (Phase 8 slice 4, EXP-003): run codex on a
    /// slice spec, capture the transcript, emit a truthful run.json.
    Codex(CodexArgs),
    /// Harness evolution: versioned spec + ledger + counterfactual gate.
    Harness(HarnessArgs),
    /// Landlock worker sandbox (ADR-0012): apply write-containment to
    /// self, then run the command after `--`. Linux-only.
    ExecSandbox(ExecSandboxArgs),
}

#[derive(Args, Debug)]
struct HarnessArgs {
    #[command(subcommand)]
    action: HarnessAction,
}

#[derive(Subcommand, Debug)]
enum HarnessAction {
    /// Snapshot the versioned harness spec + gate ledger row.
    Snapshot,
    /// Counterfactual gate (Phase 9 slice 5): swaps a candidate file in,
    /// runs the gate, reports the failure delta.
    Verify {
        /// Target file to be edited.
        target: PathBuf,
        /// Candidate file with the new content.
        candidate: PathBuf,
        /// Comma-separated failure(s) the edit claims to fix.
        #[arg(long)]
        claims: Option<String>,
    },
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
    /// Deterministic verifier command for the draft run.json.
    #[arg(long)]
    verify: Option<String>,
    /// Target repo for the verifier (defaults to the workdir).
    #[arg(long)]
    target: Option<String>,
    /// Re-parse an existing transcript log into a fresh run.json draft
    /// (no codex run).
    #[arg(long)]
    reparse_log: Option<PathBuf>,
    /// Worker wall-time cap in seconds (P0-1). Default: workdir's
    /// `.miniagi.json` `max_wall_seconds`; unlimited when unset.
    #[arg(long)]
    max_wall: Option<u64>,
    /// Worker step cap (P0-1). Default: workdir's `.miniagi.json`
    /// `max_steps`; unlimited when unset.
    #[arg(long)]
    max_steps: Option<usize>,
    /// Skip the Landlock sandbox for this run (explicit escape hatch,
    /// ADR-0012).
    #[arg(long)]
    no_sandbox: bool,
    /// Worker executable name (multi-worker; default "codex").
    #[arg(long)]
    worker_name: Option<String>,
    /// HITL approval reason (required when config `require_approval`).
    #[arg(long)]
    approve: Option<String>,
    /// Verified-iteration loop (BREAKTHROUGH): on verifier failure,
    /// re-invoke the worker with the distilled failure register, up to
    /// N attempts (default 1 = single shot).
    #[arg(long, default_value_t = 1)]
    iterate: usize,
    /// Blind-worker mode (EXP-012 isolation as a capability): the
    /// verifier's hidden suite is moved away during the worker run.
    #[arg(long)]
    blind_worker: bool,
    /// The verifier's private hidden-suite directory (required with
    /// --blind-worker).
    #[arg(long)]
    hidden_dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ExecSandboxArgs {
    /// Directories granted write access (repeatable; their subtrees too).
    #[arg(long)]
    allow_write: Vec<PathBuf>,
    /// Command + args to run under the sandbox (everything after `--`).
    #[arg(last = true, num_args = 1..)]
    command: Vec<String>,
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
        /// Print the verifier command without executing it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Register repeated failing actions from a run.json (Reflexion).
    Failures {
        /// Path to the run file (evals/cases/<case>/run.json).
        run: PathBuf,
    },
    /// Verifier-strength audit (VERIFIABLE-REWARD-RESEARCH D): check the
    /// declared `verify_command` is not vacuous — it must PASS on the real
    /// target AND FAIL on an empty counterfactual target.
    VerifyAudit {
        /// Path to the run file (evals/cases/<case>/run.json).
        run: PathBuf,
    },
}

#[derive(Args, Debug)]
struct TicketArgs {
    #[command(subcommand)]
    action: TicketAction,
}

#[derive(Args, Debug)]
struct BacklogArgs {
    /// Map knowledge gaps (failure register + canonical memory) to
    /// research questions instead of eval-gap tickets.
    #[arg(long)]
    knowledge: bool,
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
    Status {
        /// Show rerun-attempt counts per case (pilot-before-scale).
        #[arg(long)]
        attempts: bool,
    },
    /// Pick the worst open case, claim it, and write its slice spec.
    Dispatch {
        /// Case name (default: worst open case below the target).
        case: Option<String>,
        /// Composite floor for dispatchability (default from
        /// `.miniagi.json` / `MINIAGI_TARGET_COMPOSITE`).
        #[arg(long)]
        below: Option<f64>,
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
        /// Close even without a declared deterministic verifier.
        #[arg(long)]
        allow_unverified: bool,
    },
    /// Bounded batch dispatch of the worst open gaps under a shared
    /// budget (hardening audit P2-11): verifiable, unclaimed,
    /// unblocked cases only.
    Objective {
        /// Max cases to dispatch in this objective.
        #[arg(long, default_value_t = 3)]
        max_cases: usize,
        /// Total cost budget in USD (stop when spent).
        #[arg(long)]
        budget_cost: Option<f64>,
        /// Claimant name.
        #[arg(long, default_value = "local")]
        claimant: String,
    },
    /// AFK verified-iteration supervisor: run a goal (or a case) in the
    /// background under the verified-iteration core, writing progress.md
    /// per attempt, a reviewable run report, and an optional on-done
    /// hook.
    Run {
        /// Goal text, or an existing case name (evals/cases/<name>).
        goal_or_case: String,
        /// Scratch workdir for the worker.
        #[arg(long, default_value = ".run")]
        workdir: PathBuf,
        /// Deterministic verifier command (required for ad-hoc goals;
        /// P0-3).
        #[arg(long)]
        verify: Option<String>,
        /// Verifier target dir (default: the workdir).
        #[arg(long)]
        target: Option<PathBuf>,
        /// Iteration count (default 3).
        #[arg(long, default_value_t = 3)]
        iterate: usize,
        /// On-done hook: shell command run with the report path +
        /// outcome as args.
        #[arg(long)]
        on_done: Option<String>,
        /// Run report path (default: workdir/REPORT.md).
        #[arg(long)]
        report: Option<PathBuf>,
        /// Blind-worker mode (EXP-012 isolation).
        #[arg(long)]
        blind_worker: bool,
        /// Hidden-suite dir (required with --blind-worker).
        #[arg(long)]
        hidden_dir: Option<PathBuf>,
        /// Wall cap per attempt in seconds.
        #[arg(long)]
        max_wall: Option<u64>,
        /// Idle cap per attempt in seconds (overrides config).
        #[arg(long)]
        max_idle: Option<u64>,
        /// Skip the Landlock sandbox.
        #[arg(long)]
        no_sandbox: bool,
        /// Disable session resume (AFK v2): always cold re-invoke.
        #[arg(long)]
        no_resume: bool,
        /// Loop template: "sequential-reviewer" (independent read-only
        /// review + one fix pass via the worker's session resume).
        #[arg(long)]
        template: Option<String>,
        /// Launch in the background (AFK v3): spawn the same run as a
        /// detached child, print the run handle, exit immediately.
        #[arg(long)]
        detach: bool,
    },
    /// Parallel-planner (AFK v4): decompose a goal into tickets
    /// (planner pass or --manifest), run them in PARALLEL detached
    /// worktrees, merge deterministically, and gate the result with
    /// the goal's protected verifier.
    Parallel {
        /// The goal text or an existing case name.
        goal_or_case: String,
        /// A pre-written planner manifest (skips the planner pass).
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Max parallel workers (conservative default).
        #[arg(long, default_value_t = 2)]
        max_parallel: usize,
        /// Iterations per ticket.
        #[arg(long, default_value_t = 3)]
        iterate: usize,
        /// Wall cap per ticket attempt.
        #[arg(long)]
        max_wall: Option<u64>,
        /// Skip the Landlock sandbox for workers.
        #[arg(long)]
        no_sandbox: bool,
        /// The final gate (ad-hoc goals; a case reuses its own
        /// `verify_command` as the protected final truth).
        #[arg(long)]
        verify: Option<String>,
    },
}
/// CLI fields for `loop run` (bundled so the command fn stays under
/// the clippy arg budget). The bools mirror the CLI flags one-to-one.
#[allow(clippy::struct_excessive_bools)]
struct LoopRunArgs {
    goal_or_case: String,
    workdir: PathBuf,
    verify: Option<String>,
    target: Option<PathBuf>,
    iterate: usize,
    on_done: Option<String>,
    report: Option<PathBuf>,
    blind_worker: bool,
    hidden_dir: Option<PathBuf>,
    max_wall: Option<u64>,
    max_idle: Option<u64>,
    no_sandbox: bool,
    no_resume: bool,
    template: Option<String>,
    detach: bool,
}

#[derive(Args, Debug)]
struct ResearchArgs {
    /// The research question (positional).
    question: String,
    /// Worker name (cheap default).
    #[arg(long, default_value = "opencode-opencode-go/deepseek-v4-flash")]
    worker: String,
    /// Wall cap per worker invocation, seconds.
    #[arg(long, default_value = "600")]
    max_wall: u64,
    /// Re-research even when findings already exist (registry dedup).
    #[arg(long)]
    force: bool,
    /// Full autoresearch chain: research -> distill -> audit -> promote
    /// in one call (the D2 loop as a single pipeline).
    #[arg(long)]
    chain: bool,
    /// Distiller worker for `--chain` (default opencode flash).
    #[arg(long, default_value = "opencode-opencode-go/deepseek-v4-flash")]
    distiller: String,
    /// Auditor worker for `--chain` (default opencode pro).
    #[arg(long, default_value = "opencode-opencode-go/deepseek-v4-pro")]
    auditor: String,
}

#[derive(Args, Debug)]
struct UiArgs {
    /// TCP port for the dashboard.
    #[arg(long, default_value = "8199")]
    port: u16,
}

#[derive(Args, Debug)]
struct DreamArgs {
    /// Distill + audit material into `memory/staging/` (default action).
    #[arg(long)]
    source: Option<PathBuf>,
    /// Distiller worker name (cheap model; default opencode flash).
    #[arg(long, default_value = "opencode-opencode-go/deepseek-v4-flash")]
    distiller: String,
    /// Auditor worker name (strong model; default opencode pro).
    #[arg(long, default_value = "opencode-opencode-go/deepseek-v4-pro")]
    auditor: String,
    /// Apply the latest staging manifest's verdicts into canonical.
    #[arg(long)]
    promote: bool,
    /// Idle trigger (D2): when the machine is idle (load1 below
    /// `--idle-load`), distill the newest run's report into staging.
    #[arg(long)]
    idle: bool,
    /// Load threshold for the idle trigger.
    #[arg(long, default_value = "0.8")]
    idle_load: f64,
    /// Report without writing anything.
    #[arg(long)]
    dry_run: bool,
    /// Wall cap per worker invocation, seconds. When unset, the cap
    /// scales with the material size (cycle 33 finding: the fixed 300 s
    /// default was too small for large reports — a 105 KB material
    /// stalled the audit and needed ~900 s).
    #[arg(long)]
    max_wall: Option<u64>,
}

#[derive(Args, Debug)]
struct StatusArgs {
    /// Machine-readable JSON output (D6 run-state index surface).
    #[arg(long)]
    json: bool,
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
    /// Run a skill's verify hook (or ALL with --all); exit non-zero on
    /// failure.
    Verify {
        /// Skill name (ignored with --all).
        name: Option<String>,
        /// Mark the skill disabled in its frontmatter when the hook
        /// fails (HARDENING P2-14): a broken dynamic skill must not
        /// keep running silently.
        #[arg(long)]
        disable_on_fail: bool,
        /// Verify EVERY skill's hook (the gate's skills step).
        #[arg(long)]
        all: bool,
    },
    /// Verify EVERY skill's hook in one pass (the deterministic gate's
    /// skills step): a failed hook or a hookless PROCEDURAL skill makes
    /// the exit non-zero (`type: mode` skills are exempt).
    VerifyAll,
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
    /// Verifier-vs-judged drift: precision of the judged outcome
    /// against the deterministic layer (Phase 9 slice 2).
    JudgeDrift,
    /// Reset the judge-calibration corpus (derived `calibration.md`).
    /// `loop verify` abstains (blocks close) while judge precision is
    /// below `min_judge_precision`; after fixing the verifier/judge the
    /// stale disagreement rows are cleared so close can resume
    /// (cycle-33 finding: abstention needs a supported recalibration
    /// path, not a repo-wide permanent freeze).
    JudgeRecalibrate,
    /// Score held-out cases in `evals/hidden/` (not in the baseline,
    /// not gated) — contamination-safe capability measurement.
    Hidden {
        /// Subdirectory under evals/hidden (default: all).
        dir: Option<String>,
    },
    /// Regression gate over all cases vs the committed baseline.
    Gate {
        /// Max allowed composite drop (default from .miniagi.json /
        /// `MINIAGI_REGRESSION_TOLERANCE`).
        #[arg(long)]
        tolerance: Option<f64>,
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
    /// Domain/keyword retrieval over canonical facts (hardening audit
    /// C.7): load only the relevant fragment instead of the whole brief.
    Query {
        /// Keyword to filter facts by (substring, case-insensitive).
        keyword: Option<String>,
        /// Restrict to one domain.
        #[arg(long)]
        domain: Option<String>,
        /// Print raw (id, domain, body) triples instead of rendered lines.
        #[arg(long)]
        raw: bool,
        /// Token-budgeted selective retrieval (D3): keep only the top
        /// facts by enforced/link/recency score within this many chars.
        #[arg(long)]
        budget: Option<usize>,
    },
    /// Supersede one or more canonical facts (D3): a NEW fact whose entry
    /// records the soft-deleted lineage (`- supersedes: <id,...>`). The
    /// superseded facts stay on disk; the derived views stop showing them.
    Supersede {
        /// Body of the superseding fact.
        body: String,
        /// 16-hex fact ids being superseded (repeatable).
        #[arg(long = "supersedes", value_delimiter = ',')]
        supersedes: Vec<String>,
        /// Domain assigned to the new fact.
        #[arg(long, default_value = "general")]
        domain: String,
        /// Provenance source string for the entry.
        #[arg(long, default_value = "mem supersede")]
        source: String,
    },
    /// Append fact ids to the preservation list
    /// (`memory/canonical/preserved.md`) — load-bearing facts exempt
    /// from merge/supersede (D3).
    Preserve {
        /// 16-hex fact ids to preserve (repeatable).
        ids: Vec<String>,
    },
    /// Remove fact ids from the preservation list — the counterpart to
    /// `preserve`: supersede of a preserved id is refused, so a wrongly
    /// preserved id must be un-preserved before it can be superseded.
    Unpreserve {
        /// 16-hex fact ids to remove from the preservation list
        /// (repeatable).
        ids: Vec<String>,
    },
    /// Dedup + lineage integrity gate (D3): exact-duplicate bodies with
    /// different ids, supersede refs to unknown ids, preserved ids that
    /// do not exist. Exits non-zero on findings.
    Verify,
}

#[derive(Args, Debug)]
struct DeriveArgs {
    /// Skip per-domain fragment regeneration.
    #[arg(long)]
    brief_only: bool,
    /// Write a named snapshot of the derived views (canonical + brief
    /// hashes) — the deterministic-materialization reference.
    #[arg(long)]
    snapshot: Option<String>,
    /// Regenerate and verify against a named snapshot (MATCH /
    /// DIVERGENT).
    #[arg(long)]
    replay: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Production-readiness B.4: first-run auto-init of the data-dir
    // skeleton (best-effort — a missing layout must not block an
    // otherwise-working command, and an existing repo is a no-op).
    let _ = init::bootstrap(&root());
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
            MemAction::Query {
                keyword,
                domain,
                raw,
                budget,
            } => cmd_mem_query(keyword.as_deref(), domain.as_deref(), raw, budget),
            MemAction::Supersede {
                body,
                supersedes,
                domain,
                source,
            } => cmd_mem_supersede(&body, &supersedes, &domain, &source),
            MemAction::Preserve { ids } => cmd_mem_preserve(&ids),
            MemAction::Unpreserve { ids } => cmd_mem_unpreserve(&ids),
            MemAction::Verify => cmd_mem_verify(),
        },
        Command::Derive(DeriveArgs {
            brief_only,
            snapshot,
            replay,
        }) => cmd_derive(brief_only, snapshot.as_deref(), replay.as_deref()),
        Command::Provenance => {
            let root = root();
            println!("canonical_sha256: {}", memory::canonical_fingerprint(&root));
            ExitCode::SUCCESS
        }
        Command::Eval(EvalArgs { action }) => match action {
            EvalAction::Score { run } => cmd_eval_score(&run),
            EvalAction::Steps { run } => cmd_eval_steps(&run),
            EvalAction::JudgeDrift => cmd_eval_judge_drift(),
            EvalAction::JudgeRecalibrate => cmd_eval_judge_recalibrate(),
            EvalAction::Hidden { dir } => cmd_eval_hidden(dir.as_deref()),
            EvalAction::Gate {
                tolerance,
                mismatch_tolerance,
                write_baseline,
            } => {
                let tolerance = tolerance.unwrap_or_else(|| {
                    mini_agi_core::config::Config::load(&root()).regression_tolerance
                });
                cmd_eval_gate(tolerance, mismatch_tolerance, write_baseline)
            }
            EvalAction::Mismatches { run } => cmd_eval_mismatches(run.as_deref()),
        },
        Command::Skill(SkillArgs { action }) => match action {
            SkillAction::List => cmd_skill_list(),
            SkillAction::Show { name } => cmd_skill_show(&name),
            SkillAction::Verify {
                name,
                disable_on_fail,
                all,
            } => {
                if all {
                    cmd_skill_verify_all()
                } else if let Some(name) = name {
                    cmd_skill_verify(&name, disable_on_fail)
                } else {
                    fail("skill verify needs a name or --all")
                }
            }
            SkillAction::VerifyAll => cmd_skill_verify_all(),
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
            RunAction::Verify { run, dry_run } => cmd_run_verify(&run, dry_run),
            RunAction::VerifyAudit { run } => cmd_run_verify_audit(&run),
            RunAction::Failures { run } => cmd_run_failures(&run),
        },
        Command::Insights => cmd_insights(),
        Command::Backlog(BacklogArgs { knowledge }) => {
            if knowledge {
                cmd_backlog_knowledge()
            } else {
                cmd_backlog()
            }
        }
        Command::Resume => cmd_resume(),
        Command::Health => cmd_health(),
        Command::Audit => cmd_audit(),
        Command::Status(StatusArgs { json }) => cmd_status(json),
        Command::Research(ResearchArgs {
            question,
            worker,
            max_wall,
            force,
            chain,
            distiller,
            auditor,
        }) => cmd_research(
            &question, &worker, max_wall, force, chain, &distiller, &auditor,
        ),
        Command::Ui(UiArgs { port }) => match ui::serve(&root(), port) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("ui: {e}")),
        },
        Command::Dream(DreamArgs {
            source,
            distiller,
            auditor,
            promote,
            dry_run,
            max_wall,
            idle,
            idle_load,
        }) => cmd_dream(
            source.as_deref(),
            &distiller,
            &auditor,
            promote,
            dry_run,
            max_wall,
            idle.then_some(idle_load),
        ),
        Command::Codex(CodexArgs {
            spec,
            workdir,
            run_out,
            verify,
            target,
            reparse_log,
            max_wall,
            max_steps,
            no_sandbox,
            worker_name,
            approve,
            iterate,
            blind_worker,
            hidden_dir,
        }) => reparse_log.map_or_else(
            || {
                worker::cmd_codex(&worker::CodexRunArgs {
                    spec: &spec,
                    workdir: &workdir,
                    run_out: run_out.as_deref(),
                    verify: verify.as_deref(),
                    target: target.as_deref(),
                    max_wall,
                    max_steps,
                    no_sandbox,
                    worker_name,
                    approve,
                    iterate,
                    blind_worker,
                    hidden_dir,
                })
            },
            |log| {
                worker::cmd_codex_reparse(
                    &log,
                    &workdir,
                    run_out.as_deref(),
                    verify.as_deref(),
                    target.as_deref(),
                )
            },
        ),
        Command::ExecSandbox(ExecSandboxArgs {
            allow_write,
            command,
        }) => worker::cmd_exec_sandbox(&allow_write, &command),
        Command::Harness(HarnessArgs { action }) => match action {
            HarnessAction::Snapshot => cmd_harness(),
            HarnessAction::Verify {
                target,
                candidate,
                claims,
            } => cmd_harness_verify(&target, &candidate, claims.as_deref()),
        },
        Command::Loop(LoopArgs { action }) => match action {
            LoopAction::Status { attempts } => cmd_loop_status(attempts),
            LoopAction::Dispatch {
                case,
                below,
                claimant,
            } => {
                let below = below.unwrap_or_else(|| {
                    mini_agi_core::config::Config::target_composite_for(&root())
                });
                cmd_loop_dispatch(case.as_deref(), below, &claimant)
            }
            LoopAction::Verify {
                case,
                claimant,
                allow_unverified,
            } => cmd_loop_verify(&case, &claimant, allow_unverified),
            LoopAction::Objective {
                max_cases,
                budget_cost,
                claimant,
            } => cmd_loop_objective(max_cases, budget_cost, &claimant),
            LoopAction::Run {
                goal_or_case,
                workdir,
                verify,
                target,
                iterate,
                on_done,
                report,
                blind_worker,
                hidden_dir,
                max_wall,
                max_idle,
                no_sandbox,
                no_resume,
                template,
                detach,
            } => cmd_loop_run(&LoopRunArgs {
                goal_or_case,
                workdir,
                verify,
                target,
                iterate,
                on_done,
                report,
                blind_worker,
                hidden_dir,
                max_wall,
                max_idle,
                no_sandbox,
                no_resume,
                template,
                detach,
            }),
            LoopAction::Parallel {
                goal_or_case,
                manifest,
                max_parallel,
                iterate,
                max_wall,
                no_sandbox,
                verify,
            } => cmd_loop_parallel(
                &goal_or_case,
                manifest.as_deref(),
                max_parallel,
                iterate,
                max_wall,
                no_sandbox,
                verify.as_deref(),
            ),
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
            ExitCode::from(mini_agi_core::health::exit_code_for(report.verdict()))
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

fn cmd_harness_verify(target: &Path, candidate: &Path, claims: Option<&str>) -> ExitCode {
    let root = root();
    match mini_agi_core::harness::verify_candidate(&root, target, candidate, claims) {
        Ok(verdict) => {
            println!("{verdict}");
            if verdict.starts_with("ACCEPT") {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => fail(&format!("harness verify: {e}")),
    }
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

fn cmd_loop_status(attempts: bool) -> ExitCode {
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
                let best = row
                    .best_composite
                    .map_or_else(|| "-".to_string(), |b| format!("best {b:.4}"));
                let signal = row
                    .repair_signal
                    .map_or_else(String::new, |s| format!(" [{s}]"));
                let exhaust = if row.exhausted {
                    "  EXHAUSTED — needs human (retry bound hit, best below target)"
                } else {
                    ""
                };
                if attempts {
                    println!(
                        "  {:.4}  {:<24} attempts={}  {}  {}  lease: {}  {}{}{}",
                        row.composite,
                        row.case,
                        row.attempts,
                        best,
                        rerun,
                        ticket,
                        claim,
                        signal,
                        exhaust
                    );
                } else {
                    println!(
                        "  {:.4}  {:<24} {}  {}  lease: {}  {}{}",
                        row.composite, row.case, best, rerun, claim, signal, exhaust
                    );
                }
            }
            if attempts {
                println!(
                    "  (Ringelmann 2606.02646: a 5-attempt pilot predicts the N=30 ceiling — before scaling retries, compare attempts vs gains)"
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
        Err(e) => {
            // No-progress guard (autoresearch wiring): when dispatch has
            // no real work — every gap closed by rerun, past its retry
            // bound, or leased — that is a STOP signal (exit 0) with a
            // report, not a generic error. The loop consuming this
            // command stops instead of auto-continuing into nothing.
            if let Some(reason) = mini_agi_core::loopcmd::dispatch_no_work(&root(), below) {
                println!("loop dispatch: STOP — {reason}");
                return ExitCode::SUCCESS;
            }
            fail(&format!("loop dispatch: {e}"))
        }
    }
}

fn cmd_loop_verify(case: &str, claimant: &str, allow_unverified: bool) -> ExitCode {
    match mini_agi_core::loopcmd::verify(&root(), case, claimant, allow_unverified) {
        Ok((text, closed)) => {
            println!("{text}");
            if closed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        // P2-13 (hardening audit): an error is a distinct terminal
        // signal (2) from an honest OPEN (1) — a consumer can tell
        // "the gap stays open" from "the verification machinery broke".
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn cmd_loop_objective(max_cases: usize, budget_cost: Option<f64>, claimant: &str) -> ExitCode {
    match mini_agi_core::loopcmd::objective(&root(), max_cases, claimant, budget_cost) {
        Ok(plan) => {
            println!(
                "loop objective: dispatched {} case(s) under max_cases={max_cases}",
                plan.dispatched.len()
            );
            for d in &plan.dispatched {
                println!(
                    "  dispatched: {} -> {} (spec: {})",
                    d.case,
                    d.ticket,
                    d.spec.display()
                );
            }
            for c in &plan.skipped_no_verifier {
                println!("  skipped (no verifier, P0-3): {c}");
            }
            for c in &plan.skipped_blocked {
                println!("  skipped (blocked by an open ticket): {c}");
            }
            for c in &plan.skipped_unavailable {
                println!("  skipped (run.json unreadable): {c}");
            }
            for c in &plan.skipped_exhausted {
                println!("  skipped (EXHAUSTED — rerun bound hit, needs human): {c}");
            }
            match plan.budget_cost {
                Some(b) => println!(
                    "  budget: ${:.2} / ${b:.2} spent ({})",
                    plan.budget_spent,
                    if plan.budget_spent >= b {
                        "STOPPED"
                    } else {
                        "within"
                    }
                ),
                None => println!("  budget: none (${:.2} declared cost)", plan.budget_spent),
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("loop objective: {e}")),
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

/// Knowledge-backlog question generation (pure): combine repeated
/// failure reflections with stalled registry questions into deduplicated
/// `(slug, question)` pairs. Pure so the mapping is unit-testable
/// without a filesystem.
fn knowledge_questions(
    failures: &[mini_agi_core::failure::FailureEntry],
    registry: &[research_registry::RegistryEntry],
) -> Vec<(String, String)> {
    let mut questions: Vec<(String, String)> = Vec::new();
    for e in failures {
        if e.count < 2 || e.reflection.as_deref().unwrap_or("").trim().is_empty() {
            continue;
        }
        let question = format!(
            "How do production agent kernels prevent the repeated failure '{}' (tool: {}, {}x) — what guardrail or planning mechanism encodes the fix the reflection names?",
            e.action.chars().take(60).collect::<String>(),
            e.tool,
            e.count
        );
        let slug = research::slugify(&question);
        if !questions.iter().any(|(s, _)| s == &slug) {
            questions.push((slug, question));
        }
    }
    for e in registry {
        let stalled = matches!(
            e.status,
            research_registry::QuestionStatus::Distilled
                | research_registry::QuestionStatus::Findings
        );
        if stalled && !questions.iter().any(|(s, _)| *s == e.slug) {
            questions.push((e.slug.clone(), e.question.clone()));
        }
    }
    questions.sort();
    questions.dedup_by(|a, b| a.0 == b.0);
    questions
}

/// `backlog --knowledge`: map knowledge gaps to research questions.
///
/// The knowledge layer is the loop's missing signal — eval gaps are
/// ticket-backed (ADR-0005) but a repeated failure pattern or a stale
/// research question is not. This surfaces the research backlog from
/// two registers:
///   1. `failure` — repeated failing actions (FM-1.3 etc.) whose
///      reflection names a fix the codebase does not yet encode;
///   2. `research/registry.json` — questions in `distilled` or
///      `findings` state (audited but never promoted/decided).
///
/// Output is a suggested question per gap, deduplicated by slug, ready
/// to feed `mini-agi research --chain` — never a second research file
/// for a question the registry already holds.
fn cmd_backlog_knowledge() -> ExitCode {
    let root = root();
    let failures = mini_agi_core::failure::read_register(&root).unwrap_or_default();
    let registry = research_registry::load_registry(&root);
    let questions = knowledge_questions(&failures, &registry);
    if questions.is_empty() {
        println!("no knowledge gaps — research backlog is clear");
        return ExitCode::SUCCESS;
    }
    println!(
        "knowledge backlog: {} research question(s) — feed via `mini-agi research --chain \"<q>\"`",
        questions.len()
    );
    for (slug, q) in &questions {
        println!("  {slug}: {q}");
    }
    ExitCode::SUCCESS
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

fn cmd_run_verify_audit(run: &Path) -> ExitCode {
    match mini_agi_core::verifier::audit_verifier(run) {
        Ok(text) => {
            println!("{text}");
            let vacuous = text.contains("VACUOUS");
            if vacuous {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => fail(&format!("verify-audit: {e}")),
    }
}

fn cmd_run_verify(run: &Path, dry_run: bool) -> ExitCode {
    let root = root();
    if dry_run {
        let text = match std::fs::read_to_string(run) {
            Ok(t) => t,
            Err(e) => return fail(&format!("cannot read {}: {e}", run.display())),
        };
        let parsed: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => return fail(&format!("invalid run json: {e}")),
        };
        let cmd = parsed["verify_command"].as_str().unwrap_or("");
        let target = parsed["verify_target"].as_str().unwrap_or("");
        println!("dry-run: would execute '{cmd}' in '{target}' (no execution, no attribution)");
        return ExitCode::SUCCESS;
    }
    match mini_agi_core::verifier::verify_run(&root, run) {
        Ok(v) => {
            if let Err(e) = mini_agi_core::verifier::append_calibration(
                &root,
                &mini_agi_core::verifier::CalibrationRow {
                    at: mini_agi_core::memory::utc_now_stamp(),
                    case: v.case.clone(),
                    status: v.status.clone(),
                    claimed: v.claimed,
                    composite: 0.0,
                    exit: v.exit_code,
                    command: v.command.clone(),
                    target: v.target.clone(),
                },
            ) {
                eprintln!("warning: calibration row not persisted — {e}");
            }
            if let (Some(command), Some(target)) = (&v.command, &v.target)
                && let Err(e) = mini_agi_core::verifier::append_attribution(
                    &root,
                    &mini_agi_core::verifier::VerifyAttribution {
                        at: mini_agi_core::memory::utc_now_stamp(),
                        case: v.case.clone(),
                        command: command.clone(),
                        target: target.clone(),
                        status: v.status.clone(),
                        run_sha256: std::fs::read(run).ok().map(|bytes| {
                            mini_agi_core::hash::source_sha256_bytes(&bytes)
                        }),
                    },
                )
            {
                eprintln!("warning: attribution not persisted — {e}");
            }
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
    match mini_agi_core::failure::analyze_run(run, &root) {
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
            let drift = mini_agi_core::verifier::judge_drift(&root());
            if drift.total > 0 {
                let precision = drift.precision();
                if precision.is_nan() {
                    println!(
                        "  judge drift: {} verifications, {} disagreements (no claimed successes)",
                        drift.total, drift.disagreements
                    );
                } else {
                    println!(
                        "  judge drift: {} verifications, {} disagreements — precision {:.1}%",
                        drift.total,
                        drift.disagreements,
                        precision * 100.0
                    );
                }
            }
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
        "  Skills list:     {} chars for {} skills ({}% of 2% budget)",
        report.skills_list_bytes, report.skills_count, report.skills_pct_of_budget
    );
    if report.skills_over_budget {
        println!("  WARN: skills list exceeds 2% budget");
    }
    println!(
        "  Memory leverage: canonical {}B -> brief {}B (x{:.2} canonical size — {})",
        report.canonical_bytes,
        report.brief_bytes,
        report.leverage_ratio,
        if report.brief_bytes > memory::MAX_BRIEF_BYTES as u64 {
            format!(
                "brief exceeds the {}B working-set cap",
                memory::MAX_BRIEF_BYTES
            )
        } else {
            "compression into working set".to_string()
        }
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
                if skill.disabled {
                    println!("{}  [{hook}][disabled]  {}", skill.name, skill.description);
                } else {
                    println!("{}  [{hook}]  {}", skill.name, skill.description);
                }
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

fn cmd_skill_verify(name: &str, disable_on_fail: bool) -> ExitCode {
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
                    // HARDENING P2-14: persist the disabled state so a
                    // broken dynamic skill cannot keep running silently.
                    if disable_on_fail {
                        match skills::set_disabled(&root, name, true) {
                            Ok(()) => eprintln!("skill {name} marked DISABLED (persisted)"),
                            Err(e) => eprintln!("warning: could not mark {name} disabled — {e}"),
                        }
                    }
                    ExitCode::from(1)
                }
            }
            Err(e) => fail(&e.to_string()),
        },
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_skill_verify_all() -> ExitCode {
    let root = root();
    match skills::verify_all_skills(&root) {
        Ok(report) => {
            for name in &report.passed {
                println!("PASS: {name}");
            }
            for (name, code) in &report.failed {
                eprintln!("FAIL: {name} (exit {code})");
            }
            println!(
                "skills: {} passed, {} failed, {} without hooks (reported)",
                report.passed.len(),
                report.failed.len(),
                report.no_hook.len()
            );
            if !report.no_hook.is_empty() {
                println!("  no-hook: {}", report.no_hook.join(", "));
            }
            if !report.no_hook.is_empty() {
                eprintln!(
                    "FAIL: procedural skills without a verify hook: {}",
                    report.no_hook.join(", ")
                );
            }
            if !report.no_version.is_empty() {
                eprintln!(
                    "FAIL: skills missing version/source frontmatter: {}",
                    report.no_version.join(", ")
                );
            }
            if !report.lint_failed.is_empty() {
                eprintln!(
                    "FAIL: skills failing the structural lint (no checkable criteria marker): {}",
                    report.lint_failed.join(", ")
                );
            }
            let drift = skills::dual_registration_drift(&root);
            if !drift.drifted.is_empty() {
                eprintln!("FAIL: dual-registration DRIFT (local != global content):");
                for (name, lh, gh) in &drift.drifted {
                    eprintln!("    {name}: local {lh} != global {gh}");
                }
            }
            if report.failed.is_empty()
                && report.no_version.is_empty()
                && report.lint_failed.is_empty()
                && report.no_hook.is_empty()
                && drift.drifted.is_empty()
            {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => fail(&format!("skill verify --all: {e}")),
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
            // Error-budget audit (cycle-33 Flat Score pattern): report the
            // per-channel failure counts and the success-at-budget
            // projection so an end-of-run score cannot hide a run whose
            // per-step reliability is degraded.
            let audit = eval::error_budget_audit(&run);
            let budget_line: Vec<String> = audit
                .success_at_budget
                .iter()
                .enumerate()
                .map(|(k, ok)| format!("{k}:{}", if *ok { "ok" } else { "fail" }))
                .collect();
            println!(
                "  error budget: {} steps, {} failed (dedup), {} gate-fail, {} goal-drift, {} reverted (by tool: {:?})",
                audit.total_steps,
                audit.failed_steps,
                audit.failed_gate_steps,
                audit.goal_drift_steps,
                audit.reverted_steps,
                audit.failed_by_tool
            );
            println!(
                "  success at budget k (k: status): {}",
                budget_line.join(" ")
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

fn cmd_eval_hidden(dir: Option<&str>) -> ExitCode {
    let root = root();
    let hidden = root.join("evals/hidden");
    // Path safety: a raw `join` would let `eval hidden --dir ../x`
    // read outside evals/hidden/.
    let scan_dir = match dir {
        Some(d) if crate::status::plain_path_segment(d) => hidden.join(d),
        Some(_) => return fail("eval hidden: --dir must be a plain name (no separators)"),
        None => hidden,
    };
    if !scan_dir.is_dir() {
        return fail(&format!("no hidden cases in {}", scan_dir.display()));
    }
    let mut composites = Vec::new();
    let Ok(entries_dir) = std::fs::read_dir(&scan_dir) else {
        return fail(&format!("cannot read {}", scan_dir.display()));
    };
    for entry in entries_dir.flatten() {
        let run = entry.path().join("run.json");
        if !run.is_file() {
            continue;
        }
        let case = entry.file_name().to_string_lossy().into_owned();
        match eval::score_run(&run, &root, &root.join("evals/golden")) {
            Ok(report) => {
                println!("hidden {case}: {:.4}", report.composite);
                composites.push((case, report.composite));
            }
            Err(e) => println!("hidden {case}: error {e}"),
        }
    }
    if composites.is_empty() {
        return fail(&format!("no run.json files under {}", scan_dir.display()));
    }
    let avg = composites.iter().map(|(_, c)| c).sum::<f64>()
        / f64::from(u32::try_from(composites.len()).unwrap_or(1));
    println!(
        "hidden avg: {avg:.4} across {} cases (not gated, not in baseline)",
        composites.len()
    );
    ExitCode::SUCCESS
}

fn cmd_eval_judge_drift() -> ExitCode {
    let drift = mini_agi_core::verifier::judge_drift(&root());
    let precision = drift.precision();
    println!(
        "judge drift: {} verifications, {} disagreements",
        drift.total, drift.disagreements
    );
    for case in &drift.disagreement_cases {
        println!(
            "  DISAGREEMENT CASE: {case} — the judged outcome disagreed with the verifier (red-team signal)"
        );
    }
    println!(
        "  claimed successes: {} — verified by the deterministic layer: {}",
        drift.claimed_successes, drift.verified_successes
    );
    if precision.is_nan() {
        println!("  precision: n/a (no claimed successes recorded)");
    } else {
        println!("  verifier-vs-judged precision: {:.1}%", precision * 100.0);
        if precision < 1.0 {
            println!(
                "  SIGNAL: the judged outcome overstates success — calibration data is accumulating"
            );
        }
    }
    ExitCode::SUCCESS
}

fn cmd_eval_judge_recalibrate() -> ExitCode {
    // Reset the derived calibration corpus so the judge-abstention gate
    // can resume after the verifier/judge is fixed (cycle-33 review F3:
    // abstention needs a supported recalibration path, not a permanent
    // repo-wide freeze on any single disagreement).
    match mini_agi_core::verifier::reset_calibration(&root()) {
        Ok(()) => {
            println!("judge calibration reset — corpus cleared; close gates resume");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("judge recalibrate: {e}")),
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
    if entries.is_empty() && baseline.is_empty() {
        // A fresh repo has nothing to regress: the gate passes trivially,
        // so an init'd repo is gate-green from the first commit.
        return Ok("PASS: 0 cases, 0 regressions — no cases in evals/cases/".to_string());
    }
    let result = eval::run_gate(&entries, &baseline, tolerance, mismatch_tolerance);
    let mut lines = result.messages.clone();
    let verdict = if result.failures == 0 { "PASS" } else { "FAIL" };
    lines.push(format!(
        "{verdict}: {} cases, {} regressions",
        result.case_count, result.failures
    ));
    // Capability telemetry (Phase 9 slice 4): per-family composite
    // averages (Sequoia/Karpathy compounding discipline — the gate is
    // the time series, per family, not just all-green-or-red).
    let mut by_family: std::collections::BTreeMap<String, (f64, usize)> =
        std::collections::BTreeMap::new();
    for entry in &entries {
        let fam = eval::family_of(&entry.case);
        let cell = by_family.entry(fam).or_insert((0.0, 0));
        cell.0 += entry.composite;
        cell.1 += 1;
    }
    for (fam, (sum, count)) in &by_family {
        lines.push(format!(
            "family {fam}: avg {:.4} ({count} cases)",
            sum / f64::from(u32::try_from(*count).unwrap_or(1))
        ));
    }
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
        domain: domain.trim().to_lowercase(),
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

fn cmd_mem_query(
    keyword: Option<&str>,
    domain: Option<&str>,
    raw: bool,
    budget: Option<usize>,
) -> ExitCode {
    if keyword.is_none() && domain.is_none() && budget.is_none() {
        return fail("mem query: give a keyword and/or --domain to filter by");
    }
    let facts = memory::query_facts(&root(), domain, keyword);
    let facts = budget.map_or(facts, |budget_chars| {
        let all = memory::read_facts(&root());
        let links = memory::fact_links(&all);
        let enforced = memory::enforced_fact_ids(&root());
        memory::select_budgeted(&all, &links, &enforced, budget_chars)
    });
    // Relevance-ranked by query_facts/select_budgeted; no id re-sort.
    if facts.is_empty() {
        println!("no facts match (domain={domain:?}, keyword={keyword:?})");
        return ExitCode::from(1);
    }
    if raw {
        for (id, d, body) in &facts {
            println!("{id} [{d}] {body}");
        }
    } else {
        for (id, d, body) in &facts {
            println!("- `{id}` ({d}) {body}");
        }
    }
    println!("{} fact(s) matched", facts.len());
    ExitCode::SUCCESS
}

fn cmd_mem_supersede(body: &str, supersedes: &[String], domain: &str, source: &str) -> ExitCode {
    let root = root();
    if body.trim().is_empty() {
        return fail("mem supersede: the superseding body must not be empty");
    }
    if supersedes.is_empty() {
        return fail("mem supersede: give at least one --supersedes <16-hex id>");
    }
    let known = memory::existing_fact_ids(&root);
    for id in supersedes {
        if !known.contains(id) {
            return fail(&format!(
                "mem supersede: {id} is not a known canonical fact id"
            ));
        }
    }
    let h = mini_agi_core::hash::fact_id(body);
    let entry = match memory::write_supersede_entry(
        &root,
        &[(body.to_string(), h.clone())],
        source,
        domain,
        supersedes,
    ) {
        Ok(e) => e,
        Err(MemoryError::Io(e)) => return fail(&format!("mem supersede: {e}")),
        Err(MemoryError::PreservedId(id)) => {
            return fail(&format!(
                "mem supersede: {id} is preserved (load-bearing) — supersede it only after removing the preserve, or keep the old fact as-is"
            ));
        }
        Err(_) => return fail("mem supersede: unexpected memory error"),
    };
    let rel = entry.path.strip_prefix(&root).unwrap_or(&entry.path);
    println!(
        "superseded {} fact(s) with `{h}` ({domain})\nentry: {}",
        supersedes.len(),
        rel.display()
    );
    ExitCode::SUCCESS
}

fn cmd_mem_preserve(ids: &[String]) -> ExitCode {
    let root = root();
    let known = memory::existing_fact_ids(&root);
    for id in ids {
        if !known.contains(id) {
            return fail(&format!(
                "mem preserve: {id} is not a known canonical fact id"
            ));
        }
    }
    match memory::preserve_ids(&root, ids) {
        Ok(list) => {
            println!("preserved {} fact(s): {}", ids.len(), list.display());
            ExitCode::SUCCESS
        }
        Err(MemoryError::Io(e)) => fail(&format!("mem preserve: {e}")),
        Err(_) => fail("mem preserve: unexpected memory error"),
    }
}

fn cmd_mem_unpreserve(ids: &[String]) -> ExitCode {
    let root = root();
    match memory::unpreserve_ids(&root, ids) {
        Ok(n) => {
            println!("un-preserved {n} fact(s)");
            ExitCode::SUCCESS
        }
        Err(MemoryError::Io(e)) => fail(&format!("mem unpreserve: {e}")),
        Err(_) => fail("mem unpreserve: unexpected memory error"),
    }
}

fn cmd_mem_verify() -> ExitCode {
    let root = root();
    let findings = memory::integrity_findings(&root);
    if findings.is_empty() {
        println!("mem verify: OK — no duplicates, no broken lineage, preservation intact");
        ExitCode::SUCCESS
    } else {
        for f in &findings {
            println!("[finding] {f}");
        }
        println!("mem verify: {} finding(s)", findings.len());
        ExitCode::from(1)
    }
}

fn signoff_text(queue: &Path, index: usize, domain: &str, root: &Path) -> Result<String, String> {
    match memory::signoff(root, queue, index, &domain.trim().to_lowercase()) {
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

fn cmd_derive(brief_only: bool, snapshot: Option<&str>, replay: Option<&str>) -> ExitCode {
    if let Some(name) = replay {
        return match memory::replay(&root(), name) {
            Ok(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("derive replay: {e}")),
        };
    }
    if let Some(name) = snapshot {
        return match memory::snapshot(&root(), name) {
            Ok(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("derive snapshot: {e}")),
        };
    }
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
        Ok((in_brief, total, fragments)) => Ok(format!(
            "derived: context-brief.md ({in_brief}/{total} facts)\nderived: {fragments} per-domain fragments"
        )),
        Err(MemoryError::NoCanonical) => {
            Err("no canonical facts yet — run ingest first".to_string())
        }
        Err(MemoryError::Io(e)) => Err(format!("derive failed: {e}")),
        Err(_) => Err("unexpected memory error".to_string()),
    }
}

fn cmd_loop_run(a: &LoopRunArgs) -> ExitCode {
    use mini_agi_core::config::Config;
    let resolved = match supervisor::resolve(&supervisor::ResolveInput {
        goal_or_case: &a.goal_or_case,
        root: &root(),
        workdir: &a.workdir,
        verify: a.verify.as_deref(),
        target: a.target.as_deref(),
    }) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    if a.blind_worker && a.hidden_dir.is_none() {
        return fail("--blind-worker requires --hidden-dir");
    }
    if let Some(t) = &a.template
        && t != "sequential-reviewer"
    {
        return fail(&format!(
            "unknown template '{t}' (supported: sequential-reviewer)"
        ));
    }
    let cfg = Config::load(&a.workdir);
    // Detached launch (AFK v3): validate in the parent (resolution +
    // pairing), then spawn the same run as a child and exit with the
    // handle. The child runs the NORMAL path below.
    if a.detach {
        // The RESOLVED target (a case's verify_target honored) — not
        // the raw flag (codex review F1).
        let vt = resolved.target.as_path();
        let verify_cmd = match crate::supervisor::resolve_verify_cmd(
            a.verify.as_deref(),
            resolved.verify_cmd.clone(),
        ) {
            Ok(v) => v,
            Err(e) => return fail(&e),
        };
        let report_default = a.workdir.join("REPORT.md");
        let report = a.report.as_deref().unwrap_or(&report_default);
        match bg::spawn_detached(
            &a.goal_or_case,
            &a.workdir,
            &verify_cmd,
            vt,
            a.iterate,
            a.max_wall,
            a.max_idle,
            a.blind_worker,
            a.hidden_dir.as_deref(),
            a.on_done.as_deref(),
            report,
            a.template.as_deref(),
            a.no_resume,
            a.no_sandbox,
        ) {
            Ok(handle) => {
                println!("detached run launched: {}", handle.display());
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("cannot launch detached run: {e}")),
        }
    } else {
        let case_run_out = (root()
            .join("evals/cases")
            .join(&a.goal_or_case)
            .join("run.json")
            .is_file())
        .then(|| {
            root()
                .join("evals/cases")
                .join(&a.goal_or_case)
                .join("run.json")
        });
        let supervisor_args = supervisor::SupervisorArgs {
            spec_text: &resolved.spec_text,
            goal: &resolved.goal,
            scope_list: &resolved.scope_list,
            verify: &resolved.verify_cmd,
            target: &resolved.target,
            workdir: &a.workdir,
            iterate: a.iterate,
            blind_worker: a.blind_worker,
            hidden_dir: a.hidden_dir.as_deref(),
            wall_cap: a.max_wall.or(cfg.max_wall_seconds),
            max_idle: a.max_idle,
            step_cap: cfg.max_steps,
            no_sandbox: a.no_sandbox,
            worker_name: "codex",
            read_only: false,
            on_done: a.on_done.as_deref(),
            report: a.report.as_deref(),
            run_out: case_run_out.as_deref(),
            resume: a.iterate.max(1) > 1 && !a.no_resume,
            template: a.template.as_deref(),
        };
        finish_loop_run(&supervisor_args)
    }
}

fn finish_loop_run(args: &supervisor::SupervisorArgs<'_>) -> ExitCode {
    match supervisor::run(args) {
        Ok(result) => {
            println!(
                "supervised run: attempts={} verifier_passed={} final={} ({})",
                result.iteration.attempts_done,
                result.iteration.verifier_passed,
                result.final_passed,
                result.final_reason
            );
            println!("  progress: {}", result.progress_path.display());
            println!("  report: {}", result.report_path.display());
            if result.iteration.aborted {
                ExitCode::from(3)
            } else if result.final_passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => fail(&format!("loop run: {e}")),
    }
}

/// Resolve the parallel-batch goal + its final verifier and load (or
/// plan) the manifest. Returns `Err` when the goal declares no verifier,
/// the manifest file is invalid/unreadable, or the planner pass fails.
fn resolve_parallel_setup(
    goal_or_case: &str,
    manifest_path: Option<&Path>,
    verify: Option<&str>,
    no_sandbox: bool,
) -> Result<(supervisor::ResolvedSpec, planner::PlannerManifest), String> {
    let resolved = supervisor::resolve(&supervisor::ResolveInput {
        goal_or_case,
        root: &root(),
        workdir: &root(),
        verify,
        target: None,
    })?;
    if resolved.verify_cmd.is_empty() {
        return Err("the goal declares no verifier (P0-3) — the final gate cannot run".to_string());
    }
    let manifest = match manifest_path {
        Some(p) => {
            let text =
                std::fs::read_to_string(p).map_err(|e| format!("cannot read manifest: {e}"))?;
            planner::parse_manifest(&text).map_err(|e| format!("manifest invalid: {e}"))?
        }
        None => planner::run_planner_pass(&resolved.goal, &root(), no_sandbox)
            .map_err(|e| format!("planner pass failed: {e}"))?,
    };
    Ok((resolved, manifest))
}

fn cmd_loop_parallel(
    goal_or_case: &str,
    manifest_path: Option<&Path>,
    max_parallel: usize,
    iterate: usize,
    max_wall: Option<u64>,
    no_sandbox: bool,
    verify: Option<&str>,
) -> ExitCode {
    // 1. Resolve the goal + its final verifier (the case's own
    //    verify_command is the protected final gate) and the manifest.
    let (resolved, manifest) =
        match resolve_parallel_setup(goal_or_case, manifest_path, verify, no_sandbox) {
            Ok(p) => p,
            Err(e) => return fail(&e),
        };
    // F5: refuse unsandboxed workers BEFORE provisioning — an
    // invocation that must be refused must not leave batch artifacts.
    if !no_sandbox {
        return fail(
            "loop parallel requires --no-sandbox (the Landlock wrapper breaks the codex npm shim; an explicit opt-in is required)",
        );
    }
    // 3. Provision (worktrees at HEAD) + dispatch + poll.
    let base = match git_head(&root()) {
        Ok(b) => b,
        Err(e) => return fail(&e),
    };
    let provision = match planner::provision_batch(&root(), &manifest, &base) {
        Ok(p) => p,
        Err(e) => return fail(&format!("provision failed: {e}")),
    };
    println!(
        "batch: {} tickets at {} (max_parallel={max_parallel})",
        manifest.tickets.len(),
        &base[..base.len().min(10)]
    );
    for t in &manifest.tickets {
        println!(
            "  {}: {} -> [{}]",
            t.id,
            &t.goal[..t.goal.len().min(60)],
            t.scope.join(", ")
        );
    }
    let dispatch = match planner::dispatch_batch(
        &root(),
        &provision,
        &manifest,
        max_parallel,
        iterate,
        max_wall,
        no_sandbox,
    ) {
        Ok(d) => d,
        // F2: evidence + live worker worktrees stay on a dispatch error.
        Err(e) => {
            return fail(&format!(
                "batch dispatch failed: {e} (evidence preserved in .batch/ worktrees + branches)"
            ));
        }
    };
    render_parallel_dispatch_results(&dispatch, &manifest);
    let all_passed = dispatch.results.iter().all(|r| r.passed)
        && dispatch.results.len() == manifest.tickets.len();
    if !all_passed {
        // Evidence preserved: the worktrees stay (reports, transcripts,
        // handles) — teardown happens only on the SUCCESS path.
        return fail(
            "batch FAILED atomically — a ticket did not pass (evidence preserved in .batch/ worktrees + branches)",
        );
    }
    // 4. Finalize + merge + protected gate + the FINAL verifier.
    let merged =
        match planner::finalize_and_merge(&root(), &manifest, &provision, &dispatch.results) {
            Ok(m) => m,
            Err(e) => {
                return fail(&format!(
                    "merge failed: {e} (evidence preserved in .batch/ worktrees + branches)"
                ));
            }
        };
    println!(
        "merged: {} tickets -> {}",
        merged.merged.len(),
        merged.merge_sha
    );
    match planner::protected_paths_unchanged(&root(), &base) {
        Ok(true) => {}
        Ok(false) => {
            return fail(
                "PROTECTED-PATH DRIFT: the merged tree changed gate inputs — the final gate cannot be trusted (evidence preserved)",
            );
        }
        Err(e) => return fail(&format!("protected-path check failed: {e}")),
    }
    let final_gate =
        mini_agi_core::worker::run_capped("sh", &["-c", &resolved.verify_cmd], &root(), Some(600));
    let final_ok = final_gate.is_ok_and(|r| !r.aborted && r.status == Some(0));
    planner::teardown_batch(&root(), &provision);
    if final_ok {
        println!("FINAL GATE: PASSED ({})", resolved.verify_cmd);
        println!("batch SUCCESS: merged {}", merged.merge_sha);
        ExitCode::SUCCESS
    } else {
        println!("FINAL GATE: FAILED ({})", resolved.verify_cmd);
        ExitCode::from(1)
    }
}

/// Render the parallel-batch dispatch result summary (ticket-by-ticket
/// verdicts after `planner::dispatch_batch`).
fn render_parallel_dispatch_results(
    dispatch: &planner::BatchDispatchResult,
    manifest: &planner::PlannerManifest,
) {
    for r in &dispatch.results {
        println!(
            "  ticket {}: {}",
            r.id,
            if r.passed { "PASSED" } else { "NOT PASSED" }
        );
    }
    println!(
        "  batch: {} / {} tickets passed",
        dispatch.results.iter().filter(|r| r.passed).count(),
        manifest.tickets.len()
    );
    // D6: respawns are reported per-event on stderr while dispatching, and
    // summarized here so the batch verdict never hides a crashed-and-
    // relaunched ticket.
    if !dispatch.respawns.is_empty() {
        println!(
            "  respawns: {} ticket(s) relaunched after crashing without a report (events: stderr + .batch/respawns.log)",
            dispatch.respawns.len()
        );
    }
}

fn git_head(repo: &Path) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git rev-parse failed: {e}"))?;
    if !out.status.success() {
        return Err("git rev-parse HEAD failed".to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run-state index (D6): runs + totals, journal tail, live workers.
fn cmd_status(json: bool) -> ExitCode {
    let root = root();
    let idx = status::index_runs(&root.join("evals/cases"), &root);
    let journal = status::journal_tail(&root, 4);
    let workers = status::live_workers(&root);
    let respawns = status::respawn_summary(&root);
    if json {
        let out = serde_json::json!({
            "runs": idx,
            "journal_tail": journal,
            "workers": workers,
            "respawns": respawns,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).unwrap_or_else(|e| e.to_string())
        );
        return ExitCode::SUCCESS;
    }
    println!(
        "RUN-STATE INDEX — {} runs, {} achieved, ${:.4} total, {} tokens",
        idx.total_runs, idx.achieved_runs, idx.total_cost_usd, idx.total_tokens
    );
    for r in &idx.rows {
        let w = r.worker.as_deref().unwrap_or("(unreported)");
        let case: String = r.case.chars().take(28).collect();
        let comp = r
            .composite
            .map_or_else(|| "-".into(), |c| format!("{c:.2}"));
        println!(
            "  {:<28}  ${:.6}  {:>8} tok  composite={:>5}  achieved={}  {}",
            case, r.cost_usd, r.tokens_total, comp, r.achieved, w
        );
    }
    println!("journal tail:");
    for l in &journal {
        println!("  {l}");
    }
    println!("workers ({}):", workers.len());
    for w in &workers {
        println!(
            "  {}  alive={} report_ready={}",
            w.handle, w.alive, w.report_ready
        );
    }
    if respawns > 0 {
        println!("respawns: {respawns} recorded (.batch/respawns.log)");
    }
    ExitCode::SUCCESS
}

/// Dream-loop entry (D2): `--source <material>` distills + audits into
/// staging; `--promote` applies the latest manifest's verdicts.
/// D2 idle trigger: return the newest run to distill only when the
/// machine is quiet (load1 below the threshold) AND the newest run is
/// newer than the newest staging file. Returns `None` (skip) when the
/// box is busy, there is nothing to distill, or nothing is new.
fn dream_idle_source(root: &Path, idle_load: f64) -> Result<Option<std::path::PathBuf>, String> {
    let Some(load1) = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|l| l.split_whitespace().next().map(str::to_string))
        .and_then(|v| v.parse::<f64>().ok())
    else {
        return Err("dream --idle: cannot read load average".to_string());
    };
    if load1 >= idle_load {
        println!("dream --idle: load1 {load1:.2} >= {idle_load} — busy, skipping");
        return Ok(None);
    }
    let idx = status::index_runs(&root.join("evals/cases"), root);
    let Some(newest_run) = idx.rows.first() else {
        println!("dream --idle: no runs to distill");
        return Ok(None);
    };
    let staging_root = root.join(mini_agi_core::dream::STAGING_REL);
    let newest_staging = std::fs::read_dir(&staging_root).ok().and_then(|days| {
        let mut newest = None;
        for day in days.flatten() {
            let Ok(entries) = std::fs::read_dir(day.path()) else {
                continue;
            };
            for e in entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
            {
                if let Ok(meta) = std::fs::metadata(e.path())
                    && let Ok(m) = meta.modified()
                    && newest.is_none_or(|n| m > n)
                {
                    newest = Some(m);
                }
            }
        }
        newest
    });
    if let Some(st) = newest_staging
        && st >= newest_run.modified
    {
        println!("dream --idle: no newer runs since the last staging");
        return Ok(None);
    }
    Ok(Some(
        root.join("evals/cases")
            .join(&newest_run.case)
            .join("run.json"),
    ))
}

/// Run the cheap distiller once, then retry once with validator feedback
/// when it returned no parseable facts (cycle-33 finding: cheap models
/// occasionally emit prose/fences instead of the JSON fact array).
/// Returns the final worker output + parsed facts, or `Err` when the
/// worker is unavailable or exits nonzero.
fn run_distill_with_retry(
    workdir: &Path,
    distiller: &str,
    material: &str,
    max_wall: u64,
) -> Result<
    (
        mini_agi_core::worker::WorkerResult,
        Vec<mini_agi_core::dream::StagedFact>,
    ),
    String,
> {
    let mut dist = worker::run_opencode_worker(
        workdir,
        distiller,
        &mini_agi_core::dream::distiller_prompt(material),
        Some(max_wall),
        None,
    )
    .map_err(|e| format!("dream distiller not available: {e}"))?;
    let mut staged = mini_agi_core::dream::parse_distilled_facts(&dist.output);
    if staged.is_empty() && dist.status == Some(0) {
        eprintln!(
            "  [warn] distiller returned no parseable facts ({} bytes) — retrying once with feedback",
            dist.output.len()
        );
        let prompt = format!(
            "{}\n\n{}",
            mini_agi_core::dream::distiller_prompt(material),
            mini_agi_core::dream::distiller_retry_feedback()
        );
        dist = worker::run_opencode_worker(workdir, distiller, &prompt, Some(max_wall), None)
            .map_err(|e| format!("dream distiller retry not available: {e}"))?;
        staged = mini_agi_core::dream::parse_distilled_facts(&dist.output);
    }
    if dist.status != Some(0) {
        return Err(format!(
            "dream distiller exited {:?} — no candidates",
            dist.status
        ));
    }
    Ok((dist, staged))
}

fn cmd_dream(
    source: Option<&Path>,
    distiller: &str,
    auditor: &str,
    promote: bool,
    dry_run: bool,
    max_wall: Option<u64>,
    idle_load: Option<f64>,
) -> ExitCode {
    let root = root();
    if promote {
        return cmd_dream_promote(&root, dry_run);
    }
    let source = if let Some(load) = idle_load {
        match dream_idle_source(&root, load) {
            Ok(Some(p)) => p,
            Ok(None) => return ExitCode::SUCCESS,
            Err(e) => return fail(&e),
        }
    } else {
        let Some(source) = source else {
            return fail("dream: give --source <episodic material>, --idle, or --promote");
        };
        source.to_path_buf()
    };
    let material = match std::fs::read_to_string(&source) {
        Ok(t) => t,
        Err(e) => return fail(&format!("dream: cannot read {}: {e}", source.display())),
    };
    // Cycle 33 finding (measured): a fixed 300 s wall cap is too small
    // for large reports — a 105 KB material stalled the audit and needed
    // ~900 s. Scale the default with the material size (roughly 1 s per
    // 300 bytes, floored at 300 s), and let an explicit `--max-wall`
    // override the scale.
    let max_wall = max_wall.unwrap_or_else(|| (300u64).max(material.len() as u64 / 300));
    let workdir = std::env::temp_dir().join(format!("mag-dream-wd-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&workdir);
    let source_rel = source
        .to_string_lossy()
        .strip_prefix(&root.to_string_lossy().into_owned())
        .unwrap_or(&source.to_string_lossy())
        .to_string();
    // 1. Distill (cheap worker, D1 adapter) with bounded retry.
    let staged = match run_distill_with_retry(&workdir, distiller, &material, max_wall) {
        Ok((_dist, staged)) => staged,
        Err(e) => return fail(&e),
    };
    if staged.is_empty() {
        println!("dream: distiller returned no candidate facts");
        return ExitCode::SUCCESS;
    }
    if dry_run {
        println!(
            "dream (dry-run): {} candidate(s) from {}",
            staged.len(),
            source_rel
        );
        for f in &staged {
            println!("  [{}] {}", f.domain, f.body);
        }
        return ExitCode::SUCCESS;
    }
    let staging = match mini_agi_core::dream::write_staging(&root, &staged, &source_rel, distiller)
    {
        Ok(p) => p,
        Err(e) => return fail(&format!("dream: staging write failed: {e}")),
    };
    // 2. Audit (strong worker). The canonical index is BUDGETED (D3
    // select_budgeted): dumping the whole store bloats the auditor's
    // prompt and the strong model stalls (observed: a 20.8k-char audit
    // prompt timed out with zero output). The auditor only needs the
    // most relevant facts — enforced, linked, recent.
    let all = mini_agi_core::memory::read_facts(&root);
    let links = mini_agi_core::memory::fact_links(&all);
    let enforced = mini_agi_core::memory::enforced_fact_ids(&root);
    let selected = mini_agi_core::memory::select_budgeted(&all, &links, &enforced, 6000);
    let mut audit_lines: Vec<String> = selected
        .iter()
        .map(|(id, _, body)| format!("{id}: {body}"))
        .collect();
    audit_lines.sort();
    let audit_material = audit_lines.join("\n");
    // Audit in batches: the strong model stalls on oversized prompts
    // (observed twice: a 20.8k-char dump and 40 candidates both
    // returned zero output). 15 candidates per call keeps the prompt
    // ~9k chars; verdicts are merged with their batch offset.
    let audit_batch_size = 15usize;
    let mut verdicts: Vec<mini_agi_core::dream::AuditorVerdict> = Vec::new();
    for (batch_idx, chunk) in staged.chunks(audit_batch_size).enumerate() {
        let batch_prompt = mini_agi_core::dream::auditor_prompt(chunk, &audit_material);
        let aud = match worker::run_opencode_worker(
            &workdir,
            auditor,
            &batch_prompt,
            Some(max_wall),
            None,
        ) {
            Ok(w) => w,
            Err(e) => return fail(&format!("dream auditor not available: {e}")),
        };
        let mut batch_verdicts = mini_agi_core::dream::parse_audit_verdicts(&aud.output, chunk);
        if batch_verdicts.is_empty() && aud.status == Some(0) {
            // Procedure-directed retry (cycle-33 finding, Tell-Tale Trace
            // #95): the strong model occasionally answers in prose
            // instead of the JSON verdict array (observed: rc 0 with a
            // 15.7k-char prose answer). A GENERIC "try again" reproduces
            // the same mode (11.5% correction in the study); feeding back
            // the missing procedure recovers most cases (84.6%). Only a
            // successful worker call is retried — a nonzero/timed-out
            // worker is an infrastructure failure, not a format one, and
            // retrying would mislabel the diagnosis.
            eprintln!(
                "  [warn] auditor batch {batch_idx} returned no parseable verdicts \
                 ({} bytes, rc {:?}) — retrying once with procedure feedback",
                aud.output.len(),
                aud.status
            );
            let retry_prompt = format!(
                "{}\n\n{}",
                batch_prompt,
                mini_agi_core::dream::auditor_retry_feedback()
            );
            let retry =
                worker::run_opencode_worker(&workdir, auditor, &retry_prompt, Some(max_wall), None);
            if let Ok(aud) = retry {
                batch_verdicts = mini_agi_core::dream::parse_audit_verdicts(&aud.output, chunk);
            }
        }
        for v in &mut batch_verdicts {
            v.index += batch_idx * audit_batch_size;
        }
        if batch_verdicts.is_empty() {
            eprintln!("  [warn] auditor batch {batch_idx} failed after retry — skipped");
        }
        verdicts.extend(batch_verdicts);
    }
    if verdicts.is_empty() {
        eprintln!("  [warn] auditor returned no verdicts at all — nothing will promote");
    }
    // Coverage check (cycle-33 review F6): a partial verdict array must
    // not silently strand candidates. Every staged candidate needs a
    // verdict (promote/duplicate/conflict/reject); a shortfall means the
    // auditor skipped candidates and a silent promote would lose them.
    let missing = staged.len().saturating_sub(verdicts.len());
    if missing > 0 {
        eprintln!(
            "  [warn] auditor covered {}/{} candidates — {missing} would be silently stranded; \
             re-run dream --source to retry the audit",
            verdicts.len(),
            staged.len()
        );
        return fail(&format!(
            "dream audit incomplete: {missing} of {} candidates have no verdict — refusing to promote a partial verdict set",
            staged.len()
        ));
    }
    match mini_agi_core::dream::write_verdicts(&staging, &verdicts) {
        Ok(m) => println!("  verdicts manifest: {}", m.display()),
        Err(e) => return fail(&format!("dream: verdict manifest write failed: {e}")),
    }
    println!(
        "dream: {} candidate(s) staged at {}, {} verdict(s) from {}",
        staged.len(),
        staging.display(),
        verdicts.len(),
        auditor
    );
    for v in &verdicts {
        println!("  [{v:?}]", v = v.verdict);
    }
    ExitCode::SUCCESS
}

/// Apply the newest staging manifest (the latest `<date>/<seq>.md` +
/// its auditor verdicts are re-derived from the manifest file's facts;
/// the verdicts were recorded at audit time — for a truthful single
/// pipeline the audit output is re-run here only when `--reaudit` is
/// given; by default promotion applies verdicts recorded in the last
/// `dream` run).
fn cmd_dream_promote(root: &Path, dry_run: bool) -> ExitCode {
    // Locate the newest staging file.
    let staging_root = root.join(mini_agi_core::dream::STAGING_REL);
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let Ok(days) = std::fs::read_dir(&staging_root) else {
        return fail("dream promote: no staging dir yet — run dream --source first");
    };
    for day in days.flatten() {
        let Ok(entries) = std::fs::read_dir(day.path()) else {
            continue;
        };
        for e in entries.flatten() {
            if e.path().extension().is_some_and(|x| x == "md") {
                files.push(e.path());
            }
        }
    }
    files.sort();
    let Some(latest) = files
        .iter()
        .rev()
        .find(|path| {
            mini_agi_core::dream::read_promotion_receipt(path)
                .is_none_or(|receipt| !mini_agi_core::dream::receipt_matches_staged(path, &receipt))
        })
        .cloned()
    else {
        println!("dream promote: every staged batch has a matching application receipt");
        return ExitCode::SUCCESS;
    };
    let staged = match read_staged_facts(&latest) {
        Ok(s) => s,
        Err(e) => return fail(&e),
    };
    let verdicts = mini_agi_core::dream::read_verdicts(&latest.with_extension("verdicts.json"));
    if verdicts.is_empty() {
        return fail(&format!(
            "dream promote: no verdicts manifest next to {} — run dream --source first",
            latest.display()
        ));
    }
    let (promoted, queued, skipped) = match mini_agi_core::dream::apply_verdicts(
        root,
        &staged,
        &verdicts,
        &format!("dream promote ({})", latest.display()),
        dry_run,
    ) {
        Ok(r) => r,
        Err(e) => return fail(&format!("dream promote: {e}")),
    };
    if !dry_run {
        // The application receipt is written LAST: a failure here is
        // loud and leaves the batch `pending`, never a false `applied`.
        if let Err(e) = mini_agi_core::dream::write_promotion_receipt(
            &latest,
            promoted,
            queued,
            skipped,
        ) {
            return fail(&format!(
                "dream promote: verdicts were applied but the promotion receipt could not be written: {e}"
            ));
        }
        println!("  promotion receipt: {}", latest.with_extension("promotion.json").display());
    }
    println!(
        "dream promote{}: {} promoted, {} queued (human), {} skipped — from {}",
        if dry_run {
            " (dry-run — nothing written)"
        } else {
            ""
        },
        promoted,
        queued,
        skipped,
        latest.display()
    );
    ExitCode::SUCCESS
}

/// Candidate count in a staging file (ui.rs surface).
pub(crate) fn read_staged_facts_count(path: &Path) -> usize {
    read_staged_facts(path).map_or(0, |f| f.len())
}

/// Read staged `## S-NNN (domain)` blocks back from a staging file.
fn read_staged_facts(path: &Path) -> Result<Vec<mini_agi_core::dream::StagedFact>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut facts = Vec::new();
    let mut domain = "general".to_string();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## S-") {
            domain = rest
                .split('(')
                .nth(1)
                .and_then(|d| d.split(')').next())
                .unwrap_or("general")
                .trim()
                .to_lowercase();
        } else if let Some(rest) = line.strip_prefix("- domain:") {
            domain = rest.trim().to_string();
        } else if !line.trim().is_empty()
            && !line.starts_with('#')
            && !line.starts_with("- ")
            && !line.starts_with("## ")
        {
            facts.push(mini_agi_core::dream::StagedFact {
                body: line.trim().to_string(),
                domain: domain.clone(),
            });
        }
    }
    if facts.is_empty() {
        return Err(format!("no staged facts parsed from {}", path.display()));
    }
    Ok(facts)
}

/// Auto-researcher: run the flash worker with the research contract,
/// capture the answer, write research/<slug>.md (findings feed the
/// dream-loop).
///
/// Registry + dedup (autoresearch wiring): every question is recorded in
/// `research/registry.json`; asking the same question again when its
/// findings already exist resolves to the existing file instead of
/// spawning a second worker run (`--force` re-researches).
///
/// `--chain`: the full D2 autoresearch pipeline in one call —
/// research -> distill -> audit -> promote -> registry status `Promoted`
/// — so the knowledge loop closes without three manual invocations.
fn cmd_research(
    question: &str,
    worker: &str,
    max_wall: u64,
    force: bool,
    chain: bool,
    distiller: &str,
    auditor: &str,
) -> ExitCode {
    let root = root();
    let slug = research::slugify(question);
    let out_path = research::findings_path(&root, question);
    if !force && out_path.is_file() {
        let entries = research_registry::load_registry(&root);
        let status = research_registry::find_entry(&entries, &slug)
            .map_or(research_registry::QuestionStatus::Findings, |e| e.status);
        println!(
            "research: duplicate question — findings already exist at {} (status: {status:?}); pass --force to re-research",
            out_path.display()
        );
        if chain
            && matches!(
                status,
                research_registry::QuestionStatus::Promoted
                    | research_registry::QuestionStatus::Decided
            )
        {
            println!(
                "research chain: already promoted/decided — nothing to distill, pass --force to re-run"
            );
            return ExitCode::SUCCESS;
        }
        if chain {
            return run_research_chain(&root, &out_path, &slug, distiller, auditor);
        }
        return ExitCode::SUCCESS;
    }
    let workdir = std::env::temp_dir().join(format!("mag-research-wd-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&workdir);
    let result = match worker::run_opencode_worker(
        &workdir,
        worker,
        &research::research_prompt(question),
        Some(max_wall),
        None,
    ) {
        Ok(w) => w,
        Err(e) => return fail(&format!("research worker not available: {e}")),
    };
    if result.status != Some(0) {
        return fail(&format!("research worker exited {:?}", result.status));
    }
    let extracted = mini_agi_core::dream::extract_text_parts(&result.output);
    let findings = if extracted.trim().is_empty() {
        result.output
    } else {
        extracted
    };
    if findings.trim().is_empty() {
        return fail("research: worker returned no findings");
    }
    if !research::is_complete_deliverable(&findings) {
        return fail(
            "research: INCOMPLETE deliverable (missing ## Findings / ## Sources /              ## Verdict or no claims) — not written, re-run the question",
        );
    }
    let out_path = research::findings_path(&root, question);
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&out_path, &findings) {
        Ok(()) => {
            let entry = research_registry::record_asked(&root, question)
                .map_err(|e| format!("research: registry write failed: {e}"))
                .map(|e| {
                    research_registry::advance_status(
                        &root,
                        &e.slug,
                        research_registry::QuestionStatus::Findings,
                    )
                    .map_err(|e| format!("research: registry write failed: {e}"))
                });
            if let Err(e) = entry.flatten() {
                return fail(&e);
            }
            println!(
                "research: {} bytes -> {} ({}s, cost ${:.6})",
                findings.len(),
                out_path.display(),
                result.wall_seconds,
                result.usage.map_or(0.0, |u| u.cost_usd)
            );
            if chain {
                return run_research_chain(&root, &out_path, &slug, distiller, auditor);
            }
            println!("next: mini-agi dream --source {}", out_path.display());
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("research: cannot write findings: {e}")),
    }
}

/// The D2 autoresearch chain after findings exist: distill -> audit ->
/// promote -> ticket, then mark the registry `Decided`. A single failure
/// fails the whole chain (no silent partial promotion).
fn run_research_chain(
    root: &std::path::Path,
    findings: &std::path::Path,
    slug: &str,
    distiller: &str,
    auditor: &str,
) -> ExitCode {
    let dream = cmd_dream(Some(findings), distiller, auditor, false, false, None, None);
    if dream != ExitCode::SUCCESS {
        return dream;
    }
    let promote = cmd_dream_promote(root, false);
    if promote != ExitCode::SUCCESS {
        return promote;
    }
    // Close the loop research -> decision: promote alone leaves the
    // findings as knowledge without a next action. A research ticket
    // makes the decision explicit (domain `research`), deduplicated by
    // slug so a chain re-run on the same question finds it.
    let ticket_id = match ensure_research_ticket(root, slug, findings) {
        Ok(id) => id,
        Err(code) => return code,
    };
    if let Err(e) =
        research_registry::advance_status(root, slug, research_registry::QuestionStatus::Decided)
    {
        return fail(&format!("research: registry write failed: {e}"));
    }
    println!(
        "research chain: findings distilled, audited, promoted, decided — registry '{slug}' = Decided, ticket {ticket_id}"
    );
    ExitCode::SUCCESS
}

/// Create (or return the existing) research ticket for a question.
/// Dedup by slug in the ticket title; the id is the next free `TICKET-<n>`.
fn ensure_research_ticket(
    root: &std::path::Path,
    slug: &str,
    findings: &std::path::Path,
) -> Result<String, ExitCode> {
    let existing = mini_agi_core::ticket::list_tickets(root).unwrap_or_default();
    if let Some(t) = existing
        .iter()
        .find(|t| t.goal.contains(slug) || t.title.contains(slug))
    {
        return Ok(t.id.clone());
    }
    let next_number = existing
        .iter()
        .filter_map(|t| {
            t.id.strip_prefix("TICKET-")
                .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|d| d.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    let id = format!("TICKET-{next_number}");
    let rel = findings
        .strip_prefix(root)
        .unwrap_or(findings)
        .to_string_lossy();
    let body = format!(
        "# Ticket\n\n- id: {id}\n- title: Research decision: {slug}\n- goal (one sentence): Apply the researched findings at {rel} — decide, implement, and measure the change they call for.\n- scope: research\n- domain: research\n- source: {rel}\n"
    );
    let path = root.join("tickets").join(format!("{id}.md"));
    if let Err(e) = std::fs::write(&path, body) {
        return Err(fail(&format!("research: ticket write failed: {e}")));
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_ticket_created_once_and_dedups_by_slug() {
        let root = std::env::temp_dir().join(format!("mag-research-ticket-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("tickets")).unwrap();
        let findings = root.join("research").join("what-is-x.md");
        std::fs::create_dir_all(findings.parent().unwrap()).unwrap();
        std::fs::write(&findings, "x").unwrap();
        let id1 = ensure_research_ticket(&root, "what-is-x", &findings).unwrap();
        assert!(id1.starts_with("TICKET-"), "id: {id1}");
        let path = root.join("tickets").join(format!("{id1}.md"));
        assert!(path.is_file(), "ticket written");
        // Same slug again -> same ticket, no duplicate file.
        let id2 = ensure_research_ticket(&root, "what-is-x", &findings).unwrap();
        assert_eq!(id1, id2, "dedup by slug");
        let count = std::fs::read_dir(root.join("tickets"))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().starts_with("TICKET-"))
            })
            .count();
        assert_eq!(count, 1, "no duplicate ticket files");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn knowledge_questions_map_failures_and_stalled_registry() {
        let fail = mini_agi_core::failure::FailureEntry {
            hash: "abc".into(),
            tool: "edit".into(),
            action: "edit same line".into(),
            count: 3,
            steps: vec![1, 2, 3],
            case: "case-x".into(),
            reflection: Some("plan the fix before editing".into()),
            mast: Some("FM-1.3 step repetition".into()),
            verifier: None,
        };
        // Single-occurrence failures and reflection-less failures are not
        // knowledge gaps.
        let noise = mini_agi_core::failure::FailureEntry {
            count: 1,
            reflection: Some("noise".into()),
            ..fail.clone()
        };
        let stall = research_registry::RegistryEntry {
            question: "Is multi-repo the right shape?".into(),
            slug: "is-multi-repo-the-right-shape".into(),
            status: research_registry::QuestionStatus::Distilled,
            updated: "2026-08-09".into(),
        };
        let promoted = research_registry::RegistryEntry {
            status: research_registry::QuestionStatus::Promoted,
            ..stall.clone()
        };
        let qs = knowledge_questions(&[fail.clone(), noise], &[stall, promoted]);
        assert_eq!(qs.len(), 2, "one failure gap + one stalled question");
        let failure_slug = research::slugify(&qs[0].1);
        assert!(
            qs.iter().any(|(s, _)| s == &failure_slug),
            "failure reflection surfaces a question"
        );
        assert!(
            qs.iter().any(|(s, _)| s == "is-multi-repo-the-right-shape"),
            "stalled registry question surfaces"
        );
        assert!(
            !qs.iter().any(|(s, _)| s == "noise"),
            "single/reflection-less failures are not gaps"
        );
        // Dedup: the same failure twice produces one question.
        let dedup = knowledge_questions(&[fail.clone(), fail], &[]);
        assert_eq!(dedup.len(), 1, "duplicate failure dedups by slug");
    }

    #[test]
    fn parallel_respawn_summary_renders_without_panicking() {
        // D6 contract: a batch that relaunched crashed tickets reports a
        // respawn summary. Smoke: builds the render inputs and runs the
        // renderer (stdout capture is intentionally not asserted — the
        // behavior under test is that a non-empty respawn list is
        // rendered, not the exact bytes).
        let dispatch = planner::BatchDispatchResult {
            results: vec![planner::BatchTicketResult {
                id: "t1".into(),
                worktree: "/tmp/w".into(),
                passed: true,
            }],
            respawns: vec!["t1 crashed once".into()],
        };
        let manifest = planner::PlannerManifest {
            version: 1,
            tickets: vec![],
        };
        render_parallel_dispatch_results(&dispatch, &manifest);
    }

    #[test]
    fn dream_idle_freshness_selects_newer_run() {
        // D2 idle freshness (deterministic with a huge load threshold:
        // load1 < 100 on any host): a run newer than the newest staging
        // is selected; a staging newer than the run returns None.
        let root = std::env::temp_dir().join(format!(
            "mag-dream-idle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let run = root.join("evals/cases/a/run.json");
        std::fs::create_dir_all(run.parent().unwrap()).unwrap();
        std::fs::write(&run, r#"{"goal":"g","trajectory":[]}"#).unwrap();
        // Backdate the run 1h so the freshness comparison is meaningful.
        let past = std::time::SystemTime::now() - std::time::Duration::from_hours(1);
        std::fs::File::open(&run)
            .unwrap()
            .set_modified(past)
            .unwrap();
        let src = dream_idle_source(&root, 100.0).unwrap();
        assert!(
            src.is_some(),
            "a fresh run must be selected for distillation"
        );
        assert_eq!(src.unwrap(), run);
        // Now write a staging file newer than the run: freshness fails.
        let staging = root.join("memory/staging/2026-08-08/001.md");
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::write(&staging, "# s").unwrap();
        std::fs::File::open(&staging)
            .unwrap()
            .set_modified(std::time::SystemTime::now())
            .unwrap();
        assert!(
            dream_idle_source(&root, 100.0).unwrap().is_none(),
            "staging newer than the run must skip distillation"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_staged_facts_parses_domains_and_bodies() {
        let root = std::env::temp_dir().join(format!(
            "mag-staged-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let staged = root.join("staged.md");
        std::fs::write(
            &staged,
            "# Candidate\n\n## S-001 (eval-core)\n- domain: agent-behavior\n\nbody one\n\n## S-002\n\nbody two\n",
        )
        .unwrap();
        let facts = read_staged_facts(&staged).unwrap();
        assert_eq!(facts.len(), 2, "{facts:?}");
        assert_eq!(facts[0].body, "body one");
        assert_eq!(
            facts[0].domain, "agent-behavior",
            "inline domain wins: {facts:?}"
        );
        assert_eq!(facts[1].domain, "general", "default domain: {facts:?}");
        // Empty input fails cleanly.
        std::fs::write(root.join("empty.md"), "# nothing\n").unwrap();
        assert!(read_staged_facts(&root.join("empty.md")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn validate_doc_accepts_contract_and_rejects_bad() {
        let root = std::env::temp_dir().join(format!(
            "mag-vdoc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let ok = root.join("ok.json");
        std::fs::write(
            &ok,
            r#"{"goal":"g","scope":["x"],"outcome":{"achieved":true},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":null,"verify_target":null,"trajectory":[{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        let r = validate_doc_text("eval-run", &ok).unwrap();
        assert!(r.contains("validates"), "{r}");
        // Invalid document: missing required field.
        let bad = root.join("bad.json");
        std::fs::write(&bad, r#"{"goal":"g"}"#).unwrap();
        assert!(validate_doc_text("eval-run", &bad).is_err());
        // Unknown contract name.
        assert!(validate_doc_text("nope", &ok).is_err());
        // Missing file.
        assert!(validate_doc_text("eval-run", &root.join("missing.json")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
