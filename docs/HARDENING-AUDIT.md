# mini-agi — Deep Research + Hardening Audit (2026-08-04)

Deep layer-by-layer audit of the repo + benchmark against real agent
frameworks, with a concrete hardening plan. Grounded in the current
worktree (`1d3e4c8`). Anything not directly confirmed in code or in a
fetched source is marked **unconfirmed** — never assumed.

---

## A. Executive summary

**What this repo really is.** This is **not** a classical "agent loop"
(`input → LLM → tool → repeat`) like muellerberndt/mini-agi, Auto-GPT or
BabyAGI. It is an **agent kernel** — a single Rust binary that other
agents (Codex, Claude, Cursor, opencode) plug into via CLI + stdio MCP,
providing: enforcement-bound memory (canonical-first, fact ids, derived
views), a 4-dimensional eval engine with a deterministic verifier and a
regression gate, a verifiable skills registry (15 skills), a checkpoint
journal with rollback, a ticket/lease lifecycle, and a counterfactual
harness gate. The "planning" and "execution" are deliberately performed
by the **host agent**; the kernel supplies the verified brain. Within
that framing it is unusually disciplined: std-only, zero-unsafe, pinned
toolchain, 190 tests, gate ALL GREEN, CI green.

**The single biggest strength — and why it matters.** The one thing none
of the benchmarked frameworks have is what this repo calls *verified
before trusted* (ADR-0011): `mini-agi run verify <run.json>` executes the
declared `verify_command` in `verify_target` and the kernel reports
`verified / disagrees / unverified`; `loop verify` closes a gap only when
composite >= 0.5 **and** the verifier passes. muellerberndt, Auto-GPT and
all three BabyAGI generations terminate on the model's *self-declared*
"done" — with no objective check. This repo's termination is the only one
of the five that is enforced rather than narrated. That is the pattern
worth keeping and exporting, not the shell scripts.

**Top risks (ordered).**

1. **No kernel-enforced sandbox for the external agent's actions.**
   `mini-agi codex` runs Codex (arbitrary commands, `pip install`,
   `exec`-equivalent behavior) in a workdir behind only a procedural
   trust boundary (AGENTS.md). ADR-0009 exists but the sandbox check is a
   **CI-only attestation** — local `verify.sh` prints `[skip] sandbox:
   CI-only isolation attestation (ADR-0009)`. muellerberndt ships the
   same class of arbitrary-exec with zero sandbox; Auto-GPT has a real
   `CommandPermissionManager`; we have neither an allowlist nor a
   sandboxed exec path for the codex/hitl workers.
2. **No enforced budget / step limit on the worker.** `TARGET_COMPOSITE`
   (loopcmd.rs:20) is hardcoded to `0.5`; run.json records tokens/cost
   but nothing **enforces** a cost/step/time cap on a run. The
   "max 3 retries, max 40 steps" rule lives in AGENTS.md (procedural),
   not in the kernel. If the host agent ignores it, nothing stops an
   unbounded loop. Auto-GPT's `cycle_budget` tri-state and babyagi3's
   `budget_usd`/`token_limit`/cancellation are the patterns we lack.
3. **No in-kernel planning or critic layer.** The plan lives in the
   ticket spec (written by `loop dispatch`); the critic is absent — there
   is no step-level self-criticism pass in the kernel loop, no repetition
   watchdog (Auto-GPT), no VERIFY-against-objective step (babyagi3).
   Reflection exists only as *post-hoc* MAST failure-register entries
   injected into reruns. Step-level *process supervision* (eval.rs
   `StepVerdict`, `suspicious` flags) is the seed of a critic but it only
   scores, it never redirects.

**The 3 most important P0 fixes** (each concrete, in Section F):

1. **P0-1 — Kernel-enforced worker guardrails.** Add an executed-cap
   layer (`budget.rs`/`worker.rs`) that the kernel checks around every
   codex/hitl worker: max steps, max cost, max wall time, working-tree
   diff allowed — deny + abort + register when exceeded. This makes the
   AGENTS.md retry/step rules enforced instead of advisory.
2. **P0-2 — Cycle budget + stop conditions as data.** Port Auto-GPT's
   tri-state `cycle_budget` (`None`=unlimited / `1`=step-approval /
   `0`=stop) into the dispatch spec and enforce it in `loop verify`; make
   `TARGET_COMPOSITE`, regression drop tolerance, and health thresholds
   configurable via a first real config file (`.miniagi.toml` + env
   overlay) instead of hardcoded constants.
