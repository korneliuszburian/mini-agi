# Goal change journal (long-running self-hardening)

Protocol: docs/LONG-RUNNING-GOALS.md. Append-only. A fresh session reads
the tail + the goal objective + canonical memory before working.

- 2026-08-12 setup: goal created (overnight self-hardening of the
  condensed kernel). Protocol + journal established. Next: resolve the
  codex-review MUST-FIX items (ledger lifecycle, MCP schemas, tests,
  fake-field deletion) in falsifier-first cycles.
- 2026-08-12 c1: MCP tools/list now emits inputSchema (type+properties) for all 14 tools — codex/opencode can call them. 45->46 tests. Next: fake-field deletion (LoopRow.composite/repair_signal, composite_avg, RepairSignal, Run.golden/reflection/mast, regression_tolerance).
- 2026-08-12 c1b: found+fixed CLI defect — `mem consolidate` passed the buffer PATH as content (no facts ever extracted); now reads the file. 2 facts consolidated to canonical. Next: fake-field deletion.
- 2026-08-12 c2: deleted fake compatibility fields (LoopRow.composite/repair_signal, composite_avg, RepairSignal, Run.golden/reflection/mast, regression_tolerance) + ripple. 46 tests green. Next: focused transition tests for loopcmd verify (gate semantics).
- 2026-08-12 c3: focused loop transition tests (verify closes only on gate pass + achieved; stays open on fail/no-gate; dispatch rejects incomplete gate; status lists open only). 51 tests green. Survived a rollback (pipe-exit trap) — recovered from remote. Next: MUST-FIX 1 (gap ledger lifecycle) or 5 (security seams).
