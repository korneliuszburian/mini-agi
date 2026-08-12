#![allow(missing_docs)]
//! stdio MCP server (condensed).
//!
//! A small tool surface over the KNOWLEDGE core + the loop: agents
//! query/feed the brain and drive the gap loop. Framing mirrors the
//! client's transport (Content-Length vs newline — EXP-016).

use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::Path;

struct ToolDef {
    name: &'static str,
    description: &'static str,
    #[allow(dead_code)]
    params: &'static [(&'static str, &'static str)],
}

/// The server's tool registry.
const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "memory_consolidate",
        description: "Consolidate an episodic buffer into canonical facts. Requires an approval reason unless dry_run.",
        params: &[
            ("episodic", "string"),
            ("domain", "string"),
            ("require_signoff", "boolean"),
            ("dry_run", "boolean"),
            ("approve", "string"),
        ],
    },
    ToolDef {
        name: "memory_signoff",
        description: "Promote one contested fact from the review queue. Requires an approval reason.",
        params: &[
            ("queue", "string"),
            ("index", "integer"),
            ("domain", "string"),
            ("approve", "string"),
        ],
    },
    ToolDef {
        name: "memory_derive",
        description: "Regenerate derived views from canonical. Requires an approval reason.",
        params: &[("brief_only", "boolean"), ("approve", "string")],
    },
    ToolDef {
        name: "memory_query",
        description: "Retrieve canonical facts by keyword/domain.",
        params: &[("keyword", "string"), ("domain", "string")],
    },
    ToolDef {
        name: "provenance",
        description: "Print the canonical fingerprint.",
        params: &[],
    },
    ToolDef {
        name: "skill_list",
        description: "List discovered patterns/skills.",
        params: &[],
    },
    ToolDef {
        name: "skill_show",
        description: "Show one pattern.",
        params: &[("name", "string")],
    },
    ToolDef {
        name: "skill_add",
        description: "Install patterns from a git source. Requires an approval reason.",
        params: &[("source", "string"), ("approve", "string")],
    },
    ToolDef {
        name: "checkpoint_audit",
        description: "Checkpoint journal audit.",
        params: &[],
    },
    ToolDef {
        name: "loop_status",
        description: "Open gaps with tickets/claims.",
        params: &[],
    },
    ToolDef {
        name: "loop_dispatch",
        description: "Dispatch the worst open gap. Requires an approval reason.",
        params: &[
            ("claimant", "string"),
            ("case", "string"),
            ("approve", "string"),
        ],
    },
    ToolDef {
        name: "loop_objective",
        description: "Batch-dispatch open gaps. Requires an approval reason.",
        params: &[
            ("max_cases", "integer"),
            ("claimant", "string"),
            ("approve", "string"),
        ],
    },
    ToolDef {
        name: "loop_verify",
        description: "Verify a rerun; close when its gate passes.",
        params: &[("case", "string"), ("claimant", "string")],
    },
    ToolDef {
        name: "dream",
        description: "Distill a research file into canonical facts.",
        params: &[("source", "string"), ("approve", "string")],
    },
];

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framing {
    ContentLength,
    Newline,
}

pub fn run_stdio_server() -> Result<(), io::Error> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut initialized = false;
    while let Some((message, framing)) = read_frame(&mut input)? {
        if let Some(payload) = dispatch(&message, &mut initialized) {
            write_frame(&mut output, &payload, framing)?;
        }
    }
    Ok(())
}

fn read_frame<R: BufRead>(input: &mut R) -> Result<Option<(Value, Framing)>, io::Error> {
    let mut first = String::new();
    if input.read_line(&mut first)? == 0 {
        return Ok(None);
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
                "frame too large",
            ));
        }
        loop {
            let mut header = String::new();
            if input.read_line(&mut header)? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF in headers",
                ));
            }
            if header.trim().is_empty() {
                break;
            }
        }
        let mut body = vec![0u8; length];
        input.read_exact(&mut body)?;
        serde_json::from_slice(&body)
            .map(|v| Some((v, Framing::ContentLength)))
            .map_err(io::Error::other)
    } else if first.starts_with('{') || first.starts_with('[') {
        serde_json::from_str(first)
            .map(|v| Some((v, Framing::Newline)))
            .map_err(io::Error::other)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unrecognized frame",
        ))
    }
}