3. **P0-3 — Kill the trust-only edge on the codex path.** `loop verify`
   already requires a verifier; extend the same rule to **dispatch**: a
   gap cannot be dispatched to a worker whose ticket spec has no
   `verify_command`, and `mini-agi codex` must refuse to start a run
   whose spec declares no verifier. Combined with P0-1 this closes the
   only remaining "model says done" path.

---

## B. Architecture / loop review

### B.1 Inventory (files, with roles)

```
crates/mini-agi/src/        binary crate (CLI + MCP)
  main.rs                   1847 L — all CLI command dispatch, codex worker, harness CLI
  mcp.rs                     905 L — stdio MCP server (tools over JSON-RPC 2.0)
  init.rs                    repo scaffold (layout, gate, AGENTS.md, MCP config)
crates/mini-agi-core/src/   kernel crate (std-only: sha2, serde, serde_json, thiserror)
  memory.rs                  681 L — consolidate/signoff/derive; canonical facts + fact ids
  store.rs                    EntryFile, fact-id extraction
  eval.rs                   1028 L — 4D scoring, regression gate, mismatches, scope, tools
  verifier.rs                508 L — run verify (deterministic verifier), calibration, judge-drift
  loopcmd.rs                1043 L — loop status/dispatch/verify (ADR-0005)
  skills.rs                  529 L — skills registry (frontmatter, verify hooks)
  journal.rs                 435 L — checkpoint journal audit (T008 semantics)
  ticket.rs                  886 L — ticket lifecycle, leases, work graph (ADR-0007/0008)
  audit.rs                   418 L — repo invariants gate (6+ checks)
  health.rs                  391 L — machine+repo snapshot (load/mem/zoo/journal/claims)
  harness.rs                 398 L — counterfactual harness gate (Phase 9/10)
  capture.rs                 312 L — codex transcript capture (combined stdout+stderr)
  failure.rs                 313 L — failure register + MAST taxonomy + reflections
  mismatch.rs  metrics.rs  insights.rs  contract.rs  hash.rs  lib.rs
scripts/
  verify.sh                   the deterministic gate (11 steps + CI-only sandbox skip,
                              head -20 truncation)
  checkpoint.sh               ECC: begin/verify + git-reset rollback + journal audit
  demo.sh, hitl-loop.template.sh
memory/                        canonical/ (append-only, dated, provenance) · derived/
                              (generated brief + per-domain AGENTS fragments) · episodic/
                              (buffer, checkpoints.log, verify.log)
evals/                        cases/ (24) · golden/ (9 JSON) · hidden/retry-policy · results/baseline.json
tickets/                      TICKET-N.md + TICKET-N-gates.md + claims.md (leases)
docs/                         CHALLENGE.md (charter, verbatim) · PLAN.md · ADRs (10)
.agents/skills/               15 skills (SKILL.md + frontmatter + verify hooks)
.codex/config.toml            codex agent thread/reasoning settings
```

### B.2 The loop (text map)

```
GAP (eval case composite < 0.5)                     eval.rs / insights.rs / loopcmd.rs
   │
   ▼
loop dispatch <case> --claimant <agent>             loopcmd.rs:410
   ├─ creates TICKET-N.md (spec) + lease in claims.md (ADR-0008 graph check)
   └─ spec carries MAST failure context + verify_command from evals/cases/<case>/run.json
   │
   ▼
WORKER = host agent                                procedural boundary (AGENTS.md)
   ├─ `mini-agi codex <spec> <workdir>`             main.rs cmd_codex — completion protocol
   │     capture (stdout+stderr) → run.json draft   capture.rs (ok=None, goal_aligned null)
   └─ or human via hitl-loop.template.sh
   │
   ▼
run ingest <run.json>                              insights.rs:49 → canonical memory (facts)
   │
   ▼
loop verify <case> --claimant <agent>              loopcmd.rs:471
   ├─ composite = outcome × trajectory-geomean × ticket-score   (eval.rs:638)
   ├─ run verify <run.json>  → deterministic verifier           (verifier.rs:53)
   ├─ eval gate --write-baseline  → best-state regression bound (eval.rs)
   ├─ CLOSED only if composite >= 0.5 AND verifier passes       (loopcmd.rs:20)
   └─ failures <run.json> → MAST failure register + reflections (failure.rs)
   │
   ▼
derive (brief + domain fragments) + checkpoint verify → next gap
```

