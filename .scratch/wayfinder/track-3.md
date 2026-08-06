# TRACK 3 — Orchestration & Economics (wayfinder research)

Research-only output. Date: 2026-08-06. Sources: primeintellect.ai/blog/prime-agent (Aug 05 2026), mastra.ai/docs/memory/observational-memory (@mastra/memory@1.1.0+), opencode.ai/docs (agents, cli — current), api-docs.deepseek.com/quick_start/pricing (current), platform.openai.com/docs/pricing (current), anthropic.com/pricing (current), plus this repo: docs/MEMORY-RESEARCH.md, memory/episodic/2026-08-05-buffer.md, crates/mini-agi/src/{planner,bg,worker}.rs, docs/AFK-SUPERVISOR.md, docs/CODEX-INTEGRATION.md, docs/PLAN.md, memory/canonical (observed run costs).

## Verified USD rates (per 1M tokens, Aug 2026)

| Model | in (miss) | in (cached) | out | notes |
|---|---|---|---|---|
| deepseek-v4-flash | $0.14 | $0.0028 | $0.28 | 1M ctx; thinking+non-thinking. **Official notice: "significant price increase expected"** |
| deepseek-v4-pro | $0.435 | $0.0036 | $0.87 | |
| gpt-5.6-terra (current codex worker) | $2.00 | $0.20 | $12.00 | long ctx $4/$0.40/$18 |
| gpt-5.6-luna | $0.20 | $0.02 | $1.20 | OpenAI cheap tier |
| gpt-5.6-sol | $5.00 | $0.50 | $30.00 | |
| gpt-5.4-mini | $0.75 | $0.075 | $4.50 | |
| claude haiku 4.5 | $1.00 | $0.10 (read) | $5.00 | |
| claude sonnet 5 | $2.00 | $0.20 (read) | $10.00 | intro to 2026-08-31; $3/$15 after |
| claude opus 5 | $5.00 | $0.50 (read) | $25.00 | |

Observed own-run costs (canonical memory 2026-08-02/03, codex + gpt-5.6-terra): first-pass real slices 106k–1.84M tokens, $0.27–$1.31/run (mid ~130–290k tokens, $0.28–$0.77); resumed/cached runs 8.6k–18.4k tokens, $0.04–$0.09/run (cache-first, T007: 1.7M/1.8M tokens cached in-run).

---

## 1. WORKER ECONOMICS — concrete cost model

**RECOMMENDATION.** Default envelope = **hybrid (c)**: opencode worker on deepseek-v4-flash for all loop work (implement/verify/distill/link/summarize) + a strong model (gpt-5.6-terra or claude sonnet 5) for ~10% of runs that need judgment (auditor, conflict resolution, promotions, planner pass). Target: **~$1/day at 24 runs/day (~$30/mo)** — roughly 15–30x cheaper than the all-codex envelope while keeping a strong-model judgment tier.

**EVIDENCE (per-1,000-runs; representative run = 1 fresh attempt + 2 resumes, 130k in / 15k out tokens — mid-range of our observed runs):**
- (a) worker=codex (terra, API): fresh $0.26 in + $0.18 out ≈ $0.44; resumes mostly cached ≈ $0.06 each → **~$0.60/run → $600/1,000 runs** (observed range $350–1,450).
- (b) worker=opencode+v4-flash: same token volumes: fresh $0.0182 in + $0.0042 out ≈ $0.022; resumed ≈ $0.001 (cache hit) → **~$0.025/run → $25/1,000 runs**. Even 5x token waste on the weaker model = $0.10–0.15/run → $100–150/1k. The 20–30x gap is rate-card arithmetic, not estimate.
- (c) hybrid: flash loops $0.025/run + strong judgment on 10% of runs (50k in/5k out terra ≈ $0.16, sonnet 5 ≈ $0.15) → **~$0.04/run → $40/1,000 runs**.

