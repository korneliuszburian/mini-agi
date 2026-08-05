//! stdio MCP server (Model Context Protocol, JSON-RPC 2.0).
//!
//! Hand-rolled, zero dependencies: LSP-style `Content-Length` framing over
//! stdio, protocol version `2025-03-26`. Exposes the kernel as tools so
//! Codex, Claude, Cursor and opencode plug into the SAME verified brain
//! through the standard protocol (PLAN, Phase 4).

use std::io::{self, BufRead, Write};
use std::path::Path;

use mini_agi_core::{eval, insights, memory, skills, ticket};
use serde_json::{Value, json};

const PROTOCOL_VERSION: &str = "2025-03-26";
const SERVER_NAME: &str = "mini-agi";

/// Protocol versions this server actually speaks; anything else falls back
/// to `PROTOCOL_VERSION` during negotiation.
const SUPPORTED_VERSIONS: &[&str] = &["2025-03-26", "2025-06-18", "2025-11-25"];

/// Run the stdio MCP server loop until EOF.
///
/// # Errors
///
/// Returns an error when stdin/stdout framing fails.
pub fn run_stdio_server() -> Result<(), io::Error> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut initialized = false;
    while let Some(message) = read_frame(&mut input)? {
        if let Some(payload) = dispatch(&message, &mut initialized) {
            write_frame(&mut output, &payload)?;
        }
    }
    Ok(())
}

/// Maximum accepted frame body (protects the allocator from an
/// attacker-controlled `Content-Length`; MCP frames are tiny).
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Read one `Content-Length` framed JSON message; `None` on clean EOF.
fn read_frame<R: BufRead>(input: &mut R) -> Result<Option<Value>, io::Error> {
    let mut first = String::new();
    if input.read_line(&mut first)? == 0 {
        return Ok(None); // EOF before any frame
    }
    let first = first.trim_end();
    if first.to_ascii_lowercase().starts_with("content-length:") {
        let rest = first.split_once(':').map_or("", |(_, v)| v);
        let length = rest
            .trim()
            .parse::<usize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad Content-Length"))?;
        if length > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame too large: {length} > {MAX_FRAME_BYTES}"),
            ));
        }
        loop {
            let mut header = String::new();
            if input.read_line(&mut header)? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected EOF in headers",
                ));
            }
            if header.trim().is_empty() {
                break;
            }
        }
        let mut body = vec![0u8; length];
        input.read_exact(&mut body)?;
        serde_json::from_slice(&body)
            .map(Some)
            .map_err(io::Error::other)
    } else if first.starts_with('{') || first.starts_with('[') {
        serde_json::from_str(first)
            .map(Some)
            .map_err(io::Error::other)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unrecognized MCP frame: {first:?}"),
        ))
    }
}

/// Write one framed JSON message.
fn write_frame<W: Write>(output: &mut W, payload: &Value) -> Result<(), io::Error> {
    let body = serde_json::to_vec(payload).map_err(io::Error::other)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

/// Handle one JSON-RPC message; `None` = notification (no response).
fn dispatch(message: &Value, initialized: &mut bool) -> Option<Value> {
    let id = message.get("id")?; // notification
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => handle_initialize(&params),
        "ping" => json!({}),
        "tools/list" if *initialized => handle_tools_list(),
        "tools/call" if *initialized => handle_tools_call(&params),
        "tools/list" | "tools/call" => err_response("server not initialized"),
        other => err_response(&format!("unknown method '{other}'")),
    };
    if method == "initialize" {
        *initialized = true;
    }
    Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    }))
}

fn handle_initialize(params: &Value) -> Value {
    // The server negotiates: echo the client's version only when it is one
    // we actually support; otherwise offer our own (spec fallback).
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    let negotiated = if SUPPORTED_VERSIONS.contains(&requested) {
        requested
    } else {
        PROTOCOL_VERSION
    };
    json!({
        "protocolVersion": negotiated,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        // Server instructions (MCP spec InitializeResult): shown to the
        // model as server-wide guidance (codex manual: keep the first
        // 512 chars self-contained). The discipline contract for every
        // codex session using the kernel (AFK-SUPERVISOR S4).
        "instructions": concat!(
            "mini-agi kernel: enforcement-bound memory + verified-iteration. ",
            "Use loop_status (open gaps), memory_query (facts), ",
            "run_verify <path> (a run stays unverified until this passes), ",
            "loop_verify <case> (close a gap), checkpoint_audit, eval_gate, ",
            "provenance. Results are provenance-bound. NEVER claim success ",
            "on an unverified run; outcome.achieved is only the run's own ",
            "claim until run_verify passes."
        ),
    })
}