Termination is therefore **not** model-declared at the loop level — it
is gate-enforced. The remaining trust-only edge is inside the worker
(Codex's own "task complete"), which the verifier closes *after* the
fact but nothing enforces *during*.

### B.3 Memory system

Three enforced layers, all file-based (no vector store, no DB):

- **Canonical** (`memory/canonical/entries/<date>/<date>-NNN.md`):
  append-only, dated, provenance block on every entry, fact ids
  `sha256[:16]` matching the PoC exactly. Only hand-written source of
  truth. Enforced: `memory signoff` + provenance gate.
- **Derived** (`memory/derived/`): generated brief (69 facts, per
  `derive`) + per-domain AGENTS fragments. Never hand-edited; conflict →
  canonical wins.
- **Episodic** (`memory/episodic/`): session buffer, `checkpoints.log`
  (BEGIN/VERIFY/STATUS — journal audited by gate), `verify.log`,
  `failures.md`, `mismatches.md`, `calibration.md`.

This is the same three-layer *shape* as babyagi3's (event log →
knowledge graph → summaries), but with the graph replaced by a
deterministic canonical layer and the summaries replaced by a generated
brief. It is more disciplined and cheaper than any vector-bag memory in
the benchmark — and it is **not searchable**: retrieval is *load the
whole brief*, not query. That is the deliberate context-budget tradeoff;
it stops scaling the moment the brief exceeds a model's window.

### B.4 Tools

- **Kernel tools (MCP, 34)**: `audit, backlog, budget,
  checkpoint_audit, eval_gate, eval_score, eval_steps, harness, health,
  insights, loop_dispatch, loop_status, loop_verify, memory_consolidate,
  memory_derive, memory_signoff, provenance, resume, run_failures,
  run_ingest, run_verify, skill_add, skill_list, skill_show, skill_verify,
  stats, ticket_claim, ticket_claims, ticket_graph, ticket_list,
  ticket_release, ticket_show, ticket_validate, validate`
  (mcp.rs `ToolDef` literals — 34, not the "21" the README claims). All
  introspective — the kernel acts only on its own repo.
- **Worker tools**: brought by the host agent (Codex/Claude). The kernel
  has **no** web/RAG/code-exec toolset of its own. For the "kernel for
  other agents" vision this is consistent; for "mini-AGI as an agent
  that acts in the world" it is the biggest capability gap.

### B.5 Config

