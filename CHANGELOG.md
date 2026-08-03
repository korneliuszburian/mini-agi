# Changelog

All notable changes to mini-agi. Format: Keep a Changelog; this project
follows semantic versioning (workspace `Cargo.toml` `version`).

## [Unreleased]

### Added (intelligence layer, ADR-0005 — Sequoia "From Hierarchy to
Intelligence" direction)
- `mini-agi loop` (Phase 6.4, proactive composition — Wish Factory
  pattern): `status` lists cases below the 0.5 target with tickets,
  claims and rerun evidence; `dispatch` picks the worst open case,
  ensures its ticket, claims it (lease, ADR-0008) and writes the slice
  spec (`artifacts/<ticket>/spec.md`) for a fresh session; `verify`
  scores + ingests the rerun and releases the lease at composite >= 0.5.
  Demonstrated end-to-end: real-ticket-001-v2 0.2402 ->
  real-ticket-001-v2-rerun 0.7225 -> CLOSED, capability gaps: none.
- Cases with a passing `-rerun` are excluded from insights capability
  gaps (the original run stays a historical fixture, TICKET-9
  semantics).
- The loop closed ALL remaining gaps (2026-08-03): flailing and
  real-ticket-002-v2..006-v2 each received a real implementation rerun
  (consolidate/compact loop, checkpoint allowlist + exit-code integrity,
  role-aware scope scorer, scorer integrity, consolidate hardening) —
  rerun composites 0.6141-0.8500, zero cases below 0.5 without a passing
  rerun, insights composite avg 0.4469 -> 0.5380, baseline refreshed to
  19 cases, gate ALL GREEN.
- ADR-0010 fixture policy + Phase 6 acceptance COMPLETE (2026-08-03):
  historical failing fixtures stay as gate regression evidence; insights
  reports `composite_avg_effective` (passing rerun overrides its
  original, each case counted once) alongside the plain history mean —
  measured effective 0.7431 >= 0.60 (history 0.5611, reported honestly).
  real-ticket-007-v2 closed via the loop with a perfect rerun
  (composite 1.0000, 0 mismatches vs golden): deny-by-default redaction
  (sshpass, cookies, private keys, credential keys, JSON payloads).
  Baseline refreshed to 20 cases; verify ALL GREEN; 142 tests.
- Sandbox attestation (ADR-0009, Phase 6.3): `verify.sh` gains a
  `sandbox` target — skipped locally, mandatory in CI (`CI=true`):
  fails unless the runner is non-root and identifies itself, and the
  evidence line lands in the gate log. The master CI gate (GH Actions,
  running since Phase 5) is now provably sandboxed.
- Work graph (ADR-0008, Yegge "Shape of Things to Come" direction):
  tickets gain optional `blocked_by` deps (frontmatter/JSON/bullet
  forms); `ticket validate-graph` checks dangling edges + cycles;
  `ticket graph` prints the edge set; `ticket claim/release/claims`
  implement lease semantics in `tickets/claims.md` — claim fails when
  another claimant holds the ticket or an open dependency blocks it
  (unless `--force`).
- Yegge essay (Shape of Things to Come Part 1 + Gas Town) ingested into
  canonical memory (domain: strategy): convergent harness shape,
  Beads-ledger primitives, CI/CD pigeonhole collapse, Land Rush,
  agentic review, model welfare. 9 facts.
- `mini-agi run ingest <run.json> [--retro]` — scored runs compound into
  canonical memory (idempotent); retro bullets become facts.
- `mini-agi insights` — compounding report (runs, tokens/cost, per-case
  composite, memory, tickets, journal, capability gaps) wired into the
  gate.
- `mini-agi backlog` — the failure signal IS the roadmap: capability gaps
  become tickets automatically (dedup by case).
- `mini-agi resume` — resume block (brief head + journal tail +
  in-flight checkpoint) for a fresh session.
- MCP tools: run_ingest, insights, backlog, resume (21 tools total).
- Sequoia thesis ingested into canonical memory (domain: strategy);
  ADR-0005 records the direction.
- `mini-agi run failures <run.json>` — failure register (Reflexion,
  Phase 6.1, closes TICKET-9): repeated failing actions (same tool+action,
  >=2 occurrences, at least one with a failure signal) are hashed into
  `memory/derived/failures.md` (deterministic, idempotent); `resume` prints
  the register tail so a fresh session never repeats a recorded failure.
  Detected on the real evals: reactive-loop (edit "edit same line" x2) and
  8 more across real-ticket-001..005.
- Live rerun proof (Phase 6.1, closes TICKET-9): `reactive-loop-rerun`
  case — same task/scope as reactive-loop, executed with the register
  discipline (plan first, tests first, no repeated failing actions);
  composite 0.0 -> 0.7225 on a real scratch project (TS + node:test,
  9/9 tests green).