24/7 envelope at 1 run/hour (24 runs/day, 720/mo): (a) ≈ $15/day ≈ $430/mo (range $8–35/day); (b) ≈ $0.60/day ≈ $18/mo; (c) ≈ $1/day ≈ $30/mo.

**ALTERNATIVES.**
- **Flat subscription instead of API**: ChatGPT Plus $20/mo includes Terra (25–200 msgs/5h) + Luna (250–2,000 msgs/5h); Pro $100/mo = 5x. At 24–48 runs/day a $20–100/mo subscription can beat API rates, at the cost of rate-limit scheduling and no per-run telemetry. Our kernel records cost proxies per run — API keeps that signal honest; subscription hides it. Prefer API for the loop, subscription for interactive use.
- **luna as cheap tier** ($0.20/$1.20) if single-vendor matters — still ~10x terra's cost but ~7x flash's; deepseek wins on economics AND its 1M context is a long-horizon advantage.
- **All-strong (status quo)** — $400+/mo at 24 runs/day; only viable as batch (batch halves terra to $1/$6) or subscription.

**CAVEAT (flagged, not priced):** DeepSeek's pricing page officially states "overall pricing … significant increase expected". The 20–30x gap has headroom to erode; re-check the rate card at build time. Model selection is already config (worker.rs resolves worker name + model per call).

---

## 2. MODEL ASSIGNMENT — strong vs cheap per brain-layer stage

**RECOMMENDATION.**

| Stage | Tier | Evidence |
|---|---|---|
| Observer/distiller (episodic→observation) | CHEAP (v4-flash / gemini-2.5-flash class) | Mastra defaults Observer to gemini-2.5-flash; tested deepseek-reasoner/v4-flash/v4-pro; "fast enough to run in background"; token-tiered selection keeps cheap models on small inputs |
| Linker (fact→fact edges) | CHEAP | D-MEM/Mem0 extract+link on gpt-4o-mini-class models (TRACK 1) |
| Summarizer / compaction / thread title | CHEAP | Mastra extraction pipeline; prime-agent's compaction GC = "a spawned agent as garbage collector"; our MEMORY-RESEARCH compaction is threshold work |
| Failure-register extraction | CHEAP | Reflexion/MAST is structured extraction |
| Auditor (validates promoted facts) | STRONG (terra/sonnet-5; opus only for edge cases) | Mem0 runs even its AUDITOR on gpt-4o-mini — evidence auditing CAN be cheap when schema/verification-shaped; our auditor judges truth against provenance — keep strong; ADR-0003 memory-anchored review is a strong-model slot |
| Conflict resolution (supersede/merge) | STRONG | The one destructive fact write; our append-only canonical makes it the authority boundary — never cheap |
| Promotion to canonical | STRONG + HITL signoff | ADR-0010: signoff is human by design; the model pass before it should be strong |
| Planner pass (batch decomposition) | STRONG | Currently codex (read-only); prime-agent /refine plans on a background LLM call, applies cheaply — plan strong, apply cheap |
| Judgment/reflection (Karpathy promote-review) | STRONG but RARE | Periodic, small volume, cost amortizes |

**EVIDENCE.** Mastra: Observer+Reflector default gemini-2.5-flash; Reflector optionally stronger (v4-pro "condenses more readily"); `ModelByInputTokens` tiers by input size. TRACK 1 buffer: "Mem0 runs all ops on gpt-4o-mini" (cheap extraction AND audit); E-mem/D-MEM use cheap async extractors; Karpathy review is periodic. prime-agent: /refine's planning call runs in the background without blocking; its ARC-AGI-3 95.5% Best@1 and EmulatorBench results show strong models for the hard loop, not for bookkeeping.

