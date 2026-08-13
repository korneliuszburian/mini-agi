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
    /// Write tool: the kernel refuses it without an approve reason (HITL).
    requires_approval: bool,
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
        requires_approval: true,
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
        requires_approval: true,
    },
    ToolDef {
        name: "memory_derive",
        description: "Regenerate derived views from canonical. Requires an approval reason.",
        params: &[("brief_only", "boolean"), ("approve", "string")],
        requires_approval: true,
    },
    ToolDef {
        name: "memory_query",
        description: "Retrieve canonical facts by keyword/domain.",
        params: &[("keyword", "string"), ("domain", "string")],
        requires_approval: false,
    },
    ToolDef {
        name: "provenance",
        description: "Print the canonical fingerprint.",
        params: &[],
        requires_approval: false,
    },
    ToolDef {
        name: "skill_list",
        description: "List discovered patterns/skills.",
        params: &[],
        requires_approval: false,
    },
    ToolDef {
        name: "skill_show",
        description: "Show one pattern.",
        params: &[("name", "string")],
        requires_approval: false,
    },
    ToolDef {
        name: "skill_add",
        description: "Install patterns from a git source. Requires an approval reason.",
        params: &[("source", "string"), ("approve", "string")],
        requires_approval: true,
    },
    ToolDef {
        name: "checkpoint_audit",
        description: "Checkpoint journal audit.",
        params: &[],
        requires_approval: false,
    },
    ToolDef {
        name: "loop_status",
        description: "Open gaps with tickets/claims.",
        params: &[],
        requires_approval: false,
    },
    ToolDef {
        name: "loop_dispatch",
        description: "Dispatch the worst open gap. Requires an approval reason.",
        params: &[
            ("claimant", "string"),
            ("case", "string"),
            ("approve", "string"),
        ],
        requires_approval: true,
    },
    ToolDef {
        name: "loop_objective",
        description: "Batch-dispatch open gaps. Requires an approval reason.",
        params: &[
            ("max_cases", "integer"),
            ("claimant", "string"),
            ("approve", "string"),
        ],
        requires_approval: true,
    },
    ToolDef {
        name: "loop_verify",
        description: "Verify a rerun; close when its gate passes. Requires an approval reason (it executes the case's declared gate shell and writes the ledger).",
        params: &[
            ("case", "string"),
            ("claimant", "string"),
            ("approve", "string"),
        ],
        requires_approval: true,
    },
    ToolDef {
        name: "dream",
        description: "Distill a research file into canonical facts.",
        params: &[("source", "string"), ("approve", "string")],
        requires_approval: true,
    },
];

/// All registry tool names (the single source for `.codex/config.toml`
/// regeneration — MUST-FIX 2: init must not hardcode a stale list).
#[must_use]
pub fn tool_names() -> Vec<&'static str> {
    TOOLS.iter().map(|t| t.name).collect()
}

/// The write tools that require an approval reason (HITL, ADR-0010).
#[must_use]
pub fn approval_tool_names() -> Vec<&'static str> {
    TOOLS
        .iter()
        .filter(|t| t.requires_approval)
        .map(|t| t.name)
        .collect()
}

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

/// Read one line BOUNDED at `MAX_FRAME_BYTES`: `BufRead::read_line`
/// allocates the whole line before any cap check, so an unterminated
/// multi-GB line would allocate without limit. Reads byte-by-byte into a
/// buffer and errors as soon as the cap is crossed.
fn read_bounded_line<R: BufRead>(input: &mut R) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    loop {
        let (consumed, complete) = {
            let buf = input.fill_buf()?;
            if buf.is_empty() {
                return if bytes.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
                };
            }
            if let Some(i) = buf.iter().position(|&b| b == b'\n') {
                bytes.extend_from_slice(&buf[..=i]);
                (i + 1, true)
            } else {
                if bytes.len() + buf.len() > MAX_FRAME_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "frame too large",
                    ));
                }
                bytes.extend_from_slice(buf);
                (buf.len(), false)
            }
        };
        input.consume(consumed);
        if complete {
            return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
}

fn read_frame<R: BufRead>(input: &mut R) -> Result<Option<(Value, Framing)>, io::Error> {
    let Some(first) = read_bounded_line(input)? else {
        return Ok(None);
    };
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
            let Some(header) = read_bounded_line(input)? else {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF in headers",
                ));
            };
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
        // Newline framing must respect the same frame bound as
        // Content-Length — an unbounded line allocates without limit.
        if first.len() > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame too large",
            ));
        }
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
                .map(|t| {
                    let mut properties = json!({});
                    for (name, ty) in t.params {
                        properties[name] = json!({ "type": ty });
                    }
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": { "type": "object", "properties": properties },
                    })
                })
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

/// Resolve a caller-supplied file path against the repo root and reject
/// anything that escapes it — an MCP client must not read arbitrary
/// filesystem files into canonical memory (credential exfiltration).
/// Relative paths resolve inside the root; absolute paths are confined
/// lexically after canonicalization.
fn contained_path(root: &Path, declared: &str) -> String {
    let p = Path::new(declared);
    let candidate = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(declared)
    };
    let root_canon = root.canonicalize().unwrap_or_default();
    // Return the CANONICAL path (like loopcmd::resolve_target): the
    // caller opens THIS, never the raw string. This narrows the symlink-
    // swap window to a concurrent local writer racing the kernel — it is
    // defense-in-depth, not a closed TOCTOU.
    match candidate.canonicalize() {
        Ok(c) if !root_canon.as_os_str().is_empty() && c.starts_with(&root_canon) => {
            c.to_string_lossy().into_owned()
        }
        _ => String::new(),
    }
}

