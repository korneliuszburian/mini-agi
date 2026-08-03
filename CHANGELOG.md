# Changelog

All notable changes to mini-agi. Format: Keep a Changelog; this project
follows semantic versioning (workspace `Cargo.toml` `version`).

## [Unreleased]

### Added (intelligence layer, ADR-0005 — Sequoia "From Hierarchy to
Intelligence" direction)
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
