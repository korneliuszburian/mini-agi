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
