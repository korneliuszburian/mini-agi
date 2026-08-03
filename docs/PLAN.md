# mini-agi — master plan (v4, Rust product)

Status: COMPLETE. Phases 0-5 done (memory, eval, skills, journal,
contracts, metrics, MCP+adapters, ticket lifecycle, init, CI, demo,
dogfood incl. live codex-through-MCP; 110 tests green, verify ALL
GREEN, version 0.3.0). This is the
plan of record for the Rust rewrite. Charter: `docs/CHALLENGE.md` (verbatim,
founding user prompt — never lose). ADRs: `docs/adr/ADR-0001..0004` (+
inherited PoC ADR-0001..0012 semantics).

Roadmap after COMPLETE: Phase 6 below (intelligence loop closure,
ADR-0005 failure-signal loop — gaps are the plan; last update 2026-08-03).

Three generations, one lineage (ADR-0001):
- v1 `agentic-core` — loop proof; canonical facts = knowledge source.
- v2 `mini-agi` (tag `v1-spec-reference`) — FROZEN behavioral spec: 82
  tests, 11 eval cases, golden trajectories, ADR-0001..0012. The contract.
- v3 `mini-agi-rs` — this repo. Spec = PoC; knowledge = agentic-core@HEAD.

## Vision — why this blows the doors off

Everyone ships *parts*: memory tools (Anthropic memory-tool, Mem0, Letta),
skills registries (skills.sh, Agent Skills standard), eval harnesses (Ralph
Wiggum loop, IBM judge), orchestrators (AgentKit, MS Agent Framework). Nobody
ships them as ONE kernel that any agent plugs into.

`mini-agi` = a single static binary (Rust) providing:

1. **Enforcement-bound memory** (our ADR-0010 — nobody else has this): every
   canonical fact is either *enforced* (bound to a check that runs in CI) or
   flagged *reference*. Memory with teeth, not vibes.
2. **Eval-every-run**: every agent run leaves `run.json` (trajectory + 4D
   score + golden) and a baseline; `gate` blocks merges on regression. The
   system measures itself, always.
3. **Verifiable skills**: a skill = procedure + verification test. Install,
   scope per project, run the test in CI. (Voyager pattern — the only
   published compounding evidence.)
4. **Cross-agent**: exposed as MCP server — Codex, Claude, Cursor, opencode
   all plug into the SAME verified brain through the standard protocol.
5. **Cache-first orchestration**: session resume + static-prefix prompts so
   the cost curve drops per ticket instead of staying flat.

Killer demo: `mini-agi init` in a repo → MCP config → your agent (any agent)
gets a verified brain, self-measuring evals, verifiable skills, and a merge
gate that refuses regressions.

## Architecture

```
mini-agi-rs/  (Cargo workspace)
├── crates/
│   ├── mini-agi-core/    — THE KERNEL (library, zero I/O deps beyond std)
│   │   ├── hash.rs       — sha256, provenance hashing (port PoC)
│   │   ├── store.rs      — append-only entry store, dedup (port PoC)
│   │   ├── memory.rs     — canonical/derived/review/consolidate/signoff
│   │   ├── eval.rs       — run.json, 4D scoring, golden, baseline, gate
│   │   ├── skills.rs     — SKILL.md parse, verify hook, registry, scoping
│   │   ├── journal.rs    — checkpoint BEGIN/VERIFY/FAIL + audit semantics
│   │   └── contract.rs   — JSON Schema types (typed handoffs, ADR-0007)
│   └── mini-agi/         — BINARY: CLI + MCP server + agent adapters
│       ├── main.rs       — CLI (init, mem, eval, gate, skill, ticket, mcp)
│       ├── mcp.rs        — stdio MCP server (JSON-RPC 2.0, hand-rolled)
│       ├── adapters/     — codex exec, claude, opencode session adapters
│       └── derive.rs     — brief/fragments regeneration (port PoC)
├── skills/               — REMOVED (ADR-0002): skills live in .agents/skills/
├── tests/                — integration tests (behavioral contract from PoC)
└── memory/               — mini-agi's OWN memory (dogfooding, ADR-0009)
```

