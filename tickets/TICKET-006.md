# Ticket

- id: TICKET-006-v2
- title: Consolidation edge-case hardening — cross-entry dedup, cross-day numbering, dry-run, extraction boundaries
- goal (one sentence): Make scripts/consolidate.py deterministic and safe at its boundaries (dedup across ALL canonical entries, per-day numbering without collisions, non-destructive dry-run, strict extraction rules) with regression tests — and produce the first CLEAN cost-curve measurement for the v2 pipeline.
- domain: agent-harness
- context (verified by orchestrator, do not re-derive):
  - consolidate.py exists (100 lines) and runs, but has NO test coverage beyond the TICKET-002-v2 loop test; boundary behavior below is untested or unspecified.
  - existing_fact_hashes() already scans ALL entries (rglob) — cross-entry dedup should hold; needs a test proving it.
  - next_entry_file() numbers per-day from today's dir only; cross-day collision is impossible (date in filename) but numbering-per-day continuity is untested.
- acceptance criteria (verifiable, not prose):
  1. Cross-entry dedup: a fact present in an EARLIER canonical entry (different date dir) is SKIPPED when it appears in a new buffer (dedup is repo-wide, not run-local). Test: pre-seed entry in a previous date dir, feed buffer with the same fact + one new fact -> entry has 1 new fact, skipped=1.
  2. Per-day numbering: with entries 2026-07-31-002.md, 2026-08-01-001.md and none today, a consolidation creates 2026-08-01-002.md? NO — today's date drives the filename; assert the created file is <today>-001.md when today's dir is empty, and <today>-00N+1.md when N entries exist today. Tests for both, including after a previous-date-only state.
  3. `--dry-run`: prints exactly what WOULD be written (entry path, count of new facts, skipped count) and writes NOTHING (no new entry file, no dir creation beyond existing). Test: file count unchanged, output contains expected counts.
  4. Extraction strictness: `FACT: <text>` lines (case-insensitive, leading whitespace ok) are extracted; empty FACT: lines are SKIPPED not crashed; bullet lines are extracted only when stripped length > 8 chars (7-char bullet skipped, 8-char kept); plain prose lines and headers are never extracted. Tests for each boundary.
  5. Robustness: empty buffer (no facts) -> exit 1 with clear message (existing behavior, add test); buffer path missing -> exit 1 clear message; a buffer containing CRLF line endings extracts the same facts as LF (no '\r' leaking into facts).
  6. `make verify` GREEN (68 existing + new tests), `make provenance` PASS, `tickets/TICKET-006-gates.md` with VERBATIM `make verify` + `make provenance` output.
  7. Final report cites fact 43a956cb72cedb67 (append-only decision logs prevent semantic drift) as the reason dedup is repo-wide rather than run-local. Read memory/derived/context-brief.md BEFORE working.
  8. No manual commits (checkpoint.sh auto-commits permitted).
- soft goal (not a criterion): tokens < 106,513 — the T001-v2 baseline. This is the first CLEAN measurement (small scope, no fixes-to-fixes chain, artifacts/ pre-declared): if even this costs >= 106,513, the honest verdict is that pipeline overhead dominates and the memory-driven acceleration claim must be redesigned, not just re-run.
- scope (allowed files/dirs for the IMPLEMENTER): scripts/consolidate.py, tests/test_consolidate.py, tickets/TICKET-006-gates.md, artifacts/TICKET-006-v2/
- expected orchestrator post-run artifacts (NOT implementer edits): memory/episodic/checkpoints.log, evals/cases/real-ticket-006-v2/run.json, evals/results/baseline.json, git commits
- non-goals: ADR-0002 promotion controls, redaction hardening, trajectory format changes, checkpoint-gate changes; do NOT modify evals/golden/, evals/harness/, docs/, memory/ by hand, or historical run.json files.
- dependencies / related canonical facts (fact IDs from memory/canonical/index.md):
  - 43a956cb72cedb67 append-only decision logs prevent semantic drift
- blocker to START: none — make verify is GREEN (68 tests); REVIEW-003-v2 (APPROVE 8/8) is the last committed review.

## Closure evidence (2026-08-11, goal session)

- v2 (PoC, Python) is superseded in v3 (Rust): consolidation lives in
  `crates/mini-agi-core/src/memory.rs` (`consolidate`, `extract_candidates`, canonical
  entry writing). All ACC requirements below are locked by ported tests (same inputs as the
  PoC expectations).
- ACC-1 (cross-entry dedup): `consolidate_skips_facts_known_from_earlier_entries`
  (memory.rs:1351) pre-seeds a PREVIOUS date's entry and asserts the same fact from a new
  buffer is skipped repo-wide (`skipped == 1`, only the new fact lands; duplicate body
  absent from the new entry).
- ACC-2 (per-day numbering): `consolidate_numbers_per_day_continuously` (memory.rs:1409)
  — with only previous-date entries, today starts at `-001`; with N entries today, the next
  is `00N+1` (seq asserted for both transitions).
- ACC-3 (dry-run): `consolidate_dry_run_plans_but_writes_nothing` (memory.rs:1459) —
  reports planned entry + counts (new_facts/skipped) yet records zero `.md` files and
  creates no directories.
- ACC-4 (extraction strictness): `extract_candidates_enforces_boundaries` (memory.rs:1313)
  — FACT:/bullet (empty skipped), bullet >= 8 chars (7 skipped, 8 kept), prose/headers never
  extracted, CRLF does not leak `\r`.
- ACC-5 (robustness): `consolidate_empty_buffer_is_an_error` (memory.rs:1338) — empty/blank
  buffer -> `MemoryError::NoFacts` (CLI exit 1, clear message); missing buffer path handled
  by CLI with a clear error; CRLF covered by ACC-4.
- ACC-6 (gates evidence): `tickets/TICKET-006-gates.md` carries recorded v2 gate output;
  current v3 gate set = `scripts/verify.sh` ALL GREEN on the clean tree (fmt, clippy -D
  warnings, tests 501, checkpoint audit, provenance, mem-dedup, stats, budget, audit, derive).
- ACC-7 (canonical fact): repo-wide dedup is justified by `43a956cb72cedb67` (append-only
  decision logs prevent semantic drift) — dedup must be append-only + global, never
  run-local mutation.
- ACC-8: closures go through checkpoint.sh; v2 journal lines live in checkpoints.log.
- Run status (honest): `real-ticket-006-v2/run.json` predates ADR-0011 — no
  `verify_command`/`verify_target`; `run verify` reports unverified/target-missing (PoC
  checkout gone). ACC mapping verified against current v3 memory.rs tests + `mini-agi`
  docs (`mem consolidate`/`consolidating_the_same_buffer_twice…`).

Status: CLOSED (evidence above).