fn handle_tools_list() -> Value {
    json!({ "tools": tool_definitions() })
}

fn err_response(message: &str) -> Value {
    json!({ "isError": true, "content": [{ "type": "text", "text": message }] })
}

struct ToolDef {
    name: &'static str,
    description: &'static str,
    /// Declared params as (name, JSON type) — optional unless in `required`.
    params: &'static [(&'static str, &'static str)],
    required: &'static [&'static str],
}

fn tool_definitions() -> Vec<Value> {
    const TOOLS: &[ToolDef] = &[
        ToolDef {
            name: "memory_consolidate",
            description: "Consolidate an episodic buffer into canonical facts.",
            params: &[
                ("episodic", "string"),
                ("domain", "string"),
                ("require_signoff", "boolean"),
                ("dry_run", "boolean"),
            ],
            required: &["episodic"],
        },
        ToolDef {
            name: "memory_signoff",
            description: "Promote one contested fact from the review queue.",
            params: &[
                ("queue", "string"),
                ("index", "integer"),
                ("domain", "string"),
            ],
            required: &["queue", "index"],
        },
        ToolDef {
            name: "memory_derive",
            description: "Regenerate derived views from canonical memory.",
            params: &[("brief_only", "boolean")],
            required: &[],
        },
        ToolDef {
            name: "provenance",
            description: "Print the canonical fingerprint for the provenance gate.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "eval_score",
            description: "Score one run.json against the golden set.",
            params: &[("run", "string")],
            required: &["run"],
        },
        ToolDef {
            name: "eval_gate",
            description: "Regression gate over all cases vs the baseline.",
            params: &[("tolerance", "number"), ("write_baseline", "boolean")],
            required: &[],
        },
        ToolDef {
            name: "skill_list",
            description: "List all discovered skills with verify hooks.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "skill_show",
            description: "Show one skill's frontmatter summary.",
            params: &[("name", "string")],
            required: &["name"],
        },
        ToolDef {
            name: "skill_verify",
            description: "Run a skill's verify hook.",
            params: &[("name", "string")],
            required: &["name"],
        },
        ToolDef {
            name: "skill_add",
            description: "Install skills from a git source.",
            params: &[("source", "string")],
            required: &["source"],
        },
        ToolDef {
            name: "checkpoint_audit",
            description: "Checkpoint journal completeness audit.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "validate",
            description: "Validate a document against a pipeline contract.",
            params: &[("contract", "string"), ("document", "string")],
            required: &["contract", "document"],
        },
        ToolDef {
            name: "stats",
            description: "Canonical-memory inventory by domain.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "budget",
            description: "Context budget report.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "ticket_list",
            description: "List all tickets in tickets/.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "ticket_show",
            description: "Show one ticket (TICKET-<n> or number).",
            params: &[("id", "string")],
            required: &["id"],
        },
        ToolDef {
            name: "ticket_validate",
            description: "Validate one ticket against the ADR-0007 contract.",
            params: &[("id", "string")],
            required: &["id"],
        },
        ToolDef {
            name: "run_ingest",
            description: "Ingest a scored run.json into canonical memory (ADR-0005).",
            params: &[("run", "string"), ("retro", "string")],
            required: &["run"],
        },
        ToolDef {
            name: "insights",
            description: "Compounding report: runs, memory, tickets, gaps.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "backlog",
            description: "Failure signal -> roadmap: gaps become tickets.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "resume",
            description: "Resume block for a fresh session.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "loop_status",
            description: "Proactive loop status: cases below target, tickets, claims, reruns.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "health",
            description: "Runtime observability: load, memory, process zoo, journal, claims.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "audit",
            description: "Repo invariants: provenance drift, baseline, tree, eval gate.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "ticket_claim",
            description: "Claim a ticket (lease).",
            params: &[("id", "string"), ("claimant", "string")],
            required: &["id", "claimant"],
        },
        ToolDef {
            name: "ticket_release",
            description: "Release a claim (holder only).",
            params: &[("id", "string"), ("claimant", "string")],
            required: &["id", "claimant"],
        },
        ToolDef {
            name: "ticket_claims",
            description: "List held claims.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "ticket_graph",
            description: "Print the dependency graph.",
            params: &[],
            required: &[],
        },
        ToolDef {
            name: "loop_dispatch",
            description: "Dispatch the worst open case (claim + spec).",
            params: &[("claimant", "string"), ("case", "string")],
            required: &["claimant"],
        },
        ToolDef {
            name: "loop_objective",
            description: "Bounded batch dispatch of open gaps under a budget.",
            params: &[
                ("max_cases", "integer"),
                ("budget_cost", "string"),
                ("claimant", "string"),
            ],
            required: &["claimant"],
        },
        ToolDef {
            name: "memory_query",
            description: "Domain/keyword retrieval over canonical facts.",
            params: &[("keyword", "string"), ("domain", "string")],
            required: &[],
        },
        ToolDef {
            name: "loop_run",
            description: "AFK supervisor: launch a verified-iteration run in the BACKGROUND (detached child) and return the run handle; poll with run_status, read the report with run_report. Requires an approval reason (a write that changes the worker tree).",
            params: &[
                ("goal_or_case", "string"),
                ("workdir", "string"),
                ("verify", "string"),
                ("target", "string"),
                ("iterate", "integer"),
                ("max_wall", "integer"),
                ("max_idle", "integer"),
                ("blind_worker", "boolean"),
                ("hidden_dir", "string"),
                ("on_done", "string"),
                ("report", "string"),
                ("template", "string"),
                ("no_resume", "boolean"),
                ("no_sandbox", "boolean"),
                ("approve", "string"),
            ],
            required: &["goal_or_case", "workdir", "approve"],
        },
        ToolDef {
            name: "run_status",
            description: "Poll a launched background run (handle from loop_run): alive, report ready, progress tail.",
            params: &[("handle", "string")],
            required: &["handle"],
        },
        ToolDef {
            name: "run_report",
            description: "Read the run report of a launched background run (handle from loop_run).",
            params: &[("handle", "string")],
            required: &["handle"],
        },
        ToolDef {
            name: "loop_verify",
            description: "Verify a rerun; close the gap at the target.",
            params: &[("case", "string"), ("claimant", "string")],
            required: &["case", "claimant"],
        },
        ToolDef {
            name: "eval_steps",
            description: "Process supervision: per-step verdicts.",
            params: &[("run", "string")],
            required: &["run"],
        },
        ToolDef {
            name: "run_verify",
            description: "Deterministic verification of a run's outcome.",
            params: &[("run", "string")],
            required: &["run"],
        },
        ToolDef {
            name: "run_failures",
            description: "Register repeated failing actions (Reflexion).",
            params: &[("run", "string")],
            required: &["run"],
        },
        ToolDef {
            name: "harness",
            description: "Versioned harness snapshot + gate ledger row.",
            params: &[],
            required: &[],
        },
    ];
    TOOLS
        .iter()
        .map(|t| {
            let mut properties = json!({});
            for (name, ty) in t.params {
                properties[name] = json!({ "type": ty });
            }
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": { "type": "object", "properties": properties, "required": t.required },
            })
        })
        .collect()
}