**Deliberate choices:**
- Kernel = library with zero deps beyond `sha2`+`serde`; binary = thin shell.
  Testable, embeddable, auditable.
- MCP server hand-rolled (JSON-RPC 2.0 over stdio): the protocol is small,
  we control the spec, zero framework lock-in. `rmcp` only if we hit spec
  subtleties we can't afford to own.
- No async for v1 — MCP stdio is synchronous request/response. `tokio` only
  if a real need appears (nothing does for a stdio server).
- Storage: markdown+git, same as PoC. Provenance hashes, append-only entries,
  derived views regenerated. Migration-free, human-readable, diffable.
- Skills live in `.agents/skills/` (project-scoped by default) — the
  standard discovery path for Codex/Claude/Cursor/opencode (ADR-0002).
  Global skill dirs are frozen/disabled (`~/.agents/skills.disabled`). No
  leakage.
- HITL = external LLM reviewer, memory-anchored (ADR-0003): independent
  session, MUST cite canonical fact ids, deterministic gates first
  (IBM/AAAI: judge ~45% vs judge+tools ~94%). Grilling removed from spec.

## Key decisions (evidence-linked)

| Decision | Why | Source |
|---|---|---|
| Deterministic gates first, LLM judge second | judge alone catches ~45% of errors; +tools ~94% | IBM/AAAI 2026 |
| Single-writer + reviewer pipeline | multi-agent fails 41-86% in production | Cognition |
| Memory = context mgmt, hard cap | MemGPT; 200-line cap; our 8KB brief works | research |
| Skills = verified procedures | only clean compounding evidence | Voyager |
| Cache-first + session resume | T007: 1.7M/1.8M cached in-run; fresh session = miss | our runs |
| Past-failure register | +4.6% SWE-bench; stop repeating fixes | Reflexion |
| Sandbox-first (v3 pipeline) | consensus; our biggest gap | research |
| Outcome-denominated metrics | flat cost/ticket is normal; quality is the signal | METR |
| Cross-agent MCP | reach > framework; any agent gets the brain | standards |

## Engineering standards (binding)

- Edition 2024, rust-version 1.97.1 pinned in `rust-toolchain.toml`.
- `unsafe_code = "forbid"`; clippy all+pedantic+nursery+cargo warn; fmt check;
  both are CI gates (`cargo clippy --all-targets -- -D warnings`).
- Kernel crate (`mini-agi-core`): no async/tokio, no orm/sqlite, no rmcp
  without an ADR. Deps: sha2 0.11, serde 1, serde_json 1, thiserror 2.
- Release: LTO fat, strip, codegen-units 1, panic=abort (single static binary).
- Behavior ports from PoC are test-locked: same inputs, same expected outputs,
  including exact 16-hex fact ids.
- No comments beyond doc comments; doc comments required on public items
  (`missing_docs = "warn"`).

## Phases

### Phase 0 — Foundations (DONE)
- Cargo workspace, core crate, hash+store+memory with tests ported from PoC.
- Acceptance: `cargo test` green; `mini-agi mem add/consolidate/derive`
  behaves identically to PoC scripts (same file layout, same hashes).
- Deliverable: crates skeleton + memory engine + contract JSON Schemas.
- Evidence: 28 tests (6 unit + 11 integration + 10 CLI subprocess), clippy
  `-D warnings` clean, fmt clean, release build OK.

### Phase 1 — Eval engine (DONE)
- run.json parsing (codex JSONL), 4D scoring, golden matching, baseline,
  regression gate. 11 eval cases ported as fixtures.
- Acceptance: `mini-agi eval run` reproduces PoC scores on all 11 cases.
- Evidence: `crates/mini-agi-core/src/eval.rs` (score_run, score_all_cases,
  run_gate — TOOL_PARITY_PENALTY=0.85, DEFAULT_TOLERANCE=0.05,
  MAX_COST_GROWTH=1.25), 14 tests incl. 1:1 PoC baseline reproduction
  (composite/tokens/cost, tol. 0.0005), CLI `eval score`/`eval gate`,
  gate `PASS: 11 cases, 0 regressions`; 51 tests green, clippy/fmt clean.
- Commit: 3aa9f48.