fn read_arg_file(root: &Path, args: &Value, key: &str) -> String {
    let declared = arg(args, key);
    let path = contained_path(root, declared);
    if path.is_empty() {
        return format!("error: {key} path '{declared}' is outside the repo root");
    }
    match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => format!("error: {e}"),
    }
}

fn call_tool(name: &str, args: &Value, root: &Path) -> String {
    use std::fmt::Write as _;
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
            let buffer = read_arg_file(root, args, "episodic");
            if buffer.starts_with("error:") {
                return buffer;
            }
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
            // A MISSING index must refuse, never silently promote #1 —
            // a malformed HITL write would promote an unintended fact.
            let Some(index) = args
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|i| usize::try_from(i).ok())
            else {
                return "error: memory_signoff requires a numeric index".into();
            };
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
            if arg(args, "approve").is_empty() {
                return "error: loop_verify requires an approval reason (approve) — it executes the declared gate and writes the ledger".into();
            }
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
            let text = read_arg_file(root, args, "source");
            if text.starts_with("error:") {
                return text;
            }
            let source = arg(args, "source");
            let staged = mini_agi_core::dream::parse_distilled_facts(&text);
            // ADR-0010 D2: enforcement-bound facts always route to the
            // human queue (the distiller path has no strong-model audit);
            // only the rest consolidate into canonical.
            let mut buffer = String::new();
            let mut queued = 0usize;
            for f in &staged {
                if f.body.contains("enforced_by") {
                    let h = mini_agi_core::hash::fact_id(&f.body);
                    let q = root.join("memory/review").join(format!(
                        "contested-{}.md",
                        mini_agi_core::memory::utc_now_date()
                    ));
                    let already = mini_agi_core::memory::queued_facts(&q)
                        .iter()
                        .any(|(d, _)| *d == h);
                    if !already
                        && let Err(e) = mini_agi_core::memory::append_contested(
                            root,
                            &f.body,
                            &h,
                            source,
                            "0000000000000000",
                        )
                    {
                        return format!("error: dream queue write failed: {e}");
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
            match mini_agi_core::memory::consolidate(root, &buffer, source, &opts) {
                Ok(o) => format!(
                    "dream: {} facts distilled ({} enforcement-bound queued for human review)",
                    o.new_facts, queued
                ),
                Err(e) => format!("error: {e}"),
            }
        }
        _ => format!("error: unknown tool '{name}'"),
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn every_tool_exposes_an_input_schema() {
        // MCP agents read inputSchema to know a tool's arguments; a tool
        // without one is un-callable by real hosts. tools/list must emit
        // inputSchema (with properties) for every registered tool.
        let mut initialized = false;
        let init_msg = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}});
        dispatch(&init_msg, &mut initialized).unwrap();
        let msg = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}});
        let resp = dispatch(&msg, &mut initialized).unwrap();
        let tools = resp["result"]["tools"].as_array().expect("tools array");
        assert!(!tools.is_empty());
        for tool in tools {
            let schema = tool
                .get("inputSchema")
                .unwrap_or_else(|| panic!("{}: no inputSchema", tool["name"]));
            assert!(
                schema.get("type").is_some(),
                "{}: inputSchema needs a type",
                tool["name"]
            );
            assert!(
                schema.get("properties").is_some(),
                "{}: inputSchema needs properties",
                tool["name"]
            );
        }
    }

    #[test]
    fn tools_list_matches_the_registry_exactly() {
        let mut initialized = false;
        let init = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}});
        dispatch(&init, &mut initialized).unwrap();
        let msg = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}});
        let resp = dispatch(&msg, &mut initialized).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names.len(), 14, "14-tool registry, got {names:?}");
        for name in tool_names() {
            assert!(names.contains(&name.to_string()), "{name} advertised");
        }
    }

    #[test]
    fn tools_list_and_call_require_the_initialize_handshake() {
        let mut initialized = false;
        let list = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}});
        let resp = dispatch(&list, &mut initialized).unwrap();
        assert!(resp["result"]["isError"].as_bool().unwrap());
        assert!(
            resp["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("not initialized")
        );
        let call =
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_query"}});
        let resp = dispatch(&call, &mut initialized).unwrap();
        assert!(resp["result"]["isError"].as_bool().unwrap());
    }

    #[test]
    fn dispatch_rejects_unknown_methods_and_tools() {
        let mut initialized = false;
        let init = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{}});
        dispatch(&init, &mut initialized).unwrap();
        let msg = json!({"jsonrpc":"2.0","id":1,"method":"bogus/method","params":{}});
        let resp = dispatch(&msg, &mut initialized).unwrap();
        assert!(
            resp["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("unknown method")
        );
        let root = std::env::temp_dir();
        assert!(call_tool("bogus_tool", &json!({}), &root).contains("unknown tool"));
    }

    #[test]
    fn write_tools_refuse_without_an_approval_reason() {
        let root = std::env::temp_dir();
        for (tool, args) in [
            ("loop_dispatch", json!({"claimant": "t"})),
            ("loop_objective", json!({"claimant": "t", "max_cases": 1})),
            ("skill_add", json!({"source": "https://example.invalid/r"})),
        ] {
            let text = call_tool(tool, &args, &root);
            assert!(
                text.contains("requires an approval reason"),
                "{tool}: {text}"
            );
        }
    }
}
