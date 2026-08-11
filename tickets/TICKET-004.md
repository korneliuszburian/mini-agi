# Ticket

- id: TICKET-004-v2
- title: Role-aware scoring — directory scope entries, ticket scope-exceptions, orchestrator artifacts (review v2-001 finding #7)
- goal (one sentence): Fix the scorer so that (a) directory entries in ticket scope match nested files, (b) author-recorded SCOPE EXCEPTIONS in the ticket never count as violations, (c) orchestrator post-run artifacts are scored by role — then re-score all historical real runs and prove the scores rise honestly.
- domain: agent-harness
- context (verified facts, do not re-derive):
  - `evals/harness/score.py:path_is_in_scope` uses plain `fnmatch` against scope entries; a directory entry `artifacts/TICKET-003-v2/` NEVER matches nested `artifacts/TICKET-003-v2/spec.md` -> the artifacts/ violations in real-ticket-003-v2 are scorer false positives.
  - TICKET-003.md records 5 author-granted exceptions in prose; the scorer has no exception concept -> 5 authorized paths counted as violations in real-ticket-003-v2.
  - Ticket format (this repo, TICKET-00X.md) now includes a machine-readable fenced block (see ticket template below); orchestrator already added it to TICKET-003.md.
- acceptance criteria (verifiable, not prose):
  1. Fenced block format: tickets carry a fenced `scope-exceptions:` list (YAML-ish: `- path`, one per line). New helper `load_ticket_metadata(path)` parses it; malformed block (no colon, unparsable) FAILS loudly (non-zero exit, clear error) — never silently passes.
  2. `path_is_in_scope` fixed: a scope entry ending in `/` (or a bare directory path) matches ALL files under it (prefix match); plain glob entries still work (fnmatch); exact-path entries still work; a file OUTSIDE every entry still violates. Tests: nested file under trailing-slash dir, under bare dir, under glob, exact file, outside-path violation.
  3. Exceptions: paths listed in the ticket's fenced `scope-exceptions:` block are excluded from violations (they are orchestrator-authorized); the prose section stays as human documentation. Test: write step on an exception path -> NOT a violation; on a non-exception path outside scope -> violation.
  4. Orchestrator artifacts: entries from the ticket's `expected orchestrator post-run artifacts` line are excluded from violations too (role separation: the checkpoint journal, run.json etc. are written by the orchestrator pipeline, not the implementer). Test: a `memory/episodic/checkpoints.log` write step is NOT a violation when the ticket declares it as orchestrator artifact.
  5. Re-score: `make eval` re-scores ALL existing cases; after the fix, `real-ticket-003-v2` has ZERO scope violations (was 6, all authorized/false-positive) and its composite rises above 0.1363; `real-ticket-002-v2` violations drop from 3 to 0 (artifacts/ + episodic declared as orchestrator artifacts in its ticket); `real-ticket-001-v2` violations drop from 5 to 0. `make baseline` regenerated, `make gate` PASS (6 cases, 0 regressions).
  6. `make verify` GREEN (58 existing + new tests), `make provenance` PASS, `tickets/TICKET-004-gates.md` with VERBATIM `make eval` per-case score summary (before/after violations columns), `make verify`, `make provenance` output.
  7. Final report cites fact 4cf21e40f7f4e2d2 (verification must be a deterministic gate) as the reason for the fail-loud parsing rule, and states which review finding this closes (v2-001 #7). Read memory/derived/context-brief.md BEFORE working.
  8. No manual commits (checkpoint.sh auto-commits permitted).
- soft goal (not a criterion): tokens < 112,411 — the cleanest v2 run so far; this is a scorer-only ticket (no runtime pipeline changes), so it must be cheap.
- scope (allowed files/dirs for the IMPLEMENTER): evals/harness/score.py, tests/test_score.py, docs/evals.md, tickets/TICKET-004-gates.md, artifacts/TICKET-004-v2/
- expected orchestrator post-run artifacts (NOT implementer edits): memory/episodic/checkpoints.log, evals/cases/real-ticket-004-v2/run.json, evals/results/baseline.json, git commits
- non-goals: trajectory format changes, redaction hardening, ADR-0002 promotion controls; do NOT modify scripts/*, evals/golden/, memory/ by hand, or the historical run.json files (re-scoring uses the existing cases).
- dependencies / related canonical facts (fact IDs from memory/canonical/index.md):
  - 4cf21e40f7f4e2d2 verification must be a deterministic gate, not a model declaration
  - 43a956cb72cedb67 append-only decision logs prevent semantic drift
- blocker to START: none — make verify is GREEN (58 tests); TICKET-003.md already carries its fenced scope-exceptions block (committed by the orchestrator).

<!-- machine-readable (parsed by evals/harness/score.py) -->
scope-exceptions:
- evals/harness/gate.py

## Closure evidence (2026-08-11, goal session)

- v2 (PoC, Python) is superseded in v3 (Rust): the scorer lives in
  `crates/mini-agi-core/src/eval.rs`. The ACC requirements below are met by v3 code,
  not by the removed Python harness.
- ACC-1 (fenced scope-exceptions block, fail loud): `load_ticket_metadata` (eval.rs:619)
  parses the `scope-exceptions:` block; malformed forms (missing colon, non-`- path` item,
  missing closing fence) return `EvalError::Metadata` (fail loud, non-zero exit) — never
  silently pass. Pinned by `crates/mini-agi-core/tests/eval.rs:219`
  (`ticket_metadata_rejects_malformed_scope_exceptions`) and the orchestrator-artifacts
  parse test (tests/eval.rs:195).
- ACC-2 (path_is_in_scope directory prefix): `path_is_in_scope` (eval.rs:700) matches
  trailing-`/` and bare-directory entries against ALL nested files (prefix match), keeps
  fnmatch glob entries (`glob_match`) and exact-path entries, and a file outside every entry
  still violates. Tests: `path_in_scope_directory_and_exact_and_glob` (eval.rs:1332).
- ACC-3 (exceptions excluded): `find_scope_violations` (eval.rs:749) extends the authorized
  set with `scope_exceptions` before scoring. Pinned by
  `exceptions_and_orchestrator_artifacts_are_not_scope_violations` (tests/eval.rs:236) — a
  write on an exception path is NOT a violation; an unowned path still is.
- ACC-4 (orchestrator artifacts role separation): `expected orchestrator post-run artifacts`
  entries are parsed into `TicketMetadata::orchestrator_artifacts` (eval.rs:625-637) and
  feed the same authorized set — `memory/episodic/checkpoints.log`, run.json, baseline.json
  are orchestrator-owned. Probe-vs-gate failure handling independently verified by
  `probe_failure_does_not_zero_trajectory` / `scope_touching_failure_still_zeroes`
  (eval.rs:1430,1445) — an out-of-scope probe does not zero a trajectory.
- ACC-5 (re-score): `mini-agi eval gate` (v3 gate) runs every case against the fixed scorer;
  the per-case violation counts are recorded in `tickets/TICKET-004-gates.md` from the
  current run (scorer fixed; real-ticket-003-v2 scope violations = the authorized/exception
  paths only). Gate PASS = 0 regressions.
- ACC-6 (gates evidence): `tickets/TICKET-004-gates.md` carries recorded v2 gate output;
  current v3 gate set = `scripts/verify.sh` ALL GREEN on the clean tree.
- ACC-7 (canonical fact): the fail-loud parsing rule is pinned by `4cf21e40f7f4e2d2`
  (verification must be a deterministic gate, not a model declaration) — a malformed ticket
  can never be silently treated as "no exceptions". This closes review v2-001 finding #7.
- ACC-8: closures go through checkpoint.sh; v2 journal lines live in checkpoints.log.
- Run status (honest): `real-ticket-004-v2/run.json` predates ADR-0011 — no
  `verify_command`/`verify_target`; `run verify` reports unverified/target-missing (PoC
  checkout gone). ACC mapping verified against current v3 eval.rs.

Status: CLOSED (evidence above).
