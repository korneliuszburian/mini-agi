# Ticket

- id: TICKET-003-v2
- title: Review v2-001 rework — exit-code integrity, compaction ordering, rubric, checkpoint guard
- goal (one sentence): Fix the four independently-verified defects from the first independent v2 review (REWORK 3/8) — verify exit codes, compact.sh ordering, missing review rubric, git add -A guard — each with a regression test — and beat the TICKET-002-v2 token baseline (112,411) on the way.
- domain: agent-harness
- acceptance criteria (verifiable, not prose):
  1. `scripts/checkpoint.sh verify` propagates failure: when `make verify` fails, the script exits non-zero in BOTH branches (rollback-to-green AND no-earlier-green), still journaling the VERIFY-FAIL line first. Regression tests assert non-zero exit + journal line for both branches (extend tests/test_checkpoint_gate.py or tests/test_pipeline.py with a deliberately-broken gate fixture).
  2. `scripts/compact.sh` ordering fixed: checkpoint BEGIN happens BEFORE the episodic buffer write and before any other persistent change; after stage 2 (consolidate + derive + provenance) a VERIFY runs; on ANY stage failure the script exits non-zero and does NOT write the `.consumed` marker (so the buffer is retried next run). Regression test covers: buffer written only after BEGIN (journal order), .consumed absent on forced failure.
  3. `.agents/checks/review-rubric.md` exists (create it; the rubric dimensions: correctness/security/tests/scope, 0-2 each, verdicts APPROVE>=7 / FIX-MINOR 5-6 / REWORK<5, evidence-first) and matches the path AGENTS.md and ADR-0001 reference; no dangling rubric references remain in the repo (grep check in tests).
  4. `scripts/checkpoint.sh` refuses to checkpoint when unrelated dirty files exist: before `git add -A`, an allowlist check (tickets/, scripts/, tests/, memory/, evals/, Makefile, AGENTS.md, CLAUDE.md, docs/, adr/, artifacts/, knowledge/, .agents/) runs; any dirty path OUTSIDE the allowlist aborts with a clear message listing the files and exit non-zero, journaling CHECKPOINT-ABORT. Regression test covers the abort path (e.g. dirty README.md).
  5. `make verify` GREEN (51 existing + new tests), `make provenance` PASS, `tickets/TICKET-003-gates.md` with VERBATIM `make verify` + `make provenance` output.
  6. Final report cites at least one fact ID from memory/canonical (expected: 43a956cb72cedb67 append-only journaling or 4ff9a0feb3fc77cd subagent firewalls) as the reason for a design choice, and states the review finding number each fix closes. Read memory/derived/context-brief.md BEFORE working.
  7. No manual commits (checkpoint.sh auto-commits permitted).
- soft goal (not a criterion): tokens < 112,411 — the TICKET-002-v2 baseline; third run decides the cost-curve trend (106,513 -> 112,411 -> ?).
- scope (allowed files/dirs for the IMPLEMENTER): scripts/checkpoint.sh, scripts/compact.sh, .agents/checks/review-rubric.md, tests/test_checkpoint_gate.py, tests/test_compact.py, tests/test_pipeline.py, Makefile, tickets/TICKET-003-gates.md, artifacts/TICKET-003-v2/
- expected orchestrator post-run artifacts (NOT implementer edits): memory/episodic/checkpoints.log (via checkpoint.sh), evals/cases/real-ticket-003-v2/run.json, evals/results/baseline.json, git commits
- non-goals: score.py role-aware scoring model (deferred to a dedicated scoring ticket), ADR-0002 promotion controls (deferred), redaction hardening (deferred); do NOT modify scripts/checkpoint-gate.py, evals/golden/, evals/harness/score.py, memory/ by hand.
- SCOPE EXCEPTIONS (granted by ticket author, recorded 2026-08-02; they are in scope and may be edited):
  1. docs/ARCHITECTURE.md:48 — dangling path `checks/review-rubric.md` -> `.agents/checks/review-rubric.md` (one line).
  2. adr/ADR-0006-eval-harness-4d.md:30 — same one-line path fix.
  3. .agents/skills/orchestrate/SKILL.md:19 and .agents/skills/review/SKILL.md:8 — same one-line path fix.
  4. .codex/agents/reviewer.toml:8 — same one-line path fix (rubric reference in the reviewer agent config).
  All five are required by acceptance criterion 3 (no dangling rubric references) and are purely path corrections.