No `.env`, no settings file. Everything is hardcoded (thresholds in
`loopcmd.rs`, `eval.rs`, `health.rs`), pinned (`rust-toolchain.toml`,
workspace lints), or procedural (AGENTS.md). The only real config file is
`.codex/config.toml` (codex's own agent settings). README documents
"phases 0-5 complete, 110 tests" — stale vs. current 190 tests / phases
6-10; the gate list in README also omits `insights`/`audit`/`sandbox`
steps that verify.sh now runs.

---

## C. Logic flaws / design gaps (file · problem · why · fix)

1. **`main.rs` codex worker — no enforced stop on the worker.**
   `cmd_codex` runs `codex exec` to completion with no step/cost/time
   cap and no per-step approval. Why: a stuck or runaway worker is
   unbounded. Fix: wrap in `worker.rs` with a hard deadline, step cap,
   cost cap (tokens read from the log), and abort-on-violation that
   writes a `VERIFY-FAIL`-style journal line, not just a transcript.

2. **`loopcmd.rs:20` `TARGET_COMPOSITE = 0.5` and eval thresholds
   hardcoded.** Why: no per-task or per-domain calibration, no env
   override. Fix: read from `.miniagi.toml` (P0-2) with per-case
   overrides in the ticket spec.

3. **`verifier.rs` — verifier only runs *after* the run.** The
   `verify_command` is declared in the case's run.json; `loop verify`
   refuses to close without it — good — but nothing refuses to **start**
   a worker whose spec lacks a verifier. Fix: P0-3 — dispatch refuses.

4. **`eval.rs` composite kills the run if any step score <= 0**
   (`geomean`). Why: a single probe command that fails (exit≠0,
   `ok:false`) zeroes the whole trajectory — this is *by PoC design*
   but it makes step-level noise catastrophic and is why the honest
   EXP-003 rerun scored exactly 0.5000. Fix: distinguish "probe failure"
   from "gate failure" at the step level (the `suspicious` verdict is
   the seam), or score steps in the failing *probe* family as
   `ok:None` rather than `ok:false` unless they touch scope.

5. **`audit.rs` + `health.rs` duplicate the severity model** (both
   `warn`/`critical`, both read `/proc`-adjacent state). Why: two sources
   of truth for "is the system ok". Fix: single `governance.rs` severity
   enum; health becomes the runtime probe, audit the repo probe.

6. **`main.rs` is 1847 L and growing** (cmd_codex + cmd_codex_reparse
   share run.json-building logic that drifted apart — the silent
   `parses_exp003_transcript` no-op in Phase 10 was the symptom). Why:
   CLI glue + worker logic in one file. Fix: extract `worker.rs` (codex
   runner + reparse) and `clifmt.rs` (draft builder), unit-test the draft
   builder directly.

7. **Memory retrieval is whole-brief only.** Why: brief grows linearly
   with facts; the day it exceeds the context window the kernel stops
   being able to read its own brain. Fix: P1 — domain-partitioned
   retrieval with a query step (`memory query <domain|keyword>`) so a
   run's prompt loads only the relevant fragment, not all 69 facts.

8. **`capture.rs` ok:None for all non-exit-evidenced steps.** Honest but
   coarse — every `bash -lc` line without `(exit N)` is unknown. Why:
   loses the `exit 2` signal the reviewer found (it sits on the *next*
   transcript line). Fix: P2 — look-ahead parser that binds the
   `(exit N)` from the following tool-result line to the preceding
   command.

9. **README stale** (phases 0-5 / 110 tests / gate list / "21 tools" —
    the MCP server exposes 34). Why: documentation drift between
    phases. Fix: one PR regenerating Status + gate list + tool count
    from `--help` / `mcp.rs`.

10. **ADR-0007 is referenced but missing.** `main.rs` documents the
    `validate` command as "Typed handoff contract validation
    (ADR-0007)" and `contract.rs` implements handoff/typing, but
    `docs/adr/` contains no ADR-0007 (the directory jumps 0006 → 0008).
    Why: an ADR is the recorded authority for a behavior; a referenced
    but absent ADR makes the contract's provenance unverifiable. Fix:
    either write ADR-0007 (the missing authority) or correct the
    references to the ADR that actually governs `contract.rs`.

11. **No global objective beyond one case.** `loop dispatch` is
    per-case; nothing models a multi-case objective with a global stop.
    Why: BabyAGI's self-perpetuating task list is the failure mode this
    avoids, but a single dispatch also can't express "close all gaps
    under budget X". Fix: P2 — `loop objective` that dispatches up to N
    independent gaps under a shared budget and stops when the global
    target is met or budget exhausted.

---

## D. Missing elements

| Element | Status | Where it would live |
|---|---|---|
| Planning / objective manager | **Missing** (plan = ticket spec written by `loop dispatch`) | `objective.rs` / ticket spec schema |
| Critic / reflection layer | **Partial** (post-hoc MAST reflections + `suspicious` step verdicts; no in-loop critic) | `critic.rs` — one cheap LLM pass over a finished trajectory before ingest |
| Safety / governance layer | **Missing enforced** (ADR-0009 is CI-only; no allowlist, no budget) | `governance.rs` + `worker.rs` |
| Layered memory (search) | **Partial** (3 layers, no query) | `memory query` in `memory.rs` |
| Modular tools (kernel-side) | **Missing** (kernel has no act-in-world tools) | `tools/` behind a `Tool` trait — or consciously keep worker-owned |
| Target API | **Partial** (CLI + stdio MCP; no HTTP/UI) | MCP is the right seam; add a read-only `mcp` `health`/`audit` exposure before any HTTP |
| Budget / cost governor | **Missing** (tokens recorded, not capped) | `budget.rs` |
| Repetition watchdog | **Missing** (Reflexion avoids *repeated failures*, not *repeated identical actions*) | in `critic.rs` |

---

## E. Benchmark vs external repos

### E.1 muellerberndt/mini-agi (Python, ~350-line agent)

- **Strengths:** tiny and readable; disciplined prompt with exact output
  format; regex-forced structured output + retry-on-parse-failure;
  dual-buffer context memory (recent items + rolling model summary);
  optional cheap critic pass; `done` command + `PROMPT_USER` pause.
- **Weaknesses:** bare `exec()` / `shell=True` with no sandbox; memory is
  in-process only (lost on exit); **termination is trust-only** — the
  model declares `done` and the README itself shows it faking success;
  no budget/iteration cap.
- **Port:** regex/schema-forced structured output (we do it via run.json
  validation — keep); the cheap-critic-pass idea (missing, D).
- **Do not port:** `exec()`/`shell=True`; trust-only termination (our
  verifier already replaces it).

### E.2 tdolan21/miniAGI (Streamlit + LangChain)

- **Strengths:** streaming ReAct step callbacks in a chat UI (auditable
  UX); persistent PGVector RAG; description-driven tool selection;
  GitHub-codebase-import pattern.
- **Weaknesses:** ~200 mandatory pinned deps (torch, deeplake,
  banana...) for a chat demo; `PlanAndExecute`, `ConversationBufferMemory`
  and chat history all **built but never wired in** (dead weight);
  hardcoded `session_id="16390"`; re-embeds all docs on every start; no
  tests.
- **Port:** retriever-as-a-tool (only if we add a kernel RAG later);
  step-callback streaming if an HTTP UI ever appears.
- **Do not port:** the dependency monolith (we are std-only by ADR);
  built-but-unwired abstractions (name the dead code and delete it).

### E.3 Auto-GPT (classic Forge)

- **Strengths (the guards, not the machinery):** `cycle_budget`
  tri-state (None/1/0); Watchdog repetition detection (rethink with the
  bigger model on repeat); layered permission glob-match with
  `{workspace}` + session-denied set + reject-then-feedback;
  lazy last-N-verbatim + summarize-older history; output-size guard;
  `finish`-as-exception; skills progressive disclosure; typed
  env-overlaid config.
- **Weaknesses:** component/protocol pipeline is heavily overengineered
  (metaclass auto-discovery, topological ordering, three error tiers,
  argument reversion); the flagship Forge agent is a nonfunctional stub;
  termination has no verification; memory is just context-window
  management; the vector memory it is famous for was **abandoned by the
  project itself** — strong evidence that design was performance theater.
- **Port:** cycle budget (P0-2); watchdog repetition detection (P1);
  permission allowlist (P0-1); lazy summarization for long worker
  histories (P1); output-size guard on codex capture (P1);
  `finish`-raises exception as the gap-close signal (P2);
  `UserConfigurable(from_env)` (P0-2).
- **Do not port:** the component pipeline; the 7 prompt-strategy zoo;
  the platform graph engine; per-cycle directive re-injection;
  re-adopting vector memory.

### E.4 BabyAGI family

- **Original:** the durable idea — *hold the plan as data outside the
  model, re-read it per step* — is exactly what tickets + spec + run.json
  already do. The implementation is bad: LLM re-prioritizes the *whole*
  list every step (O(step²) cost, drift), has no tools, and self-
  perpetuates ("list empty = done").
- **2o:** model writes its own tools via `exec()` + auto `pip install` —
  genuinely novel, but with no sandbox, no persistence, no verification,
  and it advertises every key-like env var in the prompt.
- **3:** the robust engineering version — context-budget trimming with
  staged overflow recovery, always-appended `tool_result`, per-thread
  locks, objectives with priority/budget/retry/backoff/cancellation that
  **inject prior error history into the next prompt**, persisted +
  E2B-sandboxed dynamic tools. Also: ~2700-line `agent.py` despite a
  "~300 lines of core" claim, speculative breadth, cost without a
  governor by default, and verification that is still prompt-based.
- **Port:** objectives-with-budget/retry/cancel (P0/P1 — budget and
  cancellation are the missing primitives); retry-with-error-history
  (already have — MAST reflections, keep); context-budget trimming with
  staged recovery for long worker sessions (P1); persisted-and-disabled-
  on-fail dynamic skills (P2).
- **Do not port:** per-step full re-prioritization (we keep gated,
  human-owned tickets); the always-running loop; the "multi-agent"
  executor/creator/prioritizer branding — it is one model with role
  prompts, and the prioritizer adds cost without information (2o and 3
  both dropped it).

### E.5 Multi-agent repo analyzers

- **nozikov/github-repo-analyzer:** single LLM, role-separated LangGraph
  branches (fetch→plan→analyze/similar/web→synthesize), markdown report.
  Pattern: fan-out-then-synthesize with one report. Simple, cheap.
- **repo-health-report:** 6 deterministic dimensions scored in parallel,
  LLM only as opt-in `--ai` second opinion. Pattern: **deterministic
  layer first, LLM layered on top** — exactly our `judge-drift`
  philosophy; worth keeping and extending.
- **repo-insight:** 4 truly specialized agents (static/behavior/
  community/reporter) with ConflictResolver + GuardrailValidator +
  TimeoutGuard with tiered degradation. Pattern: hard per-agent budget +
  degradation (cache→mean→constant). Port the guard ideas, not the
  Docker+React+Prometheus stack.
- **Do not port:** LLM consensus voting without a deterministic layer;
  infra-heavy dashboards.

### E.6 Claude-centric skills / spec-kit

- **mattpocock/skills:** SKILL.md frontmatter (`name`/`description`/
  `disable-model-invocation`), user-invoked vs model-invoked split, a
  router skill, *completion criteria on every step*, progressive
  disclosure, two-axis parallel review without reranking. We already
  have the SKILL.md shape + verify hooks; the **router skill** and the
  **user/model invocation split** are the portable additions.
- **github/spec-kit:** constitution → specify → plan → tasks → implement
  → **converge** (re-assess and re-open remaining work). We have the same
  chain (to-spec → to-tickets → implement → loop verify/reopen). The
  **converge step is literally our loop verify + backlog**. Reject the
  extensions/presets/bundles template-priority-stack — ceremony a
  single-binary kernel does not need.
- **ykdojo/claude-code-tips:** handoff/compaction discipline (we have
  `handoff` + `compact` skills already).
- **Do not port:** "specifications become executable" (marketing);
  spec-kit's bundle/extensions catalog stack.

---

## F. Hardening backlog

### P0 — safety and loop stability

1. **Worker guardrails enforced in-kernel** — new `worker.rs`:
   max-steps, max-cost, max-wall, diff-allowlist; abort + register on
   breach; used by `cmd_codex` and the hitl template. (C.1)
2. **Budget/stop as data** — `cycle_budget` tri-state + first config file
   (`.miniagi.toml` with env overlay) for `TARGET_COMPOSITE`, regression
   tolerance, health thresholds; `UserConfigurable(from_env)` pattern.
   (C.2)
3. **No-dispatch-without-verifier** — `loop dispatch` refuses a gap whose
   case declares no `verify_command`; `mini-agi codex` refuses a spec
   without one. (C.3)
4. **Sandbox for worker exec** — CI-only attestation is not enough:
   wrap `codex exec` in a `bwrap`/`landlock` profile (or document the
   trust boundary explicitly in AGENTS.md as an ADR decision). (A-risk 1)

### P1 — quality, predictability, DX

5. **Repetition watchdog** — detect identical consecutive actions in the
   captured trajectory; escalate (flag for judge / abort) instead of
   letting the worker spin. (D)
6. **Lazy history summarization** — for long worker sessions, summarize
   older steps (preserve-facts instruction) instead of dumping the whole
   transcript into the draft. (E.3 port)
7. **Memory query** — `memory query <domain|keyword>` loading only the
   relevant canonical fragment; keep whole-brief for `derive`. (C.7)
8. **Probe-vs-gate step scoring** — distinguish failed probes from failed
   scope steps so a single exit≠0 probe does not zero a composite. (C.4)
9. **README/gate doc refresh** — one PR, from `--help` + verify.sh. (C.9)
10. **Codex capture output-size guard** — truncate oversized tool
    results with an explicit error to the model (Auto-GPT pattern). (E.3)

### P2 — nice to have

11. **`loop objective`** — dispatch up to N independent gaps under a
    shared budget with a global stop condition. (C.10)
12. **Capture look-ahead exit binding** — attach `(exit N)` from the
    following tool-result line to the command. (C.8)
13. **`finish`-raises-exception** gap-close signal in `loop verify`.
14. **Persisted dynamic skills** — skills created at runtime persisted +
    disabled-on-fail (babyagi3). 
15. **Extract `main.rs` → `worker.rs` + `clifmt.rs`**; delete dead code
    (the unused-planning-style dead weight seen in tdolan21/miniAGI —
    audit for our own). (C.6)

---

## G. Proposed target architecture

### G.1 Directory layout (target)

```
crates/mini-agi-core/src/
  memory/        memory.rs store.rs            (unchanged seams, + query.rs)
  eval/          eval.rs verifier.rs            (+ probe_vs_gate.rs)
  loop/          loopcmd.rs objective.rs        (dispatch/verify + global objective)
  governance/    governance.rs budget.rs worker.rs critic.rs permission.rs
  kernel.rs      lib.rs                         (wiring + Config)
crates/mini-agi/src/
  main.rs        (thin dispatch only)
  worker.rs      (codex runner + reparse — moved from main.rs)
  clifmt.rs      (run.json draft builder — unit-tested)
  mcp.rs
config/
  .miniagi.toml.example        (thresholds, budgets, allowlist)
scripts/verify.sh              (adds governance gate step)
```

### G.2 Module boundaries (who talks to whom)

- `loop/` → `eval/`, `memory/`, `ticket/` — the only orchestrator.
- `governance/budget.rs` + `governance/worker.rs` wrap the external
  worker: **nothing executes outside the repo without passing through
  them**.
- `governance/critic.rs` reads a finished trajectory and writes a
  verdict into the run draft **before** `loop verify` (kept cheap:
  optional, one LLM pass, deterministic fallback = current score).
- `governance/permission.rs` holds the allowlist consumed by
  `worker.rs` (glob, first-match-wins, session-denied set — Auto-GPT).
- Config is the single `kernel.rs → Config` struct; every module takes it
  by reference; no hardcoded thresholds remain.

### G.3 The ideal loop (with the new layers)

```
GAP → dispatch (refuses no-verifier) 
   → worker runs UNDER budget.rs + permission.rs (steps/cost/time caps, allowlist)
   → capture (honest ok/None) 
   → critic.rs (optional cheap pass: repetition? scope? objective?) → verdict into draft
   → ingest → loop verify (composite + verifier + best-state bound) 
   → on breach of budget/watchdog: journal VERIFY-FAIL + register, NOT silent
   → derive + checkpoint → next gap or objective done
```

Every step keeps a deterministic core and an optional LLM layer on top —
the one architectural rule all the good benchmark patterns share
(A2/A3/judge-drift). Never invert it.

### G.4 Plugging in multi-agent / scheduler

- **Multi-agent:** the kernel already *is* an orchestrator-worker system
  (dispatch → codex). Don't add subagents inside the kernel; add a second
  worker *type* (e.g. `claude`) behind the same `worker.rs` trait so N
  agents can be dispatched under one budget — that is the repo-analyzer
  fan-out pattern without the infra.
- **Scheduler:** port babyagi3's *persisted* schedule (at/every/cron,
  JSON store, single timer loop — no busy-wait) only if recurring
  background runs are a real need; otherwise skip. P2.

