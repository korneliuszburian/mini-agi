# mini-agi — master plan (v4, Rust product)

Status: EXECUTING. Phase 0 complete (kernel hash/store/memory + CLI,
28 tests green, clippy/fmt clean, release build). This is the plan of
record for the Rust rewrite. Charter: `docs/CHALLENGE.md` (verbatim,
founding user prompt — never lose). ADRs: `docs/adr/ADR-0001..0003` (+
inherited PoC ADR-0001..0012 semantics).

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

### Phase 1 — Eval engine
- run.json parsing (codex JSONL), 4D scoring, golden matching, baseline,
  regression gate. Port 11 eval cases as fixtures.
- Acceptance: `mini-agi eval run` reproduces PoC scores on all 11 cases.

### Phase 2 — Skills
- SKILL.md frontmatter parsing, verify-hook execution, registry with
  versioning, project scoping, install from GitHub (`mini-agi skill add`).
- Acceptance: our first 3 in-repo skills with passing verify tests.
- Ports from PoC: `.agents/skills/` minus grilling stubs (ADR-0003); every
  ported skill gets checkable+exhaustive completion criteria (Matt Pocock
  standard — our v2 skills violated it) + a verify test. Reviewer skill
  has memory-anchor gate (verdict without canonical fact ids = fail).

### Phase 3 — Orchestration
- checkpoint journal + audit (PORTED audit semantics from T008 amendments),
  ticket lifecycle, session resume (`codex exec --resume`), cache-first
  prompt layout, budgets.
- Acceptance: full ticket run through mini-agi without PoC scripts.

### Phase 4 — MCP server + adapters
- MCP stdio server (memory/eval/skills tools), codex adapter, docs for
  Claude/Cursor/opencode wiring.
- Acceptance: `mini-agi mcp` + `mcp-remote` connect; a foreign agent reads
  brief and writes an enforced fact.

### Phase 5 — Dogfood + productize
- mini-agi runs its OWN tickets through itself (memory in `memory/`).
- CLI polish, install path (`cargo install`), README, demo, versioning.

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