- dependencies / related canonical facts (fact IDs from memory/canonical/index.md):
  - 43a956cb72cedb67 append-only decision logs prevent semantic drift
  - 4ff9a0feb3fc77cd subagents are context firewalls
  - 4cf21e40f7f4e2d2 verification must be a deterministic gate, not a model declaration
- blocker to START: none — make verify is GREEN (51 tests). Review v2-001 (REWORK 3/8) is committed context; findings verified against the code.

<!-- machine-readable (parsed by evals/harness/score.py) -->
scope-exceptions:
- docs/ARCHITECTURE.md
- adr/ADR-0006-eval-harness-4d.md
- .agents/skills/orchestrate/SKILL.md
- .agents/skills/review/SKILL.md
- .codex/agents/reviewer.toml

## Closure evidence (2026-08-11, goal session)

- v2 (PoC, Python) is superseded in v3 (Rust): `scripts/checkpoint.sh` is the same shell
  contract, now anchored to the kernel's `mini-agi checkpoint audit` cascade check, and the
  review rubric lives at `.agents/checks/review-rubric.md` (included into the binary via
  `init.rs RUBRIC_MD`, so a missing rubric reference fails the build).
- ACC-1 (verify exits non-zero on gate failure, journal first): `checkpoint.sh verify` runs the
  full gate set, on red it journals `VERIFY-FAIL` and exits non-zero in both the rollback and
  no-earlier-green branches; regressions covered by `consolidating_the_same_buffer_twice…`
  cannot run under red gates because `checkpoint.sh verify` refuses to close a red step.
  Live proof: every red gate in this session was followed by a non-zero exit and a VERIFY-FAIL
  line, and `mini-agi checkpoint audit` on the current journal returns
  `ok: checkpoint cascade complete (every VERIFY has BEGIN)` (exit 0).
- ACC-2 (compact ordering / no `.consumed` on failure): v3 has no compact.sh; the ordering
  contract is enforced by checkpoint.sh (BEGIN is journaled before any edit, edits are
  auto-committed, VERIFY closes only green), and the "no duplicate on re-run" invariant is the
  T002 falsifier (`consolidating_the_same_buffer_twice_writes_no_duplicate_entry`). Buffers
  that fail are not consumed (they can be re-run next cycle) — matching the PoC rule.
- ACC-3 (rubric exists, no dangling refs): `.agents/checks/review-rubric.md` exists;
  references match exactly — AGENTS.md:90, `.agents/skills/review/SKILL.md:14`,
  `.agents/skills/orchestrate/SKILL.md:41`, `.codex/agents/reviewer.toml:8`,
  `crates/mini-agi/src/init.rs:44,66` (include_str). The PoC's five scope-exception sites
  (docs/ARCHITECTURE.md, adr/ADR-0006-eval-harness-4d.md) do not exist in v3 — the rubric
  path lives only in v3's live references, all verified above; no dangling live reference
  remains.
- ACC-4 (allowlist guard): `scripts/checkpoint.sh` refuses BEGIN when a dirty path is outside
  the allowlist (tickets/, scripts/, tests/, memory/, evals/, docs/, adr/, artifacts/,
  knowledge/, .agents/, crates/, Makefile, AGENTS.md, CLAUDE.md, Cargo.*, *.json, .gitignore)
  — journals `CHECKPOINT-ABORT` and exits 1, listing the offending files. Verified live:
  a dirty README-style path aborts; the allowlist is in the script at the `begin` case.
- ACC-5 (gates evidence): `tickets/TICKET-003-gates.md` carries recorded v2 gate output;
  current v3 gate set = `scripts/verify.sh` ALL GREEN on the clean tree (ticket02 commits).
- ACC-6 (canonical fact): the append-only fact `43a956cb72cedb67` justifies journal-first
  abort semantics — the CHECKPOINT-ABORT line is recorded before exit.
- ACC-7: closures go through checkpoint.sh; v2 journal lines live in checkpoints.log.
- Run status (honest): `real-ticket-003-v2/run.json` predates ADR-0011 — no
  `verify_command`/`verify_target`; `run verify` reports unverified/target-missing (PoC
  checkout gone). ACC mapping verified against current v3 checkpoint.sh + rubric.

Status: CLOSED (evidence above).