---

## H. Concrete implementation plan

### H.1 One PR (small, immediately shippable)

1. P0-3: dispatch + codex refuse no-verifier (C.3) — touches
   `loopcmd.rs`, `main.rs`, ~30 lines + 2 tests.
2. P0-2 part: `Config` struct reading `.miniagi.toml` + env overlay for
   `TARGET_COMPOSITE` and regression tolerance; keep defaults identical
   (behavior-preserving).
3. README refresh (C.9).

### H.2 One sprint

4. P0-1: `worker.rs` guardrails (steps/cost/time/diff) + wiring into
   `cmd_codex`; 1 falsifier test ("worker aborts when budget exceeded").
5. P0-4: sandbox profile for worker exec or an explicit ADR documenting
   the trust boundary as a decision.
6. P1-5: repetition watchdog in `capture.rs`/`critic.rs`.
7. P1-8: probe-vs-gate step scoring in `eval.rs`.
8. P1-10: output-size guard in codex capture.
9. Extract `main.rs` → `worker.rs` + `clifmt.rs` with unit tests for the
   draft builder (C.6).

### H.3 Bigger refactor (needs ADR)

10. `eval/` + `loop/` + `governance/` module split (G.1) — mechanical,
    gated by the existing suite.
11. P1-7 `memory query` + domain-partitioned retrieval.
12. `loop objective` with global stop (P2-11) — new ADR-0012 for the
    budget semantics.