- `mini-agi eval score` reports per-step tool mismatches
  (`tool_mismatches_detail`: step, run_tool, golden_tool) — additive
  diagnostic (Phase 6.2); mismatch count, D3 and composite unchanged.
- ADR-0006: tool parity compares tool families — `write`/`edit` both
  normalize to `file-modify`. Prospective comparability fix: the harness
  can never emit `edit`, so a future `write` step vs an `edit` golden
  was an unfixable penalty. Measured impact on existing cases: 0/32
  mismatches were pure `write`↔`edit` pairs; baseline refreshed (adds
  only `reactive-loop-rerun`).
- Tool-mismatch loop closure (Phase 6.2): `mini-agi eval mismatches
  [<run>]` records per-step divergences (case, step, run_tool,
  golden_tool) into `memory/derived/mismatches.md` (deterministic,
  idempotent, derived — never hand-edited); `resume` shows the register
  tail so the next session matches the golden step shape. Generated from
  the real evals: 32 divergences across 7 cases.
- The eval gate now fails on tool-mismatch growth: `GateEntry` carries
  `tool_mismatches`, `run_gate` flags `TOOL REGRESSION` beyond
  `--mismatch-tolerance` (default 1); `baseline.json` refreshed with the
  column (old baselines default to 0).
- `eval score`/`eval mismatches` distinguish a missing golden file:
  `cannot read golden file` instead of the misleading run-file error.

### Fixed
- Ticket ids follow the PoC contract exactly (`^TICKET-[0-9]+` via
  re.search): `TICKET-001-v2` accepted, `TICKET-x` rejected; v2 lookup
  via prefix scan, traversal-safe.
- Checkpoint audit: a same-second `BEGIN`/`VERIFY` pair now resolves by
  line order (`ts <=`); the strict `<` flagged a fast begin+verify as
  "VERIFY without BEGIN" (latent; surfaced by gate-fix round).
- Checkpoint rollback re-journals the wiped `BEGIN` before `VERIFY-FAIL`:
  the reset discarded the uncommitted BEGIN line, leaving an orphan
  VERIFY-FAIL the audit could never heal (gate red -> rollback -> new
  orphan = deadlock). The 2026-08-03 journal was repaired by restoring the
  two true BEGIN lines (documented in the journal via STATUS); the script
  now keeps rollbacks paired.
- `checkpoint.sh verify` is a no-op for an already-closed label: re-running
  it journaled a second VERIFY-PASS without an open BEGIN (operational
  duplicate removed from the journal; guard added).

## [0.3.0] — 2026-08-02

### Added
- `mini-agi ticket list|show|validate` — ticket lifecycle (JSON handoffs per
  ADR-0007, markdown frontmatter, PoC bullet format); `ticket_list/show/
  validate` MCP tools (17 tools total).
- MCP dual framing: LSP `Content-Length` (spec) and raw newline-delimited
  JSON (rmcp/codex stdio transport) — fixes the codex handshake timeout.
- LICENSE (MIT), crate READMEs, `cargo package`-clean metadata.
- Live foreign-agent dogfood: codex (gpt-5.6-terra) read the brief and
  wrote enforced facts through the MCP server.

### Fixed
- Eval gate never silently re-baselines: missing baseline is a failure,
  `--write-baseline` is explicit (codex review finding).
- Checkpoint rollback always lands on the last BEGIN checkpoint, discarding
  uncommitted broken edits; `VERIFY-FAIL` journaled after the reset so the
  reset cannot swallow it (ADR-0004, codex review finding).
- MCP tool schemas declare real JSON types; arguments parse numbers/bools
  tolerantly (codex review finding).

## [0.2.0] — 2026-08-02

### Added
- `mini-agi init` — scaffolds a repo (memory layout, embedded gate scripts,
  AGENTS.md, review rubric, opencode.json MCP config).
- `scripts/demo.sh` — the killer demo (init -> fact -> checkpoint -> gates
  -> MCP).
- GitHub Actions CI on the pinned toolchain (1.97.1) running the full gate.
- End-to-end test `cli_full_ticket_run_end_to_end`.
- Metrics: `mini-agi stats` (canonical inventory by domain) and
  `mini-agi budget` (AGENTS chain vs 32KiB cap, skills list vs 2% budget,
  memory leverage) — ports of `PoC` `stats.py`/`budget.py`.

### Fixed
- Checkpoint allowlist covers `opencode.json`/`.gitignore` (init output).
- CLAUDE.md derive template points at `scripts/verify.sh`.

## [0.1.0] — 2026-08-02

### Added
- Kernel (`mini-agi-core`): hash/store/memory (consolidate, signoff,
  derive, provenance), eval engine (4D scoring, golden, baseline, gate —
  PoC 1:1), skills registry (frontmatter, verify hooks, `skill add`),
  checkpoint journal audit (T008: timestamp gate + line-based audit),
  typed contracts (ADR-0007), 15 verifiable skills in `.agents/skills/`.
