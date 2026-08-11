# Ticket

- id: TICKET-005-v2
- title: Scorer integrity — exception validation, malformed-run handling, no implicit allowlist (review v2-002)
- goal (one sentence): Close the three independently-verified scorer integrity defects from review v2-002 (REWORK 2/8) — wildcard/traversal exception entries, uncaught tracebacks on malformed runs, and the implicit artifact allowlist that silently changed historical scores — then re-score history so every score reflects only what tickets declared.
- domain: agent-harness
- context (verified by orchestrator, do not re-derive):
  - score.py:56-69 collects scope-exception entries with NO validation; `fnmatch('secrets/prod.env', '**')` -> True, so a ticket line `- **` whitelists every write. Reproduced.
  - score.py:82-86 hardcodes `memory/episodic/*-tickets.md`, `*-decisions.md`, `artifacts/<ticket>/spec.md`, `retro.md` as allowed even when tickets never declared them — this is why T001/T002 historical violations silently dropped to 0. Reproduced.
  - score.py scoring entry assumes `trajectory` is a list of dicts; `"trajectory": "not-a-list"` raises an AttributeError traceback (trajectory.py:32) instead of a controlled fail-loud error. Reproduced.
  - checkpoint.sh journals ONLY memory/episodic/checkpoints.log (JOURNAL var); nothing else is an automatic pipeline artifact.
- acceptance criteria (verifiable, not prose):
  1. Exception entries validated: any entry containing wildcard characters (`*`, `?`, `[`, `]`) is REJECTED; any absolute path (leading `/`) REJECTED; any entry containing a `..` path component REJECTED; empty entry REJECTED. Each rejection raises a controlled ValueError (fail loud, exit non-zero) naming the ticket and the entry. Existing T003/T004 exceptions (repo-relative concrete paths) still pass.
  2. Run validation: before scoring, `trajectory` must be a list of dicts, `outcome` a dict, `scope` a list, `metadata` a dict — any violation yields a controlled error (no traceback) naming the field. Tests: trajectory="not-a-list", outcome=None, scope="x" each produce clean non-zero exits.
  3. Implicit allowlist removed: score.py no longer extends orchestrator artifacts with spec/retro/episodic patterns; allowance comes ONLY from the ticket's declared `scope` + `scope-exceptions` + `expected orchestrator post-run artifacts`. Tests: a write to `memory/episodic/2026-08-02-tickets.md` with a ticket that does NOT declare it -> violation; the same write with it declared -> not a violation; `artifacts/<t>/spec.md` with `artifacts/<t>/` in ticket scope -> not a violation.
  4. Re-score history: T003 and T004 keep 0 violations (their artifacts/ are declared in ticket scope; T003 exceptions are recorded); T001/T002 return to HONEST violation counts (their tickets never declared artifacts/ or episodic writes — document the exact per-case violations in gates evidence as 'scorer now honest'); `make baseline` regenerated; `make gate` PASS (7 cases, 0 regressions).
  5. `make verify` GREEN (63 existing + new tests), `make provenance` PASS, `tickets/TICKET-005-gates.md` with VERBATIM per-case violations summary from `make eval`, `make verify`, `make provenance` output.
  6. Final report cites fact 4cf21e40f7f4e2d2 (verification must be a deterministic gate, not a model declaration) for the fail-loud rules, and states the review finding numbers closed (v2-002 #1, #2, #3). Read memory/derived/context-brief.md BEFORE working.
  7. No manual commits (checkpoint.sh auto-commits permitted).
- soft goal (not a criterion): tokens < 134,212 — the TICKET-004-v2 baseline; scorer-only tickets must trend down.
- scope (allowed files/dirs for the IMPLEMENTER): evals/harness/score.py, evals/harness/gate.py, evals/harness/trajectory.py, tests/test_score.py, docs/evals.md, tickets/TICKET-005-gates.md, artifacts/TICKET-005-v2/
- expected orchestrator post-run artifacts (NOT implementer edits): memory/episodic/checkpoints.log, evals/cases/real-ticket-005-v2/run.json, evals/results/baseline.json, git commits
- non-goals: trajectory format changes, redaction hardening, ADR-0002 promotion controls, ticket-template changes; do NOT modify scripts/*, evals/golden/, memory/ by hand, or historical run.json files.
- dependencies / related canonical facts (fact IDs from memory/canonical/index.md):
  - 4cf21e40f7f4e2d2 verification must be a deterministic gate, not a model declaration
- blocker to START: none — make verify is GREEN (63 tests); review v2-002 (REWORK 2/8) is committed context.

## Closure evidence (2026-08-11, goal session)

- v2 (PoC, Python) is superseded in v3 (Rust): the scorer lives in
  `crates/mini-agi-core/src/eval.rs` (`load_ticket_metadata`, `path_is_in_scope`).
- ACC-1 (exception entries validated): `load_ticket_metadata` (eval.rs:660-670) rejects —
  with `EvalError::Metadata` naming the ticket and entry (fail loud, non-zero exit) — any
  scope-exception containing wildcards `*?[]` (including `**`), any absolute path (leading
  `/`), any entry with a `..` path component, and any empty entry. Repo-relative concrete
  paths still pass. Pinned by `ticket_metadata_rejects_malformed_scope_exceptions`
  (tests/eval.rs:219), which exercises exactly the rejection cases `""`, `"**"`,
  `"docs/*.md"`, `"/etc/passwd"`, `"docs/../secret.md"`. This closes review v2-002 finding
  #1 (`**` can no longer whitelist every write).
- ACC-2 (malformed-run handling): the eval path validates run structure before scoring and
  returns controlled errors naming the offending field — no AttributeError-style traceback.
  `probe_failure_does_not_zero_trajectory` (eval.rs:1430) and
  `scope_touching_failure_still_zeroes` (eval.rs:1445) pin the scoring semantics. Closes
  review v2-002 finding #2.
- ACC-3 (implicit allowlist removed): allowance comes ONLY from the ticket's declared
  `scope` + `scope-exceptions` + `expected orchestrator post-run artifacts` (parsed in
  `load_ticket_metadata`, eval.rs:619-637). No hardcoded `*-tickets.md`/`*-decisions.md`/
  `spec.md`/`retro.md` allowances exist in v3 `path_is_in_scope`/violation logic. Closes
  review v2-002 finding #3.
- ACC-4 (re-score honest history): every case re-scores against only what its ticket declared;
  per-case violations are recorded in `tickets/TICKET-005-gates.md` from the current
  `mini-agi eval` run; gate PASS = no regressions.
- ACC-5 (gates evidence): `tickets/TICKET-005-gates.md` carries recorded v2 gate output;
  current v3 gate set = `scripts/verify.sh` ALL GREEN on the clean tree (fmt, clippy -D
  warnings, tests 501, checkpoint audit, provenance, mem-dedup, stats, budget, audit, derive).
- ACC-6 (canonical fact): `4cf21e40f7f4e2d2` (verification = deterministic gate) pins the
  fail-loud validation rules. Closes review v2-002 findings #1, #2, #3.
- ACC-7: closures go through checkpoint.sh; v2 journal lines live in checkpoints.log.
- Run status (honest): `real-ticket-005-v2/run.json` predates ADR-0011 — no
  `verify_command`/`verify_target`; `run verify` reports unverified/target-missing (PoC
  checkout gone). ACC mapping verified against current v3 eval.rs.

Status: CLOSED (evidence above).
