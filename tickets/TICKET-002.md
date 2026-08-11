# Ticket

- id: TICKET-002-v2
- title: Two-stage compaction loop — consolidate + compact end-to-end tests (ADR-0008)
- goal (one sentence): Prove the two-stage compaction loop (checkpoint -> episodic buffer -> consolidation -> canonical -> derived -> provenance drift gate) works end-to-end with deterministic tests, closing the only memory-lifecycle component without coverage — and beat the TICKET-001-v2 token baseline (106,513) on the way.
- domain: agent-harness (cross-domain, uses ingested memory)
- acceptance criteria (verifiable, not prose):
  1. `tests/test_consolidate.py` (new) covers: FACT:/bullet extraction; the SAME fact in two episodic buffers lands ONCE in canonical (dedup by hash); provenance fields present (date, source, domain, kind: consolidation); entry numbering continues (003 after 002); empty buffer -> error exit 1. All pass.
  2. `tests/test_compact.sh`-equivalent (new `tests/test_compact.py` running compact.sh in a temp git repo) covers the FULL loop: checkpoint.sh begin journals; buffer file with facts; `make compact`-equivalent (scripts/compact.sh <buffer>) appends to episodic, consolidates into canonical (new entry), regenerates derived, and `make provenance` PASSES on the temp repo. `.consumed` marker prevents re-consolidation on the second run (no duplicate entry).
  3. Any defect found in compact.sh/consolidate.py while writing tests is FIXED in place (they are in scope); tests must pass with the fixed code.
  4. `make verify` GREEN (existing 45 + new tests), `make provenance` PASS in THIS repo (untouched by temp-repo runs).
  5. `tickets/TICKET-002-gates.md` created with VERBATIM output of `make verify` and `make provenance`.
  6. Final report MUST cite at least one fact ID from memory/canonical (ingested from agentic-core-v1) as the reason for a design choice — the append-only fact (43a956cb72cedb67) or the deterministic-verification fact (4cf21e40f7f4e2d2) is expected. Read memory/derived/context-brief.md BEFORE working (doctrine: knowledge given once must not be re-researched).
  7. No manual commits (checkpoint.sh auto-commits permitted).
- soft goal (not a criterion): tokens used < 106,513 — the TICKET-001-v2 baseline; this run measures the memory-enabled cost curve.
- scope (allowed files/dirs for the IMPLEMENTER): scripts/consolidate.py, scripts/compact.sh, tests/test_consolidate.py, tests/test_compact.py, Makefile, tickets/TICKET-002-gates.md
- expected orchestrator post-run artifacts (NOT implementer edits): memory/episodic/checkpoints.log (via checkpoint.sh), evals/cases/real-ticket-002-v2/run.json, evals/results/baseline.json, git commits
- non-goals: no changes to checkpoint-gate.py, AGENTS.md, docs/, evals/golden/, evals/harness/, memory/ manual edits (only via checkpoint.sh and compact.sh). No manual commits.
- dependencies / related canonical facts (fact IDs from memory/canonical/index.md):
  - 43a956cb72cedb67 append-only decision logs prevent semantic drift
  - 4cf21e40f7f4e2d2 verification must be a deterministic gate, not a model declaration
  - 4ff9a0feb3fc77cd subagents are context firewalls
- blocker to START: none — make verify is GREEN (45 tests). TICKET-001-v2 (first real v2 run) established the token baseline this ticket must beat.

## Closure evidence (2026-08-11, goal session)

- v2 (PoC, Python) is superseded in v3 (Rust): the compaction loop is the kernel
  `crates/mini-agi-core/src/memory.rs::consolidate` (memory.rs:795), exposed on the CLI as
  `mem consolidate` (`cmd_consolidate`, main.rs:2361) with `--dry-run` / `--require-signoff`
  and the `mem signoff` promote command. The loop (checkpoint BEGIN, episodic buffer write,
  consolidate into canonical, re-derive views, provenance gate) is exactly what
  `scripts/verify.sh` runs on the repo itself (checkpoint, provenance, derive, mem-dedup
  steps) and what goal sessions journal in `memory/episodic/checkpoints.log`.
- ACC-1 (extraction/dedup/provenance/numbering/empty): memory.rs tests —
  `extract_candidates_enforces_boundaries` (FACT:/bullet rules, empty-FACT skip, CRLF),
  `consolidate_skips_facts_known_from_earlier_entries` (same fact in two buffers lands ONCE,
  repo-wide hash dedup, skip count asserted), `consolidate_numbers_per_day_continuously`
  (001 after previous-date history, 00N+1 continuation), `consolidate_empty_buffer_is_an_error`.
  Provenance (date/source/domain/kind) is written into every entry by `consolidate`.
- ACC-2 (full loop + `.consumed` guard): NEW v3 falsifier
  `consolidating_the_same_buffer_twice_writes_no_duplicate_entry` (memory.rs:1381) — a second
  `consolidate()` over the SAME buffer lands `new_facts == 0` and writes NO new entry
  (`entry: None`, canonical unchanged) — the v3 analogue of the PoC `.consumed` marker.
  Committed as `5b97b55` (workspace 501 green).
- ACC-3: defects found by the v2 run are fixed in v3 (boundary tests + re-consolidation guard).
- ACC-4: `cargo test --workspace` = 501 passed; `./scripts/verify.sh` = ALL GREEN on the clean
  tree (fmt-check, clippy -D warnings, tests, checkpoint audit, provenance, mem-dedup, stats,
  budget, insights, audit, derive).
- ACC-5 (gates evidence): `tickets/TICKET-002-gates.md` carries the recorded v2 gate output
  (`verify: ALL GREEN`, provenance PASS); the v3 gate set above is the current evidence.
- ACC-6 (canonical fact cited): the append-only fact `43a956cb72cedb67` (append-only decision
  logs prevent semantic drift) is why consolidation appends numbered dated entries instead of
  mutating existing ones — verified in `consolidate`'s write path.
- ACC-7: closures go through checkpoint.sh; v2 journal entries live in checkpoints.log.
- Run status (honest): `real-ticket-002-v2/run.json` predates ADR-0011 — no
  `verify_command`/`verify_target`, so `run verify` reports unverified/target-missing (PoC
  checkout gone). `outcome.achieved: true` is the run's own claim; ACC mapping above is
  verified against current v3 code.

Status: CLOSED (evidence above).