### Phase 2 — Skills (DONE)
- SKILL.md frontmatter parsing, verify-hook execution, registry with
  versioning, project scoping, install from GitHub (`mini-agi skill add`).
- Acceptance: our first 3 in-repo skills with passing verify tests.
- Ports from PoC: `.agents/skills/` minus grilling stubs (ADR-0003); every
  ported skill gets checkable+exhaustive completion criteria (Matt Pocock
  standard — our v2 skills violated it) + a verify test. Reviewer skill
  has memory-anchor gate (verdict without canonical fact ids = fail).
- Evidence: `crates/mini-agi-core/src/skills.rs` (frontmatter parser with
  multiline/quoting, discovery, verify-hook exec, `skill add` via git
  clone), CLI `skill list/show/verify/add`, `scripts/verify.sh` (silent
  target = fail), `scripts/checkpoint.sh` (ECC port), 15 skills ported
  with completion criteria, verify hooks green on verify/checkpoint/review
  (+ review-anchor-test). Commits: 1a71e91, fdc3fa1.

### Phase 3 — Orchestration (DONE)
- checkpoint journal + audit (PORTED audit semantics from T008 amendments),
  typed contracts (ADR-0007), budgets, cache-first prompt layout.
- Ticket lifecycle/session resume are agent-side (skills + codex adapters);
  the kernel side (journal + audit + contracts + metrics) is complete.
- Evidence: `journal.rs` (checkpoint-gate timestamp semantics + audit.sh
  line semantics: orphan BEGIN, in-flight exception, boundary = newest
  complete green), `contract.rs` (validate.py port, 4 bundled schemas),
  `metrics.rs` (stats + budget ports), CLI `checkpoint audit`/`validate`/
  `stats`/`budget` wired into verify.sh. Commits: c248c03, b3209f5, e0ca29c,
  e3f9c8f.

### Phase 4 — MCP server + adapters (DONE)
- MCP stdio server (memory/eval/skills/ticket tools), codex adapter,
  opencode wiring.
- Evidence: `mcp.rs` (dual framing: LSP Content-Length + rmcp/codex
  newline-JSON — the codex transport was found live and fixed after its
  handshake timed out; protocol 2025-03-26, 17 tools,
  initialize/tools-list/tools-call/ping), handshake test covering BOTH
  framings, `.codex/agents/*.toml`, `opencode.json` (dogfood MCP).
- PROVEN live: codex exec (gpt-5.6-terra) called `provenance`, `stats`,
  `memory_consolidate` (new fact landed, dedup by content hash) and
  `memory_derive` through the MCP server — Phase 4 acceptance
  "a foreign agent reads brief and writes an enforced fact" demonstrated.
  Commits: ce03339, c21220d, 0ffbb31.

### Phase 5 — Dogfood + productize (DONE)
- mini-agi runs its OWN tickets through itself (memory in `memory/`).
- CLI polish, install path (`cargo install --path crates/mini-agi`),
  README, demo, versioning (0.2.0).
- Evidence: `mini-agi init` scaffolds a repo (layout, embedded gate
  scripts, AGENTS.md, review rubric, opencode.json MCP config pointing at
  the binary; idempotent, scripts chmod +x), E2E test
  `cli_full_ticket_run_end_to_end` (init -> consolidate -> derive ->
  provenance -> checkpoint begin/audit -> validate -> stats -> budget),
  `scripts/demo.sh` killer demo, `.github/workflows/ci.yml` runs the full
  gate on pinned 1.97.1, dogfood fact in canonical memory, gate green on
  a fresh init'd repo (eval gate passes with zero cases).
- Codex review fixes: eval gate never silently re-baselines (missing
  baseline = fail, explicit `--write-baseline` only), checkpoint rollback
  always lands on the last BEGIN (ADR-0004, Coherence Collapse hole),
  MCP tool schemas carry real JSON types + tolerant arg parsing.
- Commits: 0bf2515, 96fc29d, 0.2.0, v0.2.0 tag.

### Phase 5 — Dogfood + productize
- mini-agi runs its OWN tickets through itself (memory in `memory/`).
- CLI polish, install path (`cargo install`), README, demo, versioning.

