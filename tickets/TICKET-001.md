# Ticket

- id: TICKET-001-v2
- title: Zero-dependency JSON Schema validator + typed handoff schemas (ADR-0007)
- goal (one sentence): Implement the missing ADR-0007 component — a zero-dependency JSON Schema validator (scripts/validate.py) with checked-in schemas for the pipeline's typed handoffs (ticket, spec, review verdict, eval run), proven by tests, so subagent returns are contract-validated on entry.
- domain: agent-harness (cross-domain, uses ingested memory)
- acceptance criteria (verifiable, not prose):
  1. `scripts/validate.py <schema> <document>` validates a JSON document against a JSON Schema subset: `required`, `type` (string/number/integer/boolean/array/object), `properties`, `enum`, `pattern`, `minItems`. Exit 0 = valid; exit 1 = invalid with the exact path of the first violation (e.g. `outcome.achieved: expected boolean, got string`). Invalid JSON document -> exit 1 with a clear message.
  2. `scripts/schemas/handoff-ticket.json`, `handoff-spec.json`, `review-verdict.json`, `eval-run.json` checked in; `make validate-schemas` (new Makefile target) validates every document in `evals/cases/*/run.json` against `eval-run.json` and `tickets/*.md` front-matter-free tickets against `handoff-ticket.json` where parseable (skip non-JSON gracefully with a note).
  3. Tests (tests/test_validate.py) cover: valid/invalid required, wrong type at nested path, enum violation, pattern violation, minItems, invalid JSON input, exit codes. All pass via `make verify`.
  4. `tickets/TICKET-001-gates.md` created with VERBATIM output of `make verify` and `make provenance`.
  5. Final report MUST cite at least one fact ID from memory/canonical (ingested from agentic-core-v1) as the reason for a design choice — the fact about deterministic verification (4cf21e40f7f4e2d2) is expected.
  6. No manual commits (checkpoint.sh auto-commits are permitted — its BEGIN/VERIFY journal is the audit trail).
- soft goal (not a criterion): tokens used < 40,000 (first v2 run; the baseline for the cost curve).
- scope (allowed files/dirs for the IMPLEMENTER): scripts/validate.py, scripts/schemas/*.json, tests/test_validate.py, Makefile (add validate-schemas target + wire into verify), tickets/TICKET-001-gates.md (new)
- expected orchestrator post-run artifacts (listed for the reviewer, NOT implementer edits): memory/episodic/checkpoints.log (via checkpoint.sh), evals/cases/real-ticket-001-v2/run.json (capture-trajectory), evals/results/baseline.json (CI), git commits
- non-goals: no changes to checkpoint-gate.py, checkpoint.sh, AGENTS.md, docs/, evals/golden/, memory/ manual edits. No manual commits.
- dependencies / related canonical facts (fact IDs from memory/canonical/index.md):
  - 4cf21e40f7f4e2d2 verification must be a deterministic gate, not a model declaration
  - 4ff9a0feb3fc77cd subagents are context firewalls: typed handoffs, capped summaries
  - 43a956cb72cedb67 append-only decision logs prevent semantic drift
- blocker to START: none — make verify is GREEN (38 tests). This is the first real v2 run: it validates the whole pipeline end-to-end (ticket -> checkpoint -> verify -> gates evidence -> report).