**ALTERNATIVES.**
- All-cheap (Mem0-style): works for facts, risks silent drift at conflict/promotion — our canonical memory is the trust root (ADR-0010); don't cheap the write path.
- Token-tiered routing (Mastra's ModelByInputTokens) instead of stage-fixed tiers: cheap on small inputs, strong only when the observation log is big. Adopt for the Reflector; stage-fixed for the rest.

---

## 3. ORCHESTRATION SHAPE — daemon vs bg.rs vs A2A vs nuclear family

**RECOMMENDATION.**
(a) **Yes to a daemon — but as a kernel command (`loop daemon`), not a new abstraction.** It reorganizes what bg.rs already proves (identity-aware liveness via pid+starttime, run handles, one-run-per-workdir locks, evidence-preserving failure). What prime-agent's daemon adds that we lack: (1) a live session registry across workers, (2) attach/detach interactivity, (3) respawn-on-crash, (4) worker-to-worker addressing. For a single machine that is one supervisor process owning `loop run --detach` children over a local socket — not a rewrite. At the worker level, **opencode `serve` + `run --attach`** is exactly prime-agent's attach/detach pattern, and `opencode run --format json` gives scriptable per-event output for the supervisor.

(b) **A2A/worker-to-worker messaging: defer. Files + kernel state are the message bus.** Our planner manifest (strict JSON, disjoint scopes, atomic merge) + worktrees + run.json + checkpoint journal carry every message a batch needs, with better isolation than any wire protocol. prime-agent limits messaging to the nuclear family *precisely* because unrestricted cross-session communication is noise; Mastra's Observer needs no A2A at all (observations replace messages). The ONE thing files cannot do is **mid-flight steering** (prime-agent's `agent_message.send(..., mode: "follow_up")` into a retained child). Add a single MCP tool for that only when a real case appears.

(c) **Nuclear-family isolation: adopt as POLICY, skip as machinery.** worker_name + MINIAGI_SESSION_TAG + batch ticket identity already attribute parent/child/sibling; scoped worktrees enforce isolation at the filesystem (stronger than message scoping). The only rule worth encoding: any future message tool accepts `receiver_role ∈ {parent, sibling, child}` + name.

**EVIDENCE.** prime-agent: daemon "owns all live agent sessions over a local socket; attach and detach without affecting the agent loop"; sub-agents are full agent instances with persistent session dirs; A2A "limited to its nuclear family … to prevent undesirable communication across independent sessions"; recoverable workers resume from JSONL + snapshot. Ours: bg.rs (run.pid/run.start identity, launch.json, launch.lock create_new, handle authority F3), planner.rs (manifest admission, worktrees, containment, atomic scratch merge, protected final gate). opencode docs: `serve`/`run --attach` (warm server, avoids MCP cold boot), `--agent`, `--model`, `--format json`, `session list/continue`, `stats` (per-model tokens/cost).

**ALTERNATIVES.**
- Pure bg.rs + MCP today, daemon later: viable — the supervisor screen and dream-loop work without a daemon. The daemon's real payoff is crash-respawn + one place to answer "what is the swarm doing". Build it when the run board needs a live backend.
- Full A2A now (message tool + inbox per worker): overkill for ≤5 local workers; adds a permission surface for zero current use.

---

## 4. SCHEDULING — when does the dream-loop run