### Phase 6 — Intelligence loop closure (ROADMAP, ADR-0005 failure-signal loop)
Derived 2026-08-03 from the system's own measurements (`mini-agi insights`,
`mini-agi backlog`). The failure signal IS the roadmap; targets are measured,
not aspirational.

1. **Failure register (Reflexion)** — closes TICKET-9 (`reactive-loop`
   composite 0.0000: agent repeats the identical failing action 3x — edit
   same line -> verify fail -> repeat — with zero reflection). The
   "past-failure register +4.6% SWE-bench" decision in the table above was
   never implemented in v3. Product slice: per-run failure register
   (file+line+tool+action hash) surfaced through `resume`/brief so a fresh
   session never repeats a recorded failure; trajectory scorer flags
   repeated identical actions. Target: reactive-loop composite > 0.5 on
   rerun.
2. **Tool-mismatch reduction** — real-ticket-001..008 carry 4-6 tool
   mismatches/run at composite 0.24-0.52 (tool_parity 0.85 penalty). Slice:
   score tool usage against the real MCP tool catalog, fix description/
   schema drift, re-run. Target: mismatches <= 1 per run.
3. **Sandbox-first (v3 pipeline)** — DONE (2026-08-03, ADR-0009): the
   CI gate runs in-sandbox on master since Phase 5
   (`.github/workflows/ci.yml`, GH Actions runner, pinned 1.97.1); the
   gate now *attests* isolation — `verify.sh` gains a `sandbox` target
   that fails in CI unless the runner is non-root and identifies itself.
   Deferred: local-agent sandboxing (bwrap/firejail) — revisit when 6.4
   dispatches untrusted work.
4. **Proactive composition (intelligence layer 2)** — DONE (2026-08-03):
   `mini-agi loop status|dispatch|verify` closes the loop without human
   routing — dispatch picks the worst open case (below 0.5, unclaimed,
   no passing rerun), ensures its ticket (work graph ADR-0008), claims
   it (lease), and writes the slice spec; verify scores + ingests the
   rerun, releases the lease at composite >= 0.5. Demonstrated
   end-to-end: real-ticket-001-v2 (0.2402) -> TICKET-001-v2 ->
   real-ticket-001-v2-rerun composite 0.7225 -> CLOSED; insights gaps:
   none; composite avg 0.4469 -> 0.4681. Cases with a passing rerun are
   excluded from insights capability gaps (fixture stays historical).

Acceptance for Phase 6 as a whole: `mini-agi insights` shows composite avg
>= 0.60, zero open capability gaps, gate ALL GREEN, and the whole loop
(reingest -> insights -> backlog -> implement -> rerun) demonstrated end to
end on one real gap.

Phase 6 acceptance status (2026-08-03): COMPLETE. The loop ran over ALL
remaining gaps — flailing, real-ticket-002-v2, -003-v2, -004-v2, -005-v2,
-006-v2, -007-v2 each got a real implementation rerun (0.6141-1.0000;
real-ticket-007-v2-rerun scored a perfect 1.0000). ADR-0010: historical
failing fixtures stay as gate regression evidence; insights reports the
capability mean (`composite_avg_effective`, rerun overrides its original,
each case once) alongside the plain history mean. Measured: effective
0.7431 >= 0.60 (history 0.5611), zero open capability gaps, gate ALL
GREEN, loop demonstrated end-to-end on real gaps without human routing.

## Rejected (with reason)
- Python for the product (user decision; PoC = spec only).
- tokio/async v1, rmcp, orm, sqlite — YAGNI for a stdio tool; kernel stays
  std-only.
- Multi-parallel implementers — evidence says single-writer wins.
- Zep-style KG in context — 600k token overhead.
- Dreaming/unsupervised consolidation — drift risk.
- Porting our 17 superpowers skills — user decision: from scratch, small,
  with verify tests.
- Grilling (`grill-me`/`grill-with-docs`, `/grilling`, `/domain-modeling`) —
  dead stubs referencing non-existent commands; HITL in our system is the
  external memory-anchored reviewer (ADR-0003).

## Dogfooding contract (hard rule)
mini-agi's own memory/ + skills/ are first-class users of the kernel. Every
phase lands with the product's own state maintained through itself (facts
enforced, evals run on real sessions, skills verified in CI).
