# Ticket

- id: TICKET-008-v2
- title: Memory-lifecycle batch — promotion signoff, journal audit, stats coverage (A+B+C experiment: batched ticket, low reasoning, single-read budget, exact measurement)
- goal (one sentence): Land three related memory-lifecycle deliverables in ONE batched ticket under the low-reasoning + context-budget regime and measure EXACTLY whether batching amortizes the fixed pipeline overhead (target: total tokens well under 3 x 126,907, goal < 190,000).
- domain: agent-harness
- context (verified by orchestrator, do not re-derive):
  - EXPERIMENT: this is the decisive cost-curve run. reasoning = low (set at launch). Context-budget rules from TICKET-007-v2 apply unchanged (below). Orchestrator captures output to a log file (tee) — the exact CLI token count WILL be recovered this run; do not rely on it during work.
  - The three deliverables were pre-reviewed for feasibility: consolidate.py currently has NO conflict detection (ADR-0002 gap); scripts/stats.py exists; scripts/checkpoint.sh journals BEGIN/VERIFY/FAIL lines with commit hashes.
  - Batching rule: work the deliverables in order 1->2->3; do NOT start the next until the previous passes `make verify`.
- CONTEXT-BUDGET RULES (binding, AC-verifiable — same as TICKET-007-v2):
  1. Read memory/derived/context-brief.md, AGENTS.md, tickets/TICKET-008.md, and each file you edit AT MOST ONCE. After the first read, use `grep -n <pattern> <file>` for fact lookup.
  2. No speculative reads (evals/, docs/, adr/, knowledge/ only if a failing gate demands).
  3. checkpoint.sh begin/verify once per logical change; no ceremony-only slices.
- acceptance criteria (verifiable, not prose):
  Deliverable 1 — promotion signoff (ADR-0002):
  1.1 `consolidate.py` gains `--require-signoff`: contested candidates (facts that differ from an existing canonical fact only by wording — similarity heuristic: same first 40 chars) are NOT written to canonical; they go to `memory/review/contested-<date>.md` (append-only queue with source + reason + the existing fact hash). Deterministic test: two wording-variants land in the queue, not canonical.
  1.2 `consolidate.py --signoff <queue-file> <fact-index>` promotes ONE contested fact into canonical with `kind: signoff` provenance; promoting it again is a no-op error (already known). Tests for both.
  Deliverable 2 — journal audit:
  2.1 New `scripts/audit.sh` (or .py, your call, must be deterministic): walks memory/episodic/checkpoints.log; reports orphan BEGIN (no VERIFY), VERIFY-FAIL without a subsequent green checkpoint, and VERIFY without BEGIN; exit non-zero if ANY anomaly. Test with a synthetic journal (anomalies + clean case).
  2.2 AUTHOR-AMENDMENT (2026-08-02, resolves the D2 blocker): the real journal contains HISTORICAL orphan BEGINs (aborted runs from earlier today). The audit contract is therefore: (a) the OPEN segment — entries after the newest complete `VERIFY-PASS` — must be anomaly-free: any orphan BEGIN / VERIFY without BEGIN / VERIFY-FAIL without subsequent green checkpoint in the open segment -> exit non-zero; (b) HISTORICAL entries (before the newest complete VERIFY-PASS) are reported as WARNING lines (count + first-5 examples), never exit non-zero. The journal itself is NEVER edited (append-only). Tests: clean journal exit 0; open-segment anomaly exit 2; historical anomaly-only journal exit 0 with warnings.
  2.2b AUTHOR-AMENDMENT #2 (2026-08-02, resolves the verify/audit deadlock): the LAST line of the journal is allowed to be an unpaired `BEGIN` — it is the verification currently in progress (checkpoint.sh verify journals BEGIN before running make verify). Any OTHER unpaired BEGIN in the open segment is an anomaly. This is mechanical (allowline == last journal line), deterministic, and cannot hide a genuine orphan (a real orphan BEGIN is never the literal last line for long; the next verify's BEGIN supersedes it and the orphan becomes non-last -> flagged). Update the audit tests to cover: last-line-unpaired-BEGIN exits 0; non-last unpaired BEGIN in open segment exits 2.
  2.2c AUTHOR-AMENDMENT #3 (2026-08-02, supersedes #2; correct semantics verified against scripts/checkpoint.sh): a `BEGIN <label>` is RESOLVED by either a subsequent `VERIFY-PASS <label>` OR a subsequent `VERIFY-FAIL <label>` line (the FAIL line journals the rollback — the failed verification is closed evidence, not an anomaly). ORPHAN = `BEGIN` with NO following VERIFY-PASS/VERIFY-FAIL line, except when it is the LITERAL LAST journal line (verification in progress — checkpoint.sh begin writes BEGIN first, then make verify runs, then the PASS/FAIL line is appended). This resolves the deadlock without journal edits: the three stale BEGINs from failed audit rounds are each closed by their VERIFY-FAIL lines and must NOT be reported as anomalies. Tests: BEGIN..PASS ok; BEGIN..FAIL ok (not an anomaly); BEGIN..FAIL..PASS ok; bare BEGIN not-last -> anomaly exit 2; bare BEGIN as last line -> ok exit 0; historical anomalies still warning-only.
  2.3 `make audit` target wired; `make verify` runs it (real journal: history warnings allowed, open segment clean by cascade gate).
  Deliverable 3 — stats coverage:
  3.1 `scripts/stats.py` (existing) gets tests: counts of canonical entries/facts, derived views, and gate status line; deterministic against a fixture tree.
  3.2 `make stats` wired if not already; runs clean in CI.
  Common:
  4. `make verify` GREEN (74 existing + new), `make provenance` PASS, `make audit` PASS, `tickets/TICKET-008-gates.md` with VERBATIM output of all four commands.
  5. Final report cites a canonical fact supporting one design choice in deliverable 1 (43a956cb72cedb67 append-only or 4ff9a0feb3fc77cd firewalls expected) and states this ticket's role in the cost-curve experiment.
  6. No manual commits (checkpoint.sh auto-commits permitted).
- soft goal (not a criterion): tokens < 190,000 TOTAL for all three deliverables (the experiment target; 3 x single-ticket overhead would be ~380k, so < 190k shows amortization works; < 126,907 shows the redesign beats single-ticket cost outright).
- scope (allowed files/dirs for the IMPLEMENTER): scripts/consolidate.py, scripts/stats.py, scripts/audit.sh, Makefile, tests/test_consolidate.py, tests/test_stats.py, tests/test_audit.py, tickets/TICKET-008-gates.md, artifacts/TICKET-008-v2/
- expected orchestrator post-run artifacts (NOT implementer edits): memory/episodic/checkpoints.log, memory/review/ (via consolidate), evals/cases/real-ticket-008-v2/run.json, evals/results/baseline.json, git commits
- non-goals: trajectory format changes, redaction changes, checkpoint-gate changes, docs edits; do NOT modify evals/golden/, evals/harness/, docs/, memory/ by hand (memory/review/ writes via consolidate.py are allowed), or historical run.json files.
- dependencies / related canonical facts (fact IDs from memory/canonical/index.md):
  - 43a956cb72cedb67 append-only decision logs prevent semantic drift
  - 4ff9a0feb3fc77cd subagents are context firewalls
- blocker to START: none — make verify is GREEN (74 tests); T007 closed review v2-001 #5.