**RECOMMENDATION. Two-tier cadence:**
1. **Observation/distillation = continuous + event-triggered** (post-run, threshold, and idle-activated): after every completed loop run and on context pressure. Cheap model → marginal cost ~$0, so run it like Mastra's Observer: buffer during activity, activate instantly at threshold, blockAfter ceiling, activateAfterIdle tuned to the provider's prompt-cache TTL (DeepSeek cache TTL ≈ 1h per Mastra's auto table — activate before the cache expires).
2. **Promotion/audit/signoff = nightly gated window**: one strong-model pass over the day's observation queue, then the human signoff queue (ADR-0010). MemoryBank sleeps at night; Karpathy's review is periodic; nightly batches the human visit to once/day and bounds strong-model spend.

**EVIDENCE.** Mastra OM: continuous async buffering (buffer every ~6k tokens at default, activate at 30k, Reflector at 40k, blockAfter 1.2 safety ceiling, idle activation "so the next uncached prompt uses compressed observations"); context oscillates 6k↔30k — bounded *without* a nightly batch, and prompt caching is explicitly why. prime-agent: continuous + cron-style heartbeats + on-failure /refine triggers ("not only on a fixed schedule"). Letta: event-triggered (N messages / compaction) — TRACK 1. MemoryBank: nightly sleep — TRACK 1. Our own rules: event boundaries are where MemGPT interrupts fire (MEMORY-RESEARCH).

**ALTERNATIVES.**
- Nightly-only for everything: loses Mastra's cache-friendly continuous compression; episodic buffers grow unbounded between nights and each morning's pass gets more expensive (the exact failure bufferTokens prevents).
- Fully continuous including strong-model audit: unbounded daily cost, no evidence of quality gain; audit volumes are tiny and batch perfectly.

---

## 5. RECOVERY — what prime-agent has that we lack

**RECOMMENDATION.** Our crash story is *better* at the storage layer and *missing* at the supervision layer. Missing pieces, in build order:
1. **Respawn-with-resume**: bg.rs does not restart a dead run — a crashed supervisor stays dead until polled. A `loop daemon` (Q3a) needs a policy: restart ≤N times on crash via resume (codex: `codex exec resume <uuid>` — we already capture the session id via SESS-OWN markers; opencode: `opencode run --continue --session <id>` — capture the id from adapter output), escalate to human after N.
2. **Machine-state ledger**: prime-agent's "kernel state snapshot" maps to OUR plain files (journal, memory, run.json, launch.json) — inherently crash-safe. The gap is a single rebuildable index ("where do all live runs stand") reconstructed at boot by scanning `.supervisor` handles + the checkpoint journal. Small, mechanical, files-only.
3. **opencode session capture**: the codex adapter reads `~/.codex/sessions`; an opencode adapter needs the equivalent (session id from `run --format json` events) so resume works across worker types.
4. **Idle-unload: skip.** prime-agent unloads sessions after 30 min inactivity to save RAM; our workers are processes on disk — reap done runs instead of hibernating (keep `opencode serve` warm as the only resident).

**EVIDENCE.** prime-agent: "recoverable worker process; if a worker crashes, the daemon recovers it from the session JSONL and kernel state snapshot"; append-only JSONL + leaf-pointer branching; inactive sessions reloaded on demand. Ours: bg.rs handle protocol (pid+start identity, launch.json, lock), checkpoint journal (BEGIN/VERIFY, rollback to last BEGIN, repair only via checkpoint.sh), planner evidence preservation (worktrees/branches/reports survive a failed batch), run.json + `run verify` (Phase 9 verified-before-trusted). Codex resume is proven in our runs (AFK v2, SESS-OWN marker; resumes at $0.04–0.09).

---

## 6. SURFACE — Agents View vs the TRACK-2 7-panel design

**RECOMMENDATION.** The Agents View is NOT a missing panel — it's a missing **state vocabulary**: prime-agent's Running / Idle / Inactive states per worker identity. Adopt that exact 3-state machine as the run board's status tier + a worker column (worker_name), and keep everything else from TRACK 2 (needs-you pings via `--on-done` → ntfy; completed = badge; running/dreaming = silent; files-first read mirror). Do NOT build chat-into-any-session (prime-agent's space-to-steer) — our HITL is terminal/MCP by design (per-domain human gate; frontend = mandatory HITL).

What a ≤5-worker supervisor screen must show to not be noise:
- 3 columns: worker | run | state+age (Running / Idle / Inactive), each row one line.
- Exactly 3 colors: running (neutral), needs-you/blocked (warn — also pings ntfy), done-today (badge). Everything else is a click, not a color.
- No transcript by default: one click to REPORT.md / progress.md / run.json.
- Worker identity column is the whole "Agents View" delta — a run's owner (worker_name), not just its lifecycle.

