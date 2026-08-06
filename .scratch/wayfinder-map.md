# Wayfinder Map — Full AGI on the mini-agi kernel

Date: 2026-08-06. Method: wayfinder (map + decision tickets).
Status: mapping phase — research tracks 1-3 done, decisions OPEN.

## Destination (charter, user direction, 2026-08-05)

A FULL AGI with a brain/memory layer so good it all ties together: episodic
runs consolidate into a semantic memory (dream-loop: distiller/auditor/linker),
a swarm of cheap workers runs 24/7 (implement, verify, auto-research) under
kernel supervision, and a files-first web UI mirrors the state for the human.
Frontend is USER-ONLY work (HITL, per-domain gate F-011). Foundation:
the mini-agi kernel (loop, enforcement-bound memory, skills registry, MCP,
parallel dispatch) + opencode (deepseek v4 flash = cheap worker).

## Evidence base (all tracks done)

- TRACK 1 — brain/memory: docs/MEMORY-RESEARCH.md (CoALA, MemGPT/Letta
  dreaming, Generative Agents, Voyager, HippoRAG, Mem0, E-mem, D-MEM,
  MemoryBank, sleep-time compute, Karpathy append-and-review, OpenAI agentic).
- TRACK 2 — UI/UX: docs/AFK-SUPERVISOR.md v2 note (7 panels, files-first
  read mirror, needs-you pings via --on-done → ntfy, Tauri later).
- TRACK 3 — orchestration & economics: .scratch/wayfinder/track-3.md
  (verified rate cards Aug 2026, cost model, model assignment, cadence,
  A2A verdict, recovery, surface).
- External primaries: primeintellect.ai/blog/prime-agent (RLM, Continual
  Harness, A2A daemon, recoverable workers) + mastra.ai/docs/memory/
  observational-memory (Observer+Reflector, dense observation log).

## Decisions to make (tickets in .scratch/wayfinder/)

| # | Decision | Status | Effort | Feeds |
|---|---|---|---|---|
| D1 | Worker economics: LAYERED — codex executes, opencode/flash plans+reviews (role model: user 2026-08-06) | **IMPLEMENTED 2026-08-06** (adapter + telemetry, commit 8055af7) | S | everything (24/7 affordance) |
| D2 | Dream-loop cadence + model assignment | OPEN (recommend event-triggered + gated promotion) | M | brain layer |
| D3 | Memory quality: merge/supersede + retrieval budget + directed consolidation | **IMPLEMENTED 2026-08-06** (mem supersede/preserve/verify + query --budget + preserved routing; 35eb321) | M-L | brain layer |
| D4 | Supervision surface: read mirror + worker status | OPEN (user builds UI; kernel exposes status JSON) | S (kernel side) | surface |
| D5 | A2A / daemon shape | OPEN (recommend DEFER A2A; bg.rs+serve is enough) | — | orchestration |
| D6 | Crash recovery: respawn + run-state index | **IMPLEMENTED 2026-08-06** (respawn 0295cb7, status 551c6df) | S | 24/7 reliability |
| D7 | Agent-managed harness CRUD vs append-only | OPEN (recommend keep append-only + gate-bound) | — | guarantees |

## Roadmap (ordering by dependency + economics)

Phase 0 — UNLOCK (D1, D6): layered worker adapter — codex keeps executing,
opencode/flash runs the planner/reviewer/distiller stages (worker.rs seam),
cost/run telemetry, respawn-on-crash + rebuildable run-state index. Makes
the 24/7 brain affordable and crash-safe. No UI, no memory changes.

Phase 1 — BRAIN (D2, D3): dream-loop (cheap distiller event-triggered,
strong auditor + human signoff on enforced facts per ADR-0010, nightly full
audit), merge/supersede + dedup gate, selective token-budgeted retrieval,
directed consolidation. The memory layer becomes self-maintaining.

Phase 2 — SURFACE (D4, D7): kernel exposes machine-readable worker/run
status; user builds the 7-panel files-first read mirror (HITL). D5/D7 stay
decision-docs unless evidence changes.

## First build

D1 — layered worker adapter (highest value per track-3 §7: it is the
economic unlock; smallest build; measurable cost/run within a session).
Executor stays codex; flash enters via the reviewer/planner/distiller seam.

## Gate discipline while mapping

- Tickets stay OPEN until decided; a decided ticket records the decision +
  evidence + rejected alternatives (devils-advocate rule).
- Every build slice: checkpoint begin → edit → verify ALL GREEN → close
  checkpoint → push. Reviews: devils-advocate, memory-anchored (ADR-0003).