### H.4 Package split (only if/when reuse is real)

- `mini-agi-core` is already the natural library. Split further only if a
  second consumer appears: `mini-agi-governance` (worker/permission/
  budget — reusable by other agent kernels) and `mini-agi-eval`
  (verifier/calibration — reusable as a judge-drift library). Otherwise
  keep one crate: a monorepo of crates for a 14k-line kernel is exactly
  the Auto-GPT platform sprawl this repo should not copy.

---

## Part 5 — Critical thinking summary (honest verdicts)

- **What is banal but effective (keep and export):** the deterministic
  verifier + calibration + judge-drift (the only enforced termination in
  the benchmark); the counterfactual harness gate; append-only canonical
  memory with provenance; the checkpoint journal with orphan detection;
  std-only zero-unsafe kernel. None of these are novel — that is
  precisely why they are trustworthy.
- **What is overengineered elsewhere (reject):** Auto-GPT's component
  pipeline and platform graph engine; babyagi3's ~2700-line "simplest"
  agent and its speculative tool breadth; tdolan21's 200-dep monolith
  and its built-but-unwired abstractions; spec-kit's bundle/preset
  catalog stack; repo-insight's Docker+React+Prometheus for a repo
  scorer.
- **What this repo overengineers (be honest):** 10 ADRs, a harness
  ledger, hidden eval corpus, per-domain AGENTS fragments — for a
  single-user pipeline that is a heavy on-ramp. The *enforcement* is
  load-bearing; some of the *ceremony* (docs furniture) is not. Name the
  P0/P1 list as the minimum that must ship; treat the rest as
  documentation debt, not architecture.