**EVIDENCE.** prime-agent: Agents View lists running, idle (daemon-resident), inactive (not in memory) sessions; "any agent is discoverable … recursively"; sub-agents share the same state machine and are unloaded after 30 min. TRACK 2 (buffer): attention model = needs-you pings, completed badge, silent running/dreaming; Claude Code agent view = minimal honest status surface; board-alone failed (Vibe Kanban). opencode: `session list` gives the same running/idle/inactive data source at the worker level.

**ALTERNATIVES.**
- Full prime-agent-style steering UI (enter any session, space to queue prompts): rejected — HITL stays in terminal/MCP; the UI is a read mirror (TRACK 2 decision).
- Polling all `run_status` handles every N seconds from the web panel vs kernel-owned daemon state: daemon state is the Q3a build; until then the panel polls bg.rs handles directly (works today, no new kernel surface).

---

## 7. THE SINGLE HIGHEST-VALUE BUILD

**RECOMMENDATION.** **The opencode + deepseek-v4-flash worker adapter** (worker_name swap: `opencode run --agent build --model deepseek/deepseek-v4-flash --format json --auto` + session-id capture for resume), because it is the economic unlock that makes every other item on the wishlist affordable. The 24/7 brain layer, the auto-researcher on cheap workers, the multi-worker swarm — all of them are cost-prohibitive on today's $0.27–1.31/run codex envelope and trivially cheap on flash ($0.02–0.05/run, 20–30x rate-card gap, verified). It is also the smallest build of the three candidates: worker.rs:900 already parameterizes the worker command; the DECIDED direction (buffer) names opencode as the worker harness; opencode's CLI already exposes everything the kernel needs (`run --format json`, `--continue`, `serve`/`--attach`, `stats`). Second build: the dream-loop distiller on the cheap worker (event-triggered observations per Q4 — near-zero marginal cost). Third: `loop daemon` (Q3a) with respawn (Q5). The UI panel can then consume bg.rs/daemon state without any new invention. Opinion: swap the worker first, because everything else in this track is either enabled or cheapened by it — and it can be shipped and measured (cost/run delta) within one session, verified against the kernel's own run cost proxies.

**EVIDENCE.** Rate cards (verified, Q1 table); observed run costs (canonical facts 2026-08-02/03); worker.rs:900 seam; opencode CLI capabilities (cli docs: `run --format json`, `--agent`, `--model`, `--continue/--session`, `serve`+`attach`); Mastra's proven cheap-model Observer pattern (default gemini-2.5-flash) shows the memory layer needs nothing stronger than flash; prime-agent shows the daemon/supervisor layer is real but strictly downstream of worker economics. One caution from prime-agent: its Factorio run "reward hacked" through its own refine loop — the autonomous loop must keep our deterministic gates + HITL signoff (which it already has) in front of anything learned automatically.

---

## DECISIONS THIS FEEDS

Candidate wayfinder decision tickets (to be written with the map synthesis):
- **D1 Worker economics** — opencode+v4-flash as default worker; hybrid strong-model tier (~10%); API not subscription for the loop; re-check deepseek rate card before build (price-increase notice).
- **D2 Model assignment** — cheap: observer/distiller, linker, summarizer, failure-register; strong: auditor, conflict resolution, promotion, planner pass; token-tiered Reflector.
- **D3 Orchestration shape** — `loop daemon` (kernel command over bg.rs, not a new abstraction); A2A deferred; nuclear-family as policy only.
- **D4 Dream-loop cadence** — continuous event-triggered observation (Mastra knob set) + nightly gated promotion/audit/signoff window.
- **D5 Recovery** — respawn-with-resume policy in the daemon; machine-state ledger (rebuildable index); opencode session capture in the adapter; no idle-unload.
- **D6 Supervisor surface** — Running/Idle/Inactive state vocabulary + worker column on the TRACK-2 run board; no steering UI; needs-you pings unchanged.
- **D7 Build order** — opencode worker swap first (measure cost/run), dream-loop distiller second, `loop daemon` third.
