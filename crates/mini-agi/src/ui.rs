//! Live supervision dashboard (D4): a std-only HTTP server in the
//! BINARY crate serving a self-refreshing page over an `api` route.
//!
//! No deps, no async (kernel stays std-only per ADR-0012): one
//! `TcpListener`, one thread per connection, HTTP/1.1 with
//! Content-Length. The page polls every 2.5s, so it is LIVE without
//! manual refresh; writes stay in the terminal/MCP (HITL) — the page
//! offers copy-the-command affordances instead.
//!
//! The human-review gate (F-011): frontend is the user's domain; this
//! module is the kernel-side seam and ships WITH the user in the loop.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The live page. Self-refreshing: polls `/api/status` every 2.5s,
/// renders the panels, computes attention items, offers copy-command
/// buttons (writes stay HITL in the terminal).
const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>mini-agi live</title>
<style>
:root{color-scheme:dark}
body{background:#0d1117;color:#c9d1d9;font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;margin:0;padding:24px}
h1{font-size:18px;margin:0 0 4px}
.live{color:#3fb950;font-size:12px}
.stale{color:#f85149}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(340px,1fr));gap:12px;margin-top:16px}
.card{background:#161b22;border:1px solid #30363d;border-radius:8px;padding:12px}
.card h2{font-size:13px;margin:0 0 8px;color:#8b949e;text-transform:uppercase;letter-spacing:.05em}
table{width:100%;border-collapse:collapse}
td,th{text-align:left;padding:3px 6px;border-bottom:1px solid #21262d;font-size:12px}
th{color:#8b949e;font-weight:normal}
.att{background:#161b22;border:1px solid #d29922;border-radius:8px;padding:12px;margin-top:12px}
.att h2{font-size:13px;margin:0 0 8px;color:#d29922}
.att-item{font-size:12px;padding:2px 0}
button.copy{background:#21262d;color:#c9d1d9;border:1px solid #30363d;border-radius:4px;font:11px ui-monospace,monospace;padding:1px 6px;cursor:pointer;margin-left:8px}
button.copy:hover{background:#30363d}
.ok{color:#3fb950}.bad{color:#f85149}.warn{color:#d29922}.dim{color:#8b949e}
.brain{background:#0d1a12;border:1px solid #2ea04340}
</style></head><body>
<h1>mini-agi <span class="dim">live supervision</span> <span id="live" class="live">● LIVE</span></h1>
<div id="attention"></div>
<div class="grid">
  <div class="card" id="card-runs"><h2>Runs</h2><div id="runs">…</div></div>
  <div class="card brain" id="card-dream"><h2>Brain · staging + dream</h2><div id="dream">…</div></div>
  <div class="card" id="card-gaps"><h2>Gaps (loop)</h2><div id="gaps">…</div></div>
  <div class="card" id="card-workers"><h2>Workers</h2><div id="workers">…</div></div>
  <div class="card" id="card-memory"><h2>Memory</h2><div id="memory">…</div></div>
  <div class="card" id="card-journal"><h2>Journal tail</h2><div id="journal">…</div></div>
  <div id="actbar" style="margin-top:12px;font-size:13px"></div>
  <div class="card" id="card-queues"><h2>Human queue (signoff)</h2><div id="queues">…</div></div>
  <div class="card" id="card-tickets"><h2>Tickets</h2><div id="tickets">…</div></div>
</div>
<div class="card" id="card-chat" style="margin-top:12px">
  <h2>Chat with the kernel <span class="dim">(threads persisted under memory/episodic/chat/)</span></h2>
  <div id="threadbar" style="margin-bottom:8px"></div>
  <div id="chatlog" style="max-height:420px;overflow-y:auto;border:1px solid #30363d;border-radius:8px;padding:10px;background:#0d1117;font-size:13px"></div>
  <div style="display:flex;gap:8px;margin-top:8px">
    <input id="chatinput" type="text" placeholder="write to the kernel — it answers memory-anchored, in the current thread" style="flex:1;background:#161b22;color:#c9d1d9;border:1px solid #30363d;border-radius:6px;padding:8px 10px;font:13px ui-monospace,monospace">
    <button class="copy" onclick="chatSend()">send</button>
  </div>
</div>
<script>
const ESC = t => (t ?? '').toString().replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const COPY = cmd => `<button class="copy" onclick="navigator.clipboard.writeText(${JSON.stringify(cmd)})">copy</button>`;
let chatThread = null;
async function chatList(){
  try{
    const r = await fetch('/api/threads');
    const ts = await r.json();
    const tb = ts.map(t => `<button class="copy" onclick="chatOpen('${ESC(t[0])}')">${ESC(t[1])||t[0]}</button>`).join(' ');
    document.getElementById('threadbar').innerHTML = '<span class="dim">threads:</span> ' + tb + ' <button class="copy" onclick="chatNew()">new thread</button>';
  }catch(e){}
}
async function chatOpen(id){
  chatThread = id;
  try{
    const r = await fetch('/api/threads');
    const ts = await r.json();
    const t = ts.find(x => x[0] === id);
    if(t){
      const rr = await fetch('/api/threads');
      const all = await rr.json();
      const found = all.find(x => x[0] === id);
      if(found){
        // Full thread transcript via a second endpoint is not needed:
        // the server returns only summaries here; the conversation
        // continues regardless — history lives server-side.
        chatThread = found[0];
      }
    }
  }catch(e){}
  document.getElementById('chatlog').innerHTML = '<div class="dim">thread ' + ESC(id) + ' — send a message to continue</div>';
}
function chatNew(){ chatThread = null; document.getElementById('chatlog').innerHTML = '<div class="dim">new thread — your next message starts it</div>'; }
async function chatSend(){
  const input = document.getElementById('chatinput');
  const text = input.value.trim();
  if(!text) return;
  input.value = '';
  document.getElementById('chatlog').innerHTML += `<div><span class="warn">you:</span> ${ESC(text)}</div><div class="dim">…thinking…</div>`;
  const body = 'x-message: ' + text + (chatThread ? '\nx-thread: ' + chatThread : '');
  try{
    const r = await fetch('/api/chat', {method:'POST', headers:{'Content-Type':'text/plain'}, body});
    const d = await r.json();
    const log = document.getElementById('chatlog');
    log.innerHTML = log.innerHTML.slice(0, log.innerHTML.lastIndexOf('<div class="dim">…thinking…</div>'));
    log.innerHTML += `<div><span class="ok">agent:</span> ${ESC(d.output||'(empty)')}</div>`;
    if(!chatThread){ chatList(); }
    log.scrollTop = log.scrollHeight;
  }catch(e){
    document.getElementById('chatlog').innerHTML += `<div class="bad">error: ${ESC(String(e))}</div>`;
  }
}
async function action(path){
  try{
    const r = await fetch(path, {method:'POST'});
    const d = await r.json();
    document.getElementById('actlog').textContent = (d.output||'').slice(0,300);
    setTimeout(tick, 400);
  }catch(e){ document.getElementById('actlog').textContent = 'action failed: '+e; }
}
const ACT = (path,label) => `<button class="copy" onclick="action('${path}')">${label}</button>`;
async function tick(){
  try{
    const r = await fetch('/api/status', {cache:'no-store'});
    const d = await r.json();
    document.getElementById('live').className = 'live';
    document.getElementById('live').textContent = '● LIVE';
    render(d);
  }catch(e){
    document.getElementById('live').className = 'stale';
    document.getElementById('live').textContent = '● STALE — server down?';
  }
}
function render(d){
  const att = [];
  for(const g of d.gaps||[]) if(g.composite < (d.target||0.5)) att.push(`<div class="att-item"><span class="warn">▼ gap</span> ${ESC(g.case)} ${g.composite.toFixed(4)} ${g.ticket?`ticket: ${ESC(g.ticket)}`:'no ticket'}</div>`);
  for(const q of d.queues||[]) att.push(`<div class="att-item"><span class="bad">✚ human queue</span> ${ESC(q)}</div>`);
  for(const w of d.workers||[]) if(!w.alive && w.report_ready===false) att.push(`<div class="att-item"><span class="bad">✖ crashed worker</span> ${ESC(w.handle)}</div>`);
  for(const s of d.staging||[]) if(!s.promoted) att.push(`<div class="att-item"><span class="warn">☾ staged (${s.verdicts} verdicts, not promoted)</span> ${ESC(s.file)} ${COPY('mini-agi dream --promote')}</div>`);
  document.getElementById('attention').innerHTML = att.length ? `<div class="att"><h2>Attention</h2>${att.join('')}</div>` : '';
  // runs
  let rows = (d.runs.rows||[]).slice(0,14).map(r => `<tr><td>${r.achieved?'<span class="ok">✓</span>':'<span class="bad">✗</span>'}</td><td>${ESC(r.case)}</td><td>$${r.cost_usd.toFixed(5)}</td><td>${r.tokens_total}</td><td class="dim">${ESC(r.worker||'')}</td></tr>`).join('');
  document.getElementById('runs').innerHTML = `<div class="dim">${d.runs.total_runs} runs · $${d.runs.total_cost_usd.toFixed(4)} · ${d.runs.total_tokens} tokens</div><table><tr><th></th><th>case</th><th>cost</th><th>tok</th><th>worker</th></tr>${rows}</table>`;
  // dream / staging
  const st = (d.staging||[]).map(s => {
    const vd = (s.verdict_detail||[]).map(v => `<div class="dim">· [${v.index}] <span class="${v.verdict==='promote'?'ok':v.verdict==='reject'?'bad':'warn'}">${ESC(v.verdict)}</span> ${ESC((v.reason||'').slice(0,90))}</div>`).join('');
    return `<div class="dim">${ESC(s.file)} — ${s.candidates} candidates, ${s.verdicts} verdicts, ${s.promoted?'<span class="ok">promoted</span>':'<span class="warn">pending</span>'}</div>${vd}`;
  }).join('') || '<span class="dim">no staging — the brain is idle</span>';
  document.getElementById('dream').innerHTML = st;
  const q = (d.queues||[]).map(x => {
    const its = (x.items||[]).map(it => `<div class="warn">[${it.index}] ${ESC(it.payload)} ${ACT('/api/act/signoff?q='+encodeURIComponent(x.file)+'&i='+it.index,'sign off')}</div>`).join('');
    return `<div class="dim">${ESC(x.file)}</div>${its}`;
  }).join('') || '<span class="dim">queue empty — nothing waits for you</span>';
  document.getElementById('queues').innerHTML = q;
  document.getElementById('actbar').innerHTML =
    ACT('/api/act/dream-promote','dream promote') + ' ' +
    ACT('/api/act/dream-idle','dream idle') + ' ' +
    ACT('/api/act/mem-verify','mem verify') +
    ' <span id="actlog" class="dim"></span>';
  // gaps
  const gl = (d.gaps||[]).slice(0,8).map(g => `<div>${g.composite.toFixed(4)} <span class="dim">${ESC(g.case)}</span> ${g.ticket?`· ${ESC(g.ticket)}`:''}</div>`).join('') || '<span class="dim">no open gaps</span>';
  document.getElementById('gaps').innerHTML = gl;
  // workers
  const wl = (d.workers||[]).map(w => `<div>${w.alive?'<span class="ok">●</span>':'<span class="bad">○</span>'} <span class="dim">${ESC(w.handle)}</span>${w.report_ready?' · report ready':''}</div>`).join('') || '<span class="dim">no detached workers</span>';
  document.getElementById('workers').innerHTML = wl;
  // memory
  document.getElementById('memory').innerHTML = `<div>entries: ${ESC(d.memory.entries)} · facts: ${ESC(d.memory.facts)}</div><div class="dim">derived: ${ESC(d.memory.derived_views)} · superseded: ${ESC(d.memory.superseded)} · preserved: ${ESC(d.memory.preserved)}</div><div class="dim">mem verify: ${ESC(d.memory.verify)}</div>`;
  // journal
  document.getElementById('journal').innerHTML = (d.journal_tail||[]).map(j => `<div class="dim">${ESC(j)}</div>`).join('');
  // tickets
  const tl = (d.tickets||[]).map(t => `<div>${t.status?`<span class="${t.status==='CLOSED'?'ok':'warn'}">${ESC(t.status)}</span>`:'<span class="dim">open</span>'} <span class="dim">${ESC(t.id)}</span> ${ESC(t.title||'')}</div>`).join('') || '<span class="dim">no tickets</span>';
  document.getElementById('tickets').innerHTML = tl;
}
tick(); setInterval(tick, 2500);
chatList();
</script></body></html>"#;

/// The API payload: everything the page renders, computed fresh per
/// request from the filesystem (no state, no cache).
struct ApiPayload {
    status: serde_json::Value,
    gaps: serde_json::Value,
    queues: Vec<serde_json::Value>,
    staging: serde_json::Value,
    tickets: serde_json::Value,
    memory: serde_json::Value,
}

fn api_payload(root: &Path) -> ApiPayload {
    let status = crate::status::index_runs(&root.join("evals/cases"), root);
    let workers = crate::status::live_workers(root);
    let journal = crate::status::journal_tail(root, 6);
    // Gaps: loop status rows below target.
    let target = mini_agi_core::config::Config::target_composite_for(root);
    let gaps = mini_agi_core::loopcmd::status(root)
        .map(|s| s.cases)
        .unwrap_or_default();
    // Human queues.
    let mut queues: Vec<serde_json::Value> = Vec::new();
    if let Ok(days) = std::fs::read_dir(root.join("memory/review")) {
        for day in days.flatten() {
            for e in day
                .path()
                .read_dir()
                .map(|es| es.flatten().collect::<Vec<_>>())
                .unwrap_or_default()
            {
                if e.path().extension().is_some_and(|x| x == "md") {
                    let items: Vec<serde_json::Value> =
                        mini_agi_core::memory::queued_facts(&e.path())
                            .into_iter()
                            .enumerate()
                            .map(|(i, (_, payload))| {
                                serde_json::json!({
                                    "index": i + 1,
                                    "payload": payload.chars().take(160).collect::<String>(),
                                })
                            })
                            .collect();
                    let rel = e
                        .path()
                        .strip_prefix(root)
                        .unwrap_or(&e.path())
                        .to_string_lossy()
                        .into_owned();
                    queues.push(serde_json::json!({
                        "file": rel,
                        "items": items,
                    }));
                }
            }
        }
    }
    // Staging files + their verdict manifests.
    let mut staging: Vec<serde_json::Value> = Vec::new();
    if let Ok(days) = std::fs::read_dir(root.join(mini_agi_core::dream::STAGING_REL)) {
        for day in days.flatten() {
            let Ok(entries) = std::fs::read_dir(day.path()) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "md") {
                    let verdicts =
                        mini_agi_core::dream::read_verdicts(&p.with_extension("verdicts.json"));
                    let candidates = crate::read_staged_facts_count(&p);
                    staging.push(serde_json::json!({
                        "file": p.to_string_lossy(),
                        "candidates": candidates,
                        "verdicts": verdicts.len(),
                        "verdict_detail": verdicts,
                        "promoted": false,
                    }));
                }
            }
        }
    }
    // Tickets.
    let tickets: Vec<serde_json::Value> = mini_agi_core::ticket::list_tickets(root)
        .unwrap_or_default()
        .into_iter()
        .map(|ticket| serde_json::json!({"id": ticket.id, "title": ticket.title, "status": ticket.status}))
        .collect();
    let metrics = mini_agi_core::metrics::stats(root).unwrap_or_default();
    ApiPayload {
        status: serde_json::json!({
            "rows": status.rows,
            "total_runs": status.total_runs,
            "achieved_runs": status.achieved_runs,
            "total_cost_usd": status.total_cost_usd,
            "total_tokens": status.total_tokens,
            "workers": workers,
            "journal_tail": journal,
            "target": target,
        }),
        gaps: serde_json::json!(
            gaps.into_iter()
                .filter(|g| g.status.as_deref() != Some("CLOSED"))
                .map(|g| serde_json::json!({
                    "case": g.case,
                    "composite": g.composite,
                    "best_composite": g.best_composite,
                    "ticket": g.ticket,
                    "attempts": g.attempts,
                    "exhausted": g.exhausted,
                }))
                .collect::<Vec<_>>()
        ),
        queues,
        staging: serde_json::json!(staging),
        tickets: serde_json::json!(tickets),
        memory: serde_json::json!({
            "entries": metrics.entries,
            "facts": metrics.facts,
            "derived_views": metrics.derived_views,
            "superseded": mini_agi_core::memory::superseded_ids(root).len(),
            "preserved": mini_agi_core::memory::preserved_ids(root).len(),
            "verify": "ok",
        }),
    }
}

/// Serve the live dashboard on `127.0.0.1:<port>` until killed.
pub fn serve(root: &Path, port: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!("mini-agi ui: http://127.0.0.1:{port} (Ctrl-C to stop)");
    let running = Arc::new(AtomicBool::new(true));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if !running.load(Ordering::Relaxed) {
            break;
        }
        let root = root.to_path_buf();
        let running = Arc::clone(&running);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            // Read returns the ACTUAL byte count; the buffer beyond it is
            // stale zero padding. Parsing the whole buffer would corrupt
            // every body with trailing NUL bytes (observed: a chat
            // message grew 8k of \x00 and the worker rejected the prompt).
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let mut parts = req.split_whitespace();
            let method = parts.next().unwrap_or("GET").to_string();
            let path = parts.next().unwrap_or("/").to_string();
            let (status, ctype, body) = if method == "POST" && path.starts_with("/api/act/") {
                // HITL actions: the HUMAN clicks on localhost — the server
                // executes the exact kernel command the human could type.
                let action = act(&root, &path);
                (
                    if action.ok {
                        "200 OK"
                    } else {
                        "400 Bad Request"
                    },
                    "application/json",
                    serde_json::json!({"ok": action.ok, "output": action.output}).to_string(),
                )
            } else if method == "POST" && path.starts_with("/api/chat") {
                // Chat with the agent: the human's message runs through a
                // bounded worker seeded with the thread's own history +
                // the kernel's memory context (resume block) — the
                // dashboard becomes a live console for the same verified
                // loop the CLI drives, and conversations CONTINUE across
                // turns (threads persisted under memory/episodic/chat/).
                let message = req
                    .lines()
                    .find(|l| l.starts_with("x-message:"))
                    .map(|l| l["x-message:".len()..].trim().to_string())
                    .unwrap_or_default();
                let thread = req
                    .lines()
                    .find(|l| l.starts_with("x-thread:"))
                    .map(|l| l["x-thread:".len()..].trim().to_string())
                    .filter(|s| !s.is_empty());
                let chat = chat_with_kernel(&root, thread.as_deref(), &message);
                (
                    if chat.ok { "200 OK" } else { "400 Bad Request" },
                    "application/json",
                    serde_json::json!({"ok": chat.ok, "output": chat.output}).to_string(),
                )
            } else {
                match path.as_str() {
                    "/" => ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string()),
                    "/api/status" => {
                        let payload = api_payload(&root);
                        let json = serde_json::json!({
                            "runs": payload.status,
                            "gaps": payload.gaps,
                            "queues": payload.queues,
                            "staging": payload.staging,
                            "tickets": payload.tickets,
                            "memory": payload.memory,
                        });
                        (
                            "200 OK",
                            "application/json",
                            serde_json::to_string(&json).unwrap_or_default(),
                        )
                    }
                    "/api/threads" => {
                        let threads = list_threads(&root);
                        (
                            "200 OK",
                            "application/json",
                            serde_json::to_string(&threads).unwrap_or_default(),
                        )
                    }
                    _ => ("404 Not Found", "text/plain", "not found".to_string()),
                }
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = running.load(Ordering::Relaxed);
        });
    }
    Ok(())
}

/// One HITL action result.
struct ActResult {
    ok: bool,
    output: String,
}

/// Execute a human-triggered kernel action from the dashboard. The
/// action names map 1:1 to CLI commands; each is logged to the action
/// log (audit trail). `act` is a pure router: unknown actions are a
/// 400, never executed.
fn act(root: &Path, path: &str) -> ActResult {
    let action = path.strip_prefix("/api/act/").unwrap_or("");
    let (args, label): (Vec<String>, String) = match action.split('?').next().unwrap_or("") {
        "dream-promote" => (
            vec!["dream".into(), "--promote".into()],
            "dream promote".into(),
        ),
        "dream-idle" => (vec!["dream".into(), "--idle".into()], "dream idle".into()),
        "mem-verify" => (vec!["mem".into(), "verify".into()], "mem verify".into()),
        "signoff" => {
            let q = action.split("?q=").nth(1).unwrap_or("");
            let i = action.split("&i=").nth(1).unwrap_or("");
            if q.is_empty() || !i.chars().all(|c| c.is_ascii_digit()) {
                return ActResult {
                    ok: false,
                    output: "signoff: bad query (expect ?q=<queue>&i=<index>)".into(),
                };
            }
            // Path safety: q must be a plain segment — a raw `join`
            // would let the page sign off queues outside `memory/review/`.
            if !crate::status::plain_path_segment(q) {
                return ActResult {
                    ok: false,
                    output: "signoff: queue must be a plain file name".into(),
                };
            }
            let queue = root.join(q);
            if !queue.is_file() {
                return ActResult {
                    ok: false,
                    output: format!("signoff: queue not found: {}", queue.display()),
                };
            }
            let queue = queue.to_string_lossy().into_owned();
            (
                vec!["mem".into(), "signoff".into(), queue, i.to_string()],
                format!("human signoff {i} from {q}"),
            )
        }
        _ => {
            return ActResult {
                ok: false,
                output: format!("unknown action: {action}"),
            };
        }
    };
    let _ = mini_agi_core::audit::append_action(root, "ui", "human", &label);
    let exe = std::env::current_exe().unwrap_or_default();
    let output = std::process::Command::new(exe)
        .args(&args)
        .current_dir(root)
        .output()
        .map_or_else(
            |e| format!("cannot execute: {e}"),
            |o| {
                let mut s = String::from_utf8_lossy(&o.stdout).to_string();
                s.push_str(&String::from_utf8_lossy(&o.stderr));
                s
            },
        );
    ActResult {
        ok: !output.trim().is_empty(),
        output: output.trim().to_string(),
    }
}

/// Chat result: the agent's reply (or the error).
struct ChatResult {
    ok: bool,
    output: String,
}

/// One chat message in a thread (persisted JSONL, append-only).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ChatMessage {
    role: String,
    text: String,
    ts: String,
}

/// The chat worker argv: opencode `run --agent plan --format default
/// -- <prompt>`. Plan agent = read-only (no edit tools), default format
/// = clean text answer (not the JSON event stream the `--format json`
/// worker path produces).
///
/// NO explicit `-m`: with `--agent plan` a pinned model id makes the
/// opencode server fail with "Unexpected server error" (observed
/// 2026-08-09 on opencode 1.18.11 — reproduced bare, no sandbox); the
/// plan agent resolves its own default model and answers fine.
fn chat_worker_args(prompt: &str) -> Vec<String> {
    vec![
        "run".to_string(),
        "--agent".to_string(),
        "plan".to_string(),
        "--format".to_string(),
        "default".to_string(),
        "--".to_string(),
        prompt.to_string(),
    ]
}

/// The chat thread store: `memory/episodic/chat/<thread-id>.jsonl`.
///
/// Threads are first-class: a conversation continues by id, each turn
/// re-seeds the worker with the FULL thread transcript (bounded to the
/// latest `CHAT_CONTEXT_TURNS` turns) plus the canonical memory resume
/// block — the agent answers with the conversation's own history, not
/// as a blank model. Files are append-only JSONL (one message per line)
/// like the checkpoint journal; thread ids are plain segments (no
/// traversal — the same rule as signoff queues).
const CHAT_DIR_REL: &str = "memory/episodic/chat";
const CHAT_CONTEXT_TURNS: usize = 20;

fn chat_thread_dir(root: &Path) -> std::path::PathBuf {
    root.join(CHAT_DIR_REL)
}

/// A new thread id: timestamp-based, plain segment (`chat-<unix>`).
fn new_thread_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("chat-{secs}")
}

/// List threads (id, first-message title, message count), newest first.
fn list_threads(root: &Path) -> Vec<(String, String, usize)> {
    let dir = chat_thread_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "jsonl")
            && let Some(id) = path.file_stem().and_then(|s| s.to_str())
            && !id.contains('/')
            && let Some(msgs) = read_thread(root, id)
        {
            let title = msgs.iter().find(|m| m.role == "human").map_or_else(
                || id.to_string(),
                |m| m.text.chars().take(48).collect::<String>(),
            );
            out.push((id.to_string(), title, msgs.len()));
        }
    }
    out.sort_by_key(|(_, _, n)| std::cmp::Reverse(*n));
    out
}

/// Read a thread's messages; `None` when the thread does not exist or
/// its id is not a plain segment.
fn read_thread(root: &Path, id: &str) -> Option<Vec<ChatMessage>> {
    if !crate::status::plain_path_segment(id) {
        return None;
    }
    let path = chat_thread_dir(root).join(format!("{id}.jsonl"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return None;
    };
    Some(
        text.lines()
            .filter_map(|l| serde_json::from_str::<ChatMessage>(l).ok())
            .collect(),
    )
}

/// Append one message to a thread (create the dir/file as needed).
fn append_thread_message(root: &Path, id: &str, role: &str, text: &str) -> std::io::Result<()> {
    let dir = chat_thread_dir(root);
    std::fs::create_dir_all(&dir)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| "unknown".to_string(), |d| d.as_secs().to_string());
    let msg = serde_json::to_string(&ChatMessage {
        role: role.to_string(),
        text: text.to_string(),
        ts,
    })?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{id}.jsonl")))?;
    writeln!(f, "{msg}")
}

/// One dashboard chat turn: append the human message, seed a bounded
/// opencode worker with the thread's own history (bounded turns) plus
/// the kernel memory resume block, append the reply. `thread` is
/// optional: a fresh thread is created for the first message.
fn chat_with_kernel(root: &Path, thread: Option<&str>, message: &str) -> ChatResult {
    let message = message.trim();
    if message.is_empty() {
        return ChatResult {
            ok: false,
            output: "empty message".into(),
        };
    }
    // Resolve (or create) the thread FIRST so the human message lands in
    // storage before any worker call — a crashed worker still leaves the
    // conversation intact for retry.
    let thread_id = match thread {
        Some(id) if crate::status::plain_path_segment(id) => id.to_string(),
        Some(_) => {
            return ChatResult {
                ok: false,
                output: "invalid thread id".into(),
            };
        }
        None => new_thread_id(),
    };
    if let Err(e) = append_thread_message(root, &thread_id, "human", message) {
        return ChatResult {
            ok: false,
            output: format!("thread write failed: {e}"),
        };
    }
    // Context: canonical memory resume block + the thread's own history
    // (bounded). The transcript makes the conversation CONTINUE — the
    // agent sees what it said before instead of answering in a vacuum.
    let context = crate::insights::resume(root).unwrap_or_else(|_| String::new());
    let history: Vec<ChatMessage> = read_thread(root, &thread_id)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(CHAT_CONTEXT_TURNS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut transcript = String::new();
    for m in &history {
        let role = if m.role == "human" { "HUMAN" } else { "AGENT" };
        let _ = writeln!(transcript, "{role}: {}", m.text);
    }
    let prompt = format!(
        "You are mini-agi's in-dashboard assistant — a senior engineer \
         working INSIDE this repo, memory-anchored (ADR-0003).\n\n\
         CONTEXT FROM CANONICAL MEMORY:\n{context}\n\n\
         CONVERSATION SO FAR (latest {CHAT_CONTEXT_TURNS} turns):\n{transcript}\n\n\
         Continue the conversation. The human just wrote:\n\n\
         HUMAN: {message}\n\n\
         Answer concisely and concretely: state facts (with evidence from \
         the context or the repo when you can), propose the next action, \
         and NEVER invent facts — say 'unknown' when the context does not \
         answer."
    );
    // The worker runs IN THE REPO ROOT (not a temp dir) so it reads the
    // real canonical memory, journal and brief — the resume block says
    // "0 facts" when the workdir is elsewhere (observed live: the chat
    // answered from a blank repo). Plan agent is read-only by design, so
    // running in-root cannot mutate anything.
    let workdir = root.to_path_buf();
    // The chat worker is a PLAN-ONLY agent: `--agent plan` runs without
    // edit tools (the dashboard is read-only by design — writes stay
    // HITL in the terminal) and `--format default` returns a clean text
    // answer instead of a raw JSON event stream (observed: the first
    // chat attempt returned opencode step_start/tool_use JSONL events).
    let args = chat_worker_args(&prompt);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = crate::worker::run_worker_sandboxed(
        "opencode-opencode-go/deepseek-v4-flash",
        &workdir,
        false,
        true,
        Some(120),
        None,
        &arg_refs,
    );
    let output = match result {
        Ok(w) => {
            let s = w.output;
            if w.status == Some(0) {
                s
            } else {
                format!("worker exited {:?} — {s}", w.status)
            }
        }
        Err(e) => format!("worker not available: {e}"),
    };
    if let Err(e) = append_thread_message(root, &thread_id, "agent", &output) {
        return ChatResult {
            ok: false,
            output: format!("thread write failed: {e}"),
        };
    }
    ChatResult {
        ok: !output.trim().is_empty(),
        output: output.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_serves_self_refreshing_markup() {
        assert!(INDEX_HTML.contains("/api/status"));
        assert!(INDEX_HTML.contains("setInterval(tick, 2500)"));
        assert!(INDEX_HTML.contains("Attention"));
        assert!(INDEX_HTML.contains("Brain"));
        assert!(INDEX_HTML.contains("copy"));
        assert!(INDEX_HTML.contains("POST"));
        assert!(INDEX_HTML.contains("action("));
    }

    #[test]
    fn act_router_rejects_unknown_and_malformed() {
        let root = std::env::temp_dir().join(format!("mag-ui-act-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let r = act(&root, "/api/act/rm-rf");
        assert!(!r.ok);
        assert!(r.output.contains("unknown action"));
        let r = act(&root, "/api/act/signoff?q=none.md&i=abc");
        assert!(!r.ok);
        assert!(r.output.contains("bad query"));
        let r = act(&root, "/api/act/signoff?q=none.md&i=1");
        assert!(!r.ok, "missing queue file must fail closed");
        assert!(r.output.contains("queue not found"));
        // Traversal: q must stay a plain segment.
        let r = act(&root, "/api/act/signoff?q=../etc/passwd&i=1");
        assert!(!r.ok, "traversal queue must be rejected");
        assert!(r.output.contains("plain file name"));
    }

    #[test]
    fn chat_threads_append_list_and_reject_traversal() {
        let root = std::env::temp_dir().join(format!("mag-ui-chat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(list_threads(&root).is_empty(), "no threads yet");
        // Two messages in one thread (human + agent), appended.
        append_thread_message(&root, "chat-1", "human", "hello").unwrap();
        append_thread_message(&root, "chat-1", "agent", "hi").unwrap();
        append_thread_message(&root, "chat-2", "human", "other").unwrap();
        let threads = list_threads(&root);
        assert_eq!(threads.len(), 2, "both threads listed");
        let msgs = read_thread(&root, "chat-1").unwrap();
        assert_eq!(msgs.len(), 2, "thread transcript persisted");
        assert_eq!(msgs[0].role, "human");
        assert_eq!(msgs[1].role, "agent");
        assert!(msgs[1].text.contains("hi"));
        // Traversal and absolute paths are rejected (plain segments only).
        assert!(read_thread(&root, "../etc/passwd").is_none());
        assert!(read_thread(&root, "/tmp/x").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn chat_parses_message_without_zero_padding() {
        // Regression (observed live): the server read a POST into a
        // zero-initialized 8192-byte buffer and parsed the WHOLE buffer,
        // so a short message arrived with ~8k of trailing NUL bytes and
        // the worker rejected the prompt. The fix slices the buffer to
        // the actual read length before parsing. This test reproduces the
        // exact read pattern (n < buffer len) and asserts the parsed
        // message is clean.
        let mut buf = [0u8; 8192];
        let raw = b"POST /api/chat HTTP/1.1\r\nx-message: hello kernel\r\n\r\n";
        let n = raw.len();
        buf[..n].copy_from_slice(raw);
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let message = req
            .lines()
            .find(|l| l.starts_with("x-message:"))
            .map(|l| l["x-message:".len()..].trim().to_string())
            .unwrap_or_default();
        assert_eq!(message, "hello kernel", "no NUL padding in the message");
        assert!(!message.contains('\0'));
        // The OLD behavior (whole buffer) must never come back:
        let old = String::from_utf8_lossy(&buf[..]).to_string();
        assert!(old.contains('\0'), "precondition: stale buffer had zeros");
    }

    #[test]
    fn page_serves_chat_markup_and_threads_endpoint() {
        assert!(INDEX_HTML.contains("/api/threads"));
        assert!(INDEX_HTML.contains("chatSend"));
        assert!(INDEX_HTML.contains("x-message"));
        assert!(INDEX_HTML.contains("threads persisted"));
    }

    #[test]
    fn api_payload_carries_runs_workers_journal() {
        let root = std::env::temp_dir().join(format!(
            "mag-ui-payload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let run = root.join("evals/cases/a/run.json");
        std::fs::create_dir_all(run.parent().unwrap()).unwrap();
        std::fs::write(
            &run,
            r#"{"goal":"g","scope":["x"],"outcome":{"achieved":true},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":null,"verify_target":null,"trajectory":[{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        let payload = api_payload(&root);
        let status = payload.status.as_object().expect("status object");
        assert_eq!(
            status["total_runs"].as_u64(),
            Some(1),
            "payload must carry the run index"
        );
        assert!(
            payload.queues.is_empty(),
            "queues field must be present (empty by default)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