fn write_frame<W: Write>(
    output: &mut W,
    payload: &Value,
    framing: Framing,
) -> Result<(), io::Error> {
    let body = serde_json::to_vec(payload).map_err(io::Error::other)?;
    match framing {
        Framing::ContentLength => {
            write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
            output.write_all(&body)?;
        }
        Framing::Newline => {
            output.write_all(&body)?;
            output.write_all(b"\n")?;
        }
    }
    output.flush()
}

fn err_response(message: &str) -> Value {
    json!({ "isError": true, "content": [{ "type": "text", "text": message }] })
}

fn dispatch(message: &Value, initialized: &mut bool) -> Option<Value> {
    let id = message.get("id")?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => {
            json!({ "protocolVersion": "2025-03-26", "capabilities": { "tools": {} }, "serverInfo": { "name": "mini-agi", "version": env!("CARGO_PKG_VERSION") } })
        }
        "ping" => json!({}),
        "tools/list" if *initialized => {
            let tools: Vec<Value> = TOOLS
                .iter()
                .map(|t| json!({ "name": t.name, "description": t.description }))
                .collect();
            json!({ "tools": tools })
        }
        "tools/call" if *initialized => handle_tools_call(&params),
        "tools/list" | "tools/call" => err_response("server not initialized"),
        other => err_response(&format!("unknown method '{other}'")),
    };
    if method == "initialize" {
        *initialized = true;
    }
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn arg<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or("")
}