- CLI: `mem`, `derive`, `provenance`, `eval score|gate`, `skill
  list|show|verify|add`, `checkpoint audit`, `validate`, `stats`, `budget`.
- stdio MCP server (14 tools) + codex/opencode adapters.
- `scripts/verify.sh` (deterministic gate) + `scripts/checkpoint.sh` (ECC).
- Dogfooding: the repo runs on its own kernel (memory, journal, gate).

### Added (Phase 7 — senior-like runtime quality)
- `mini-agi health` — runtime observability: load1 vs cores, memory,
  swap, process zoo (thresholds catch the 2026-08-03 OOM incident class:
  500 agent-browser, load 21), checkpoint journal health (T008
  semantics), stale claims. Verdict OK/WARN/CRITICAL, exit 0/1/2.
- `mini-agi audit` — repo invariants: provenance drift vs brief,
  gate-baseline freshness, working-tree state, the eval gate itself.
- docs/EXPERIMENTS.md — EXP-001: codex exec implemented a loop-style
  slice (green, 8/8 tests, 365s wall); learnings recorded; codex needs
  a trusted repo (.codex/config.toml) or --skip-git-repo-check.
- `mini-agi init` onboarding: writes `.codex/config.toml` (trusted),
  AGENTS.md documents the intelligence layer (resume/insights/loop/
  health/audit), and prints a ready-summary (verify.sh, health, audit,
  resume).
### Added (Phase 7 — EXP-002)
- EXP-002 (docs/EXPERIMENTS.md): codex as a full pipeline participant —
  new gap case `codex-exp-002` → `loop dispatch` (TICKET-10, lease) →
  `codex exec` (trusted repo via `mini-agi init`, no flags) → trajectory
  reconstructed from transcript evidence → `codex-exp-002-rerun`
  composite 1.0000 → `loop verify` CLOSED, lease released. Baseline 22
  cases; gate 0 regressions.
### Added (Phase 8 — verifiable reward layer, ADR-0011)
- `run.json` may declare `verify_command` + `verify_target`; `mini-agi
  run verify <run>` executes the target repo's own gate and reports
  verified / verified-failed / disagrees (judge-calibration signal,
  exit 1) / unverified.
- `loop verify` closes a gap only when the composite reaches the target
  AND the declared verifier passes — self-reported outcomes are no
  longer trusted when a verifier is available.
- Backfilled all 9 rerun cases with their real gates — all 9 verify
  PASS against their scratch repos (executable proof of outcomes).
- ADR-0011 documents the trust boundary (exec only on explicit verify,
  score/gate stay pure).
### Added (Phase 8 — Reflexion upgrade)
- Failure register entries gain `reflection` (verbal self-assessment)
  and `mast` (one of the 14 MAST modes, arXiv 2503.13657; validated
  fail-loud by `run failures`).
- `loop dispatch` injects a "Failure context (Reflexion — do not
  repeat)" section into every slice spec: top-K recorded failures for
  the case with reflections and classifications.
- reactive-loop fixture now carries its reflection + FM-1.3
  classification (the canonical Reflexion example).
### Added (Phase 8 — best-state regression bound + metrics series)
- `loop verify` closes a gap only when composite >= 0.5 AND the
  verifier passes AND the eval gate has ZERO regressions — a slice never
  displaces the frozen suite state (RSIBench-Data discipline).
- Every close appends a row to docs/METRICS.md (date, case, composite,
  tokens) — the published Compounding-Test time series.
### Added (Phase 8 — codex integration, EXP-003)
- `mini-agi codex <spec> <workdir>`: completion protocol
  (`<promise>COMPLETE</promise>` + `<result>` JSON), transcript capture,
  truthful trajectory parsing (exec/write/read with line provenance),
  run.json draft output.
- capture.rs parser validated against the real EXP-002 transcript;
  EXP-003 documented in docs/EXPERIMENTS.md.
### Added (Phase 8 — process supervision)
- `eval steps <run>`: per-step verdicts (step_score) + suspicious flags
  where the step-level signal contradicts the outcome claim — active
  judge-budget selection (Let's Verify Step by Step 2305.20050).
### Added (Phase 8 — memory evolution)
- Fact-linking pass in derived views: facts sharing >= 2 keywords get
  cross-links; the brief lists linked (important) facts first —
  importance learned from cross-referencing, canonical stays append-only
  (A-MEM 2502.12110 semantics, derived-only).
### Added (Phase 8 — harness evolution)
- `mini-agi harness`: versioned spec snapshot (docs/harness/HARNESS-*)
  + ledger row with the frozen-suite gate verdict — pairwise eval over
  the frozen suite, accepted only when green (RHI 2607.15524).
