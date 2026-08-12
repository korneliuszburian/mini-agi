# Goal change journal (long-running self-hardening)

Protocol: docs/LONG-RUNNING-GOALS.md. Append-only. A fresh session reads
the tail + the goal objective + canonical memory before working.

- 2026-08-12 setup: goal created (overnight self-hardening of the
  condensed kernel). Protocol + journal established. Next: resolve the
  codex-review MUST-FIX items (ledger lifecycle, MCP schemas, tests,
  fake-field deletion) in falsifier-first cycles.
- 2026-08-12 c1: MCP tools/list now emits inputSchema (type+properties) for all 14 tools — codex/opencode can call them. 45->46 tests. Next: fake-field deletion (LoopRow.composite/repair_signal, composite_avg, RepairSignal, Run.golden/reflection/mast, regression_tolerance).