fn handle_tools_call(params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let root = super::root();
    let text = call_tool(name, &args, &root);
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn call_tool(name: &str, args: &Value, root: &Path) -> String {
    macro_rules! arg {
        ($key:literal) => {
            args.get($key).and_then(Value::as_str).unwrap_or("")
        };
    }
    macro_rules! opt_arg {
        ($key:literal) => {
            args.get($key).and_then(Value::as_str)
        };
    }
    match name {
        "memory_consolidate" => {
            let episodic = args.get("episodic").and_then(Value::as_str).unwrap_or("");
            let domain = arg!("domain").to_string();
            let domain = if domain.is_empty() {
                "general".to_string()
            } else {
                domain
            };
            let require_signoff = arg_bool(args, "require_signoff");
            let dry_run = arg_bool(args, "dry_run");
            match super::consolidate_text(
                Path::new(episodic),
                &domain,
                require_signoff,
                dry_run,
                root,
            ) {
                Ok(text) => text,
                Err(msg) => format!("error: {msg}"),
            }
        }
        "memory_signoff" => {
            let queue = arg!("queue");
            let index = arg_u64(args, "index")
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(1);
            let domain = arg!("domain");
            let domain = if domain.is_empty() {
                "general".to_string()
            } else {
                domain.to_string()
            };
            match super::signoff_text(Path::new(queue), index, &domain, root) {
                Ok(text) => text,
                Err(msg) => format!("error: {msg}"),
            }
        }
        "memory_derive" => {
            let brief_only = arg_bool(args, "brief_only");
            match super::derive_text(brief_only, root) {
                Ok(text) => text,
                Err(msg) => format!("error: {msg}"),
            }
        }
        "provenance" => memory::canonical_fingerprint(root),
        "eval_score" => {
            let run = arg!("run");
            match eval::score_run(Path::new(run), root, &root.join("evals/golden")) {
                Ok(report) => serde_json::to_string_pretty(&report).unwrap_or_default(),
                Err(e) => format!("error: {e}"),
            }
        }
        "eval_gate" => {
            let tolerance = arg_f64(args, "tolerance").unwrap_or(0.05);
            let mismatch_tolerance =
                usize::try_from(arg_u64(args, "mismatch_tolerance").unwrap_or(1)).unwrap_or(1);
            let write_baseline = arg_bool(args, "write_baseline");
            match super::eval_gate_text(root, tolerance, mismatch_tolerance, write_baseline) {
                Ok(text) => text,
                Err(msg) => format!("error: {msg}"),
            }
        }
        "skill_list" => match skills::discover_skills(root) {
            Ok(reg) => reg
                .iter()
                .map(|s| {
                    let hook = if s.verify.is_some() { "verify" } else { "ref" };
                    format!("{}  [{hook}]  {}", s.name, s.description)
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("error: {e}"),
        },
        "skill_show" => match skills::find_skill(root, arg!("name")) {
            Ok(skill) => format!(
                "name: {}\ndescription: {}\nverify: {}\npath: {}",
                skill.name,
                skill.description,
                skill.verify.as_deref().unwrap_or("(none — reference only)"),
                skill.path.display()
            ),
            Err(e) => format!("error: {e}"),
        },
        "skill_verify" => match skills::find_skill(root, arg!("name"))
            .and_then(|s| skills::verify_skill(&s, root))
        {
            Ok(result) if result.passed => "PASS".to_string(),
            Ok(result) => format!("FAIL (exit {:?})\n{}", result.exit_code, result.output),
            Err(e) => format!("error: {e}"),
        },
        "skill_add" => match skills::install_skills(root, arg!("source")) {
            Ok(installed) => installed
                .iter()
                .map(|name| format!("installed: {name}"))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("error: {e}"),
        },
        "checkpoint_audit" => match super::checkpoint_audit_text(root) {
            Ok(text) => text,
            Err(e) => format!("error: {e}"),
        },
        "validate" => {
            let contract_name = arg!("contract");
            let document = arg!("document");
            match super::validate_doc_text(contract_name, Path::new(document)) {
                Ok(text) => text,
                Err(e) => format!("error: {e}"),
            }
        }
        "stats" => match mini_agi_core::metrics::stats(root) {
            Ok(report) => format!(
                "canonical entries: {}\ncanonical facts: {}\nderived views: {}\ngate: PASS",
                report.entries, report.facts, report.derived_views
            ),
            Err(e) => format!("error: {e}"),
        },
        "budget" => {
            let report = mini_agi_core::metrics::budget(root);
            format!(
                "AGENTS chain: {}B ({}% of 32KiB cap)\nSkills list: {}B for {} skills ({}% of 2% budget)\nMemory leverage: canonical {}B -> brief {}B (x{})",
                report.agents_chain_bytes,
                report.chain_pct_of_32k,
                report.skills_list_bytes,
                report.skills_count,
                report.skills_pct_of_budget,
                report.canonical_bytes,
                report.brief_bytes,
                report.leverage_ratio
            )
        }
        "ticket_list" => match ticket::list_tickets(root) {
            Ok(tickets) => tickets
                .iter()
                .map(|t| format!("{}  {}  scope: {}", t.id, t.title, t.scope.join(", ")))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("error: {e}"),
        },
        "run_ingest" => {
            let run = arg!("run");
            let retro = {
                let r = arg!("retro");
                if r.is_empty() {
                    None
                } else {
                    Some(Path::new(r))
                }
            };
            match super::ingest_text(root, Path::new(run), retro) {
                Ok(text) => text,
                Err(msg) => format!("error: {msg}"),
            }
        }
        "insights" => match insights::insights(root) {
            Ok(report) => {
                let mut lines = vec![format!(
                    "runs: {} (composite avg {:.4}, {} tokens, {:.4} USD)",
                    report.runs, report.composite_avg, report.tokens_total, report.cost_total
                )];
                for case in &report.cases {
                    lines.push(format!("  {}: {:.4}", case.case, case.composite));
                }
                lines.push(format!(
                    "memory: {} entries, {} facts",
                    report.entries, report.facts
                ));
                lines.push(format!("tickets: {}", report.tickets));
                lines.push(format!(
                    "journal: {} begins, {} passes, {} fails, {} status",
                    report.journal[0], report.journal[1], report.journal[2], report.journal[3]
                ));
                if report.gaps.is_empty() {
                    lines.push("capability gaps: none".to_string());
                } else {
                    lines.push("capability gaps (roadmap, ADR-0005):".to_string());
                    for gap in &report.gaps {
                        lines.push(format!("  {gap}"));
                    }
                }
                lines.join("\n")
            }
            Err(e) => format!("error: {e}"),
        },
        "backlog" => match insights::backlog(root) {
            Ok(items) => {
                if items.is_empty() {
                    "no capability gaps — roadmap is clear".to_string()
                } else {
                    items
                        .iter()
                        .map(|i| {
                            if i.created {
                                format!("created: {} — gap: {}", i.id, i.case)
                            } else {
                                format!("exists: {} — gap: {}", i.id, i.case)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            Err(e) => format!("error: {e}"),
        },
        "resume" => match insights::resume(root) {
            Ok(block) => block,
            Err(e) => format!("error: {e}"),
        },
        "loop_status" => match mini_agi_core::loopcmd::status(root) {
            Ok(report) => {
                let mut lines = vec![format!(
                    "{} runs, composite avg {:.4}, {} cases below target",
                    report.runs,
                    report.composite_avg,
                    report.cases.len()
                )];
                for row in &report.cases {
                    lines.push(format!(
                        "  {:.4}  {:<24} ticket={:?} lease={:?} rerun={:?}",
                        row.composite, row.case, row.ticket, row.claimant, row.rerun_composite
                    ));
                }
                lines.join("\n")
            }
            Err(e) => format!("error: {e}"),
        },
        "health" => match mini_agi_core::health::health(root) {
            Ok(report) => {
                let mut lines = vec![format!("HEALTH CHECK — {}", report.verdict())];
                if let Some(load1) = report.load1 {
                    lines.push(format!("  load1: {load1:.2} on {} cores", report.nproc));
                }
                if report.findings.is_empty() {
                    lines.push("  no findings".to_string());
                }
                for finding in &report.findings {
                    lines.push(format!("  [{}] {}", finding.severity, finding.message));
                }
                lines.join("\n")
            }
            Err(e) => format!("error: {e}"),
        },
        "audit" => match mini_agi_core::audit::audit(root) {
            Ok(report) => {
                let mut lines = vec![format!("AUDIT CHECK — {}", report.verdict())];
                for line in &report.passed {
                    lines.push(format!("  [ok] {line}"));
                }
                for finding in &report.findings {
                    lines.push(format!("  [{}] {}", finding.severity, finding.message));
                }
                lines.join("\n")
            }
            Err(e) => format!("error: {e}"),
        },
        "ticket_claim" => {
            let id = arg!("id");
            let claimant = arg!("claimant");
            match ticket::claim_ticket(root, id, claimant, false) {
                Ok(claim) => format!(
                    "claimed: {} by {} since {}",
                    claim.ticket, claim.claimant, claim.since
                ),
                Err(e) => format!("error: {e}"),
            }
        }
        "ticket_release" => {
            let id = arg!("id");
            let claimant = arg!("claimant");
            match ticket::release_ticket(root, id, claimant) {
                Ok(()) => format!("released: {id}"),
                Err(e) => format!("error: {e}"),
            }
        }
        "ticket_claims" => match ticket::read_claims(root) {
            Ok(claims) => {
                if claims.is_empty() {
                    "no claims held".to_string()
                } else {
                    claims
                        .iter()
                        .map(|c| {
                            format!("{} claimed by {} since {}", c.ticket, c.claimant, c.since)
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            Err(e) => format!("error: {e}"),
        },
        "ticket_graph" => match ticket::list_tickets(root) {
            Ok(tickets) => tickets
                .iter()
                .flat_map(|t| {
                    t.blocked_by
                        .iter()
                        .map(move |dep| format!("{dep} -> {}", t.id))
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("error: {e}"),
        },
        "loop_dispatch" => {
            let claimant = arg!("claimant");
            let case = arg!("case");
            let case = if case.is_empty() { None } else { Some(case) };
            match mini_agi_core::loopcmd::dispatch(root, case, 0.5, claimant) {
                Ok(outcome) => format!(
                    "dispatched: {} -> {} (spec: {})",
                    outcome.case,
                    outcome.ticket,
                    outcome.spec.display()
                ),
                Err(e) => format!("error: {e}"),
            }
        }
        "loop_objective" => {
            let claimant = arg!("claimant");
            let max_cases = arg!("max_cases").parse::<usize>().unwrap_or(3);
            let budget_cost = arg!("budget_cost");
            let budget_cost = if budget_cost.is_empty() {
                None
            } else {
                budget_cost.parse::<f64>().ok()
            };
            match mini_agi_core::loopcmd::objective(root, max_cases, claimant, budget_cost) {
                Ok(plan) => format!(
                    "dispatched {} case(s); budget ${:.2}{}",
                    plan.dispatched.len(),
                    plan.budget_spent,
                    plan.budget_cost
                        .map_or_else(String::new, |b| format!(" / ${b:.2}"))
                ),
                Err(e) => format!("error: {e}"),
            }
        }
        "memory_query" => {
            let keyword = arg!("keyword");
            let domain = arg!("domain");
            let keyword = (!keyword.is_empty()).then_some(keyword);
            let domain = (!domain.is_empty()).then_some(domain);
            let facts = mini_agi_core::memory::query_facts(root, domain, keyword);
            if facts.is_empty() {
                "no facts match".to_string()
            } else {
                facts
                    .iter()
                    .map(|(id, d, body)| format!("- `{id}` ({d}) {body}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        "loop_run" => {
            let goal_or_case = arg!("goal_or_case");
            let workdir = std::path::PathBuf::from(arg!("workdir"));
            let approve = arg!("approve");
            if approve.is_empty() {
                return "error: loop_run requires an approval reason (approve) — a write that changes the worker tree".to_string();
            }
            // Parent-side validation mirrors the CLI (resolution +
            // template pairing) so the child starts clean.
            let resolved = match crate::supervisor::resolve(&crate::supervisor::ResolveInput {
                goal_or_case,
                root,
                workdir: &workdir,
                verify: opt_arg!("verify"),
                target: opt_arg!("target").map(std::path::PathBuf::from).as_deref(),
            }) {
                Ok(r) => r,
                Err(e) => return format!("error: {e}"),
            };
            if opt_arg!("blind_worker").is_some_and(|b| b == "true")
                && opt_arg!("hidden_dir").is_none()
            {
                return "error: blind_worker requires hidden_dir".to_string();
            }
            let template = opt_arg!("template");
            if let Some(t) = template
                && t != "sequential-reviewer"
            {
                return format!("error: unknown template '{t}' (supported: sequential-reviewer)");
            }
            let verify_cmd = if let Some(v) = opt_arg!("verify") {
                v.to_string()
            } else {
                let v = resolved.verify_cmd;
                if v.is_empty() {
                    return "error: cannot resolve a verifier for this goal".to_string();
                }
                v
            };
            let target = resolved.target;
            let report = opt_arg!("report")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| workdir.join("REPORT.md"));
            match crate::bg::spawn_detached(
                goal_or_case,
                &workdir,
                &verify_cmd,
                &target,
                opt_arg!("iterate")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(3),
                opt_arg!("max_wall").and_then(|s| s.parse().ok()),
                opt_arg!("max_idle").and_then(|s| s.parse().ok()),
                opt_arg!("blind_worker").is_some_and(|b| b == "true"),
                opt_arg!("hidden_dir")
                    .map(std::path::PathBuf::from)
                    .as_deref(),
                opt_arg!("on_done"),
                &report,
                template,
                opt_arg!("no_resume").is_some_and(|b| b == "true"),
                opt_arg!("no_sandbox").is_some_and(|b| b == "true"),
            ) {
                Ok(handle) => format!(
                    "launched: handle={} pid={} (approved: {approve})",
                    handle.display(),
                    std::fs::read_to_string(handle.join("run.pid"))
                        .ok()
                        .map_or_else(|| "?".to_string(), |p| p.trim().to_string())
                ),
                Err(e) => format!("error: cannot launch detached run: {e}"),
            }
        }
        "run_status" => {
            let handle = std::path::PathBuf::from(arg!("handle"));
            let st = crate::bg::run_status(&handle);
            serde_json::json!({
                "alive": st.alive,
                "workdir": st.workdir,
                "report": st.report,
                "report_ready": st.report_ready,
                "progress_tail": st.progress_tail,
            })
            .to_string()
        }
        "run_report" => {
            let handle = std::path::PathBuf::from(arg!("handle"));
            crate::bg::run_report_text(&handle).map_or_else(
                || "error: report not ready or handle missing".to_string(),
                |text| serde_json::json!({ "report": text }).to_string(),
            )
        }
        "loop_verify" => {
            let case = arg!("case");
            let claimant = arg!("claimant");
            match mini_agi_core::loopcmd::verify(root, case, claimant, false) {
                Ok((text, _)) => text,
                Err(e) => format!("error: {e}"),
            }
        }
        "eval_steps" => {
            let run = arg!("run");
            match std::fs::read_to_string(run) {
                Ok(text) => match serde_json::from_str::<mini_agi_core::eval::Run>(&text) {
                    Ok(run) => {
                        let verdicts = mini_agi_core::eval::score_steps(&run);
                        verdicts
                            .iter()
                            .map(|v| {
                                format!(
                                    "step {} [{}] score {:.2}{}",
                                    v.step,
                                    v.tool,
                                    v.score,
                                    if v.suspicious { "  <-- SUSPICIOUS" } else { "" }
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                    Err(e) => format!("error: invalid run json: {e}"),
                },
                Err(e) => format!("error: {e}"),
            }
        }
        "run_verify" => {
            let run = arg!("run");
            match mini_agi_core::verifier::verify_run(root, std::path::Path::new(run)) {
                Ok(v) => format!(
                    "verify {}: {} (exit {})",
                    v.case,
                    v.status,
                    v.exit_code
                        .map_or_else(|| "-".to_string(), |c| c.to_string())
                ),
                Err(e) => format!("error: {e}"),
            }
        }
        "run_failures" => {
            let run = arg!("run");
            match mini_agi_core::failure::analyze_run(std::path::Path::new(run), root) {
                Ok((case, entries)) => {
                    if entries.is_empty() {
                        format!("no repeated failing actions in {case}")
                    } else {
                        match mini_agi_core::failure::update_register(root, &entries) {
                            Ok(total) => format!(
                                "recorded {} repeated failing actions in {case} (register total {total})",
                                entries.len()
                            ),
                            Err(e) => format!("error: {e}"),
                        }
                    }
                }
                Err(e) => format!("error: {e}"),
            }
        }
        "harness" => match mini_agi_core::harness::snapshot(root) {
            Ok((name, verdict)) => format!("harness snapshot: {name}\n  {verdict}"),
            Err(e) => format!("error: {e}"),
        },
        "ticket_show" | "ticket_validate" => {
            let id = arg!("id");
            match ticket::find_ticket(root, id) {
                Ok(t) if name == "ticket_show" => format!(
                    "id: {}\ntitle: {}\ngoal: {}\nscope: {}",
                    t.id,
                    t.title,
                    t.goal,
                    t.scope.join(", ")
                ),
                Ok(t) => format!(
                    "ok: {} ({}) validates against the ticket contract",
                    t.id, t.title
                ),
                Err(e) => format!("error: {e}"),
            }
        }
        other => format!("error: unknown tool '{other}'"),
    }
}

/// Parse an argument as u64, accepting a JSON number or a numeric string.
fn arg_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
}

/// Parse an argument as bool, accepting a JSON bool or "true"/"false".
fn arg_bool(args: &Value, key: &str) -> bool {
    args.get(key)
        .and_then(|v| {
            v.as_bool()
                .or_else(|| v.as_str().map(|s| s == "true" || s == "1"))
        })
        .unwrap_or(false)
}

/// Parse an argument as f64, accepting a JSON number or numeric string.
fn arg_f64(args: &Value, key: &str) -> Option<f64> {
    args.get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg_parsers_accept_numbers_and_strings() {
        let args = json!({
            "index": "2",
            "flag": "true",
            "rate": "0.5",
            "num": 3,
            "bool": true,
        });
        assert_eq!(arg_u64(&args, "index"), Some(2));
        assert_eq!(arg_u64(&args, "num"), Some(3));
        assert_eq!(arg_u64(&args, "missing"), None);
        assert!(arg_bool(&args, "flag"));
        assert!(arg_bool(&args, "bool"));
        assert!(!arg_bool(&args, "missing"));
        assert_eq!(arg_f64(&args, "rate"), Some(0.5));
        assert_eq!(arg_f64(&args, "missing"), None);
    }

    #[test]
    fn supervisor_tools_are_declared_with_required_params() {
        let tools = tool_definitions();
        let run = tools
            .iter()
            .find(|t| t["name"] == "loop_run")
            .unwrap_or_else(|| panic!("loop_run must be declared"));
        let props = &run["inputSchema"]["properties"];
        for key in [
            "goal_or_case",
            "workdir",
            "verify",
            "target",
            "iterate",
            "template",
            "approve",
        ] {
            assert!(
                props.get(key).is_some(),
                "loop_run must declare param {key}"
            );
        }
        assert_eq!(
            props["iterate"]["type"], "integer",
            "iterate must be typed integer"
        );
        let req = run["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        assert!(req.contains(&"goal_or_case"));
        assert!(req.contains(&"workdir"));
        assert!(req.contains(&"approve"), "HITL approval reason required");
        let status = tools.iter().find(|t| t["name"] == "run_status").unwrap();
        assert!(status["inputSchema"]["properties"].get("handle").is_some());
        let report = tools.iter().find(|t| t["name"] == "run_report").unwrap();
        assert!(report["inputSchema"]["properties"].get("handle").is_some());
    }

    #[test]
    fn loop_run_without_approval_reason_is_refused() {
        // The HITL gate: a write that changes the worker tree requires
        // an approval reason; missing -> error, no child spawned.
        let root = std::env::temp_dir().join(format!("mag-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let args = serde_json::json!({
            "goal_or_case": "some case",
            "workdir": root.to_string_lossy(),
        });
        let out = call_tool("loop_run", &args, &root);
        assert!(
            out.starts_with("error: loop_run requires an approval reason"),
            "{out}"
        );
        assert!(
            !root.join(".supervisor").exists(),
            "no detached child must be spawned without approval"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tool_schemas_declare_real_types() {
        let tools = tool_definitions();
        let signoff = tools
            .iter()
            .find(|t| t["name"] == "memory_signoff")
            .unwrap();
        assert_eq!(
            signoff["inputSchema"]["properties"]["index"]["type"],
            "integer"
        );
        assert_eq!(
            signoff["inputSchema"]["properties"]["queue"]["type"],
            "string"
        );
        let gate = tools.iter().find(|t| t["name"] == "eval_gate").unwrap();
        assert_eq!(
            gate["inputSchema"]["properties"]["tolerance"]["type"],
            "number"
        );
        let derive = tools.iter().find(|t| t["name"] == "memory_derive").unwrap();
        assert_eq!(
            derive["inputSchema"]["properties"]["brief_only"]["type"],
            "boolean"
        );
    }

    #[test]
    fn dispatch_rejects_tools_before_initialize() {
        let mut initialized = false;
        let msg = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}});
        let resp = dispatch(&msg, &mut initialized).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let init = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}});
        assert!(dispatch(&init, &mut initialized).is_some());
        let msg2 = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
        let resp2 = dispatch(&msg2, &mut initialized).unwrap();
        assert!(resp2["result"]["tools"].is_array());
    }

    #[test]
    fn notifications_get_no_response() {
        let mut initialized = true;
        let note = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert!(dispatch(&note, &mut initialized).is_none());
    }
}