fn arg_bool(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn handle_tools_call(params: &Value) -> Value {
    let name = arg(params, "name");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let root = super::root();
    let text = call_tool(name, &args, &root);
    let is_error = text.starts_with("error:");
    json!({ "isError": is_error, "content": [{ "type": "text", "text": text }] })
}

fn call_tool(name: &str, args: &Value, root: &Path) -> String {
    match name {
        "memory_consolidate" => {
            let dry_run = arg_bool(args, "dry_run");
            if !dry_run && arg(args, "approve").is_empty() {
                return "error: memory_consolidate requires an approval reason (approve) unless dry_run".into();
            }
            let domain = if arg(args, "domain").is_empty() {
                "general".to_string()
            } else {
                arg(args, "domain").to_string()
            };
            let buffer = std::fs::read_to_string(arg(args, "episodic")).unwrap_or_default();
            let opts = mini_agi_core::memory::ConsolidateOptions {
                domain,
                require_signoff: arg_bool(args, "require_signoff"),
                dry_run,
            };
            match mini_agi_core::memory::consolidate(root, &buffer, "mcp", &opts) {
                Ok(o) => format!(
                    "consolidated {} new facts, {} skipped",
                    o.new_facts, o.skipped
                ),
                Err(e) => format!("error: {e}"),
            }
        }
        "memory_signoff" => {
            if arg(args, "approve").is_empty() {
                return "error: memory_signoff requires an approval reason (approve)".into();
            }
            let queue = arg(args, "queue");
            let index = usize::try_from(args.get("index").and_then(Value::as_u64).unwrap_or(1))
                .unwrap_or(1);
            match mini_agi_core::memory::signoff(root, Path::new(queue), index, arg(args, "domain"))
            {
                Ok(e) => format!("promoted {}", e.path.display()),
                Err(e) => format!("error: {e}"),
            }
        }
        "memory_derive" => {
            if arg(args, "approve").is_empty() {
                return "error: memory_derive requires an approval reason (approve)".into();
            }
            match mini_agi_core::memory::derive(root, arg_bool(args, "brief_only")) {
                Ok((_, _, _)) => "derived: views regenerated".into(),
                Err(e) => format!("error: {e}"),
            }
        }
        "memory_query" => {
            let facts = mini_agi_core::memory::query_facts(
                root,
                Some(arg(args, "domain")).filter(|d| !d.is_empty()),
                Some(arg(args, "keyword")).filter(|k| !k.is_empty()),
            );
            if facts.is_empty() {
                "no matching facts".into()
            } else {
                facts
                    .iter()
                    .map(|(id, d, b)| format!("`{id}` [{d}] {b}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        "provenance" => format!(
            "canonical_sha256: {}",
            mini_agi_core::memory::canonical_fingerprint(root)
        ),
        "skill_list" => match mini_agi_core::skills::discover_skills(root) {
            Ok(skills) => skills
                .iter()
                .map(|s| {
                    format!(
                        "{}  [{}]  {}",
                        s.name,
                        if s.verify.is_some() { "verify" } else { "ref" },
                        s.description
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("error: {e}"),
        },
        "skill_show" => match mini_agi_core::skills::find_skill(root, arg(args, "name")) {
            Ok(s) => format!("{}: {}", s.name, s.description),
            Err(e) => format!("error: {e}"),
        },
        "skill_add" => {
            if arg(args, "approve").is_empty() {
                return "error: skill_add requires an approval reason (approve)".into();
            }
            match mini_agi_core::skills::install_skills(root, arg(args, "source")) {
                Ok(v) => format!("installed: {}", v.join(", ")),
                Err(e) => format!("error: {e}"),
            }
        }
        "checkpoint_audit" => {
            let path = root.join("memory/episodic/checkpoints.log");
            let Ok(text) = std::fs::read_to_string(path) else {
                return "checkpoint: absent".into();
            };
            let events = mini_agi_core::journal::parse_journal(&text);
            let audit = mini_agi_core::journal::audit_journal(&events);
            format!("{} events, {} bad", events.len(), audit.bad.len())
        }
        "loop_status" => match mini_agi_core::loopcmd::status(root) {
            Ok(s) => s
                .cases
                .iter()
                .map(|r| format!("{} attempts={} {:?}", r.case, r.attempts, r.status))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("error: {e}"),
        },
        "loop_dispatch" => {
            if arg(args, "approve").is_empty() {
                return "error: loop_dispatch requires an approval reason (approve)".into();
            }
            let case = if arg(args, "case").is_empty() {
                None
            } else {
                Some(arg(args, "case"))
            };
            match mini_agi_core::loopcmd::dispatch(root, case, 0.5, arg(args, "claimant")) {
                Ok(o) => format!(
                    "dispatched {} -> {} — {}",
                    o.case,
                    o.ticket,
                    o.spec.display()
                ),
                Err(e) => format!("error: {e}"),
            }
        }
        "loop_objective" => {
            if arg(args, "approve").is_empty() {
                return "error: loop_objective requires an approval reason (approve)".into();
            }
            let max = usize::try_from(args.get("max_cases").and_then(Value::as_u64).unwrap_or(1))
                .unwrap_or(1);
            match mini_agi_core::loopcmd::objective(root, max, arg(args, "claimant"), None) {
                Ok(o) => format!("dispatched {} case(s)", o.dispatched.len()),
                Err(e) => format!("error: {e}"),
            }
        }
        "loop_verify" => {
            match mini_agi_core::loopcmd::verify(
                root,
                arg(args, "case"),
                arg(args, "claimant"),
                false,
            ) {
                Ok((text, closed)) => format!("{text} (closed: {closed})"),
                Err(e) => format!("error: {e}"),
            }
        }
        "dream" => {
            if arg(args, "approve").is_empty() {
                return "error: dream requires an approval reason (approve)".into();
            }
            let source = arg(args, "source");
            let text = match std::fs::read_to_string(source) {
                Ok(t) => t,
                Err(e) => return format!("error: {e}"),
            };
            let staged = mini_agi_core::dream::parse_distilled_facts(&text);
            let mut buffer = String::new();
            for f in &staged {
                use std::fmt::Write as _;
                let _ = writeln!(buffer, "- {}", f.body);
            }
            let opts = mini_agi_core::memory::ConsolidateOptions {
                domain: "knowledge".into(),
                require_signoff: false,
                dry_run: false,
            };
            match mini_agi_core::memory::consolidate(root, &buffer, source, &opts) {
                Ok(o) => format!("dream: {} facts distilled", o.new_facts),
                Err(e) => format!("error: {e}"),
            }
        }
        _ => format!("error: unknown tool '{name}'"),
    }
}
