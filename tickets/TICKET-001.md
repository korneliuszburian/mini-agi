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

## Closure evidence (2026-08-11, goal session)

- v2 (PoC, Python) is superseded in v3 (Rust): the ADR-0007 validator lives in the
  kernel as `crates/mini-agi-core/src/contract.rs` — zero-dependency (serde_json only),
  deterministic, and wired into the CLI as `mini-agi validate <contract> <document>` and
  `mini-agi ticket validate <id>` (plus `ticket validate-graph`). The ACC requirements
  below are met by v3 code, not by the removed Python path.
- ACC-1 (valid validation + exact first-violation path): `Schema { validate }`
  (contract.rs:48) enforces `required`, `type` (string/number/integer/boolean/array/object),
  `properties`, `enum`, `pattern`, `minItems`. `validate_contract_value` (contract.rs:211)
  returns `SchemaError` (contract.rs:20) whose `Display` carries the exact dotted path
  (`outcome.achieved: expected boolean, got string`). Invalid JSON → parse error exits 1.
  Live proof: `mini-agi validate eval-run evals/cases/afk-max-idle/run.json`
  → `ok: … run.json validates against eval-run` (exit 0); bad documents and unknown
  contract names covered by `validate_doc_accepts_contract_and_rejects_bad` (main.rs:4118).
- ACC-2 (checked-in schemas + fleet validation): contracts exist for every typed handoff —
  CLI accepts eval-run | ticket | spec | verdict, and `mini-agi ticket validate TICKET-006-v2`
  → `ok: TICKET-006-v2 (…) validates against the ticket contract` (exit 0). Ticket files
  (no front-matter) are validated as ticket-contract documents.
- ACC-3 (tests): contract.rs unit tests cover valid/invalid-required, bad id pattern, empty
  scope (minItems), verdict enum + integer scores, eval-run/spec contracts, integer-vs-bool
  typing, depth-first first-error (`first_error_wins_depth_first`), and repair-until-valid.
  Greened by `cargo test --workspace` = 501 passed (isolated: contract tests pass).
- ACC-4 (gates evidence): `tickets/TICKET-001-gates.md` carries the recorded v2 gate output
  (`verify: ALL GREEN`, `validate-schemas: ok`, provenance PASS). v3 gate set —
  `scripts/verify.sh` (fmt/clippy/tests 501, checkpoint audit, provenance, mem-dedup, stats,
  budget, insights, audit, derive) — ALL GREEN on the clean tree.
- ACC-5 (canonical fact cited): the deterministic-verification fact `4cf21e40f7f4e2d2`
  (verification is a deterministic gate, not a model declaration) is the reason this validator
  is a pure function with zero LLM involvement, matching ADR-0007.
- ACC-6: v2 ran under checkpoint.sh auto-commits only (its BEGIN/VERIFY journal entries are
  in `memory/episodic/checkpoints.log`); the v3 closures likewise go through checkpoint.sh.
- Run status (honest): `evals/cases/real-ticket-001-v2/run.json` predates the ADR-0011
  verifier — it carries no `verify_command`/`verify_target`, so `run verify` reports
  `unverified`/target-missing (the PoC checkout is gone). Its `outcome.achieved: true` is the
  run's OWN claim; the ACC mapping above is verified against the current v3 code instead.
- Commits (closure context): `5b97b55` (T002 falsifier), `62cf42b` (checkpoint close).

Status: CLOSED (evidence above).
