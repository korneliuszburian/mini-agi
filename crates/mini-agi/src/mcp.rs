//! stdio MCP server (Model Context Protocol, JSON-RPC 2.0).
//!
//! Hand-rolled, zero dependencies: LSP-style `Content-Length` framing over
//! stdio, protocol version `2025-03-26`. Exposes the kernel as tools so
//! Codex, Claude, Cursor and opencode plug into the SAME verified brain
//! through the standard protocol (PLAN, Phase 4).

use std::io::{self, BufRead, Write};
use std::path::Path;

use mini_agi_core::{eval, memory, skills};
use serde_json::{Value, json};

const PROTOCOL_VERSION: &str = "2025-03-26";
const SERVER_NAME: &str = "mini-agi";

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

/// Read one `Content-Length` framed JSON message; `None` on clean EOF.
fn read_frame<R: BufRead>(input: &mut R) -> Result<Option<Value>, io::Error> {
    let mut length: Option<usize> = None;
    loop {
        let mut header = String::new();
        let read = input.read_line(&mut header)?;
        if read == 0 {
            return Ok(None); // EOF before any frame
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(rest) = header.strip_prefix("Content-Length:") {
            length = rest.trim().parse::<usize>().ok();
        }
    }
    let Some(length) = length else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing Content-Length",
        ));
    };
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body).unwrap_or(Value::Null)))
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
    let protocol_version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": protocol_version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
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
            let write_baseline = arg_bool(args, "write_baseline");
            match super::eval_gate_text(root, tolerance, write_baseline) {
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