- **The one structural tension to accept deliberately:** the kernel
  cannot both be a zero-dependency "verified brain for other agents" and
  an "agent that acts in the world". The charter's "mini-AGI" framing
  pushes toward the latter; the architecture is the former. Resolve it
  with an explicit ADR: worker-owned action, kernel-owned memory/truth —
  and add the worker guardrails (P0) so the boundary is enforced, not
  assumed.

## Implementation status (2026-08-04)

The P0/P1 backlog below is being implemented slice by slice (each slice:
tests + verify ALL GREEN + pushed + CI green). Status:

- **P0-3 (no trust-only worker runs) — DONE.** `mini-agi codex` refuses
  to execute a spec that declares no verifier (`--verify`/`--target` or
  the spec's embedded `verify_command`); `write_spec` embeds the case
  verifier when present. Enforcement is at the worker boundary, not
  dispatch — historical frozen fixtures predate the verifier.
- **P0-2 (budget/stop as data) — DONE.** `config.rs` (`.miniagi.json` +
  `MINIAGI_*` env overlay, behavior-preserving defaults); loop target,
  eval-gate tolerance, and dispatch floor resolve from config.
- **P0-1 (worker guardrails) — DONE.** `worker.rs`: `run_capped` kills
  the worker at the wall-time cap; `cmd_codex` aborts (exit 3, honest
  draft) on wall/step breach; `run ingest` refuses a run over
  `max_cost_usd`.
- **P1-5 (repetition watchdog) — DONE.** `eval::max_consecutive_repeat`;
  `loop verify` warns when a run exceeds `max_repeated_steps`.
- **ADR-0007 written** (the referenced-but-missing authority).
- **README refreshed** (phases 0-10, 190 tests, 34 MCP tools, gate list).
- **Deferred (documented, need ADR/decision):** probe-vs-gate step
  scoring (changes scoring semantics + baseline), sandbox for worker
  exec (external tooling decision), module split, `loop objective`,
  `memory query`.

## Status
- Everything above is grounded in the current worktree (`1d3e4c8` and
  the hardening commits `e7225d7`/`3799353`/`e9b7bf9`) and the fetched
  sources. Unconfirmed items are marked as such inline.
- This audit is the deliverable of the deep-research goal; it proposes
  changes and does not modify code. Implementation is tracked by the
  P0/P1/P2 backlog above (H.1 starts with a single PR).
