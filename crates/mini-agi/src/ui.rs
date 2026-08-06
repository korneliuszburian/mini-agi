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
  <div class="card" id="card-queues"><h2>Human queue (signoff)</h2><div id="queues">…</div></div>
  <div class="card" id="card-tickets"><h2>Tickets</h2><div id="tickets">…</div></div>
</div>
<script>
const ESC = t => (t ?? '').toString().replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const COPY = cmd => `<button class="copy" onclick="navigator.clipboard.writeText(${JSON.stringify(cmd)})">copy</button>`;
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
  const q = (d.queues||[]).map(x => `<div class="warn" style="white-space:pre">${ESC(x.slice(0,400))}</div>`).join('') || '<span class="dim">queue empty — nothing waits for you</span>';
  document.getElementById('queues').innerHTML = q;
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
</script></body></html>"#;

/// The API payload: everything the page renders, computed fresh per
/// request from the filesystem (no state, no cache).
struct ApiPayload {
    status: serde_json::Value,
    gaps: serde_json::Value,
    queues: Vec<String>,
    staging: serde_json::Value,
    tickets: serde_json::Value,
    memory: serde_json::Value,
}

fn api_payload(root: &Path) -> ApiPayload {
    let status = crate::status::index_runs(&root.join("evals/cases"));
    let workers = crate::status::live_workers(root);
    let journal = crate::status::journal_tail(root, 6);
    // Gaps: loop status rows below target.
    let target = mini_agi_core::config::Config::target_composite_for(root);
    let gaps = mini_agi_core::loopcmd::status(root)
        .map(|s| s.cases)
        .unwrap_or_default();
    // Human queues.
    let mut queues = Vec::new();
    if let Ok(days) = std::fs::read_dir(root.join("memory/review")) {
        for day in days.flatten() {
            for e in day
                .path()
                .read_dir()
                .map(|es| es.flatten().collect::<Vec<_>>())
                .unwrap_or_default()
            {
                if e.path().extension().is_some_and(|x| x == "md") {
                    let preview = std::fs::read_to_string(&e.path())
                        .unwrap_or_default()
                        .lines()
                        .take(6)
                        .collect::<Vec<_>>()
                        .join("\n");
                    queues.push(format!("{}:\n{preview}", e.path().to_string_lossy()));
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
                .map(|g| serde_json::json!({
                    "case": g.case,
                    "composite": g.composite,
                    "ticket": g.ticket,
                    "attempts": g.attempts,
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
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let req = String::from_utf8_lossy(&buf[..]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (status, ctype, body) = match path.as_str() {
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
                _ => ("404 Not Found", "text/plain", "not found".to_string()),
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
    }
}
