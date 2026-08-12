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
- 2026-08-12 c4: gap ledger lifecycle (MUST-FIX 1) — evals/ledger/<case>.json atomic under claims lock; dispatch=dispatched, terminal blocks redispatch, verify closes BASE row + releases claim + ticket CLOSED in one transaction. 54 tests, verify.sh ALL GREEN (VERIFY-PASS gap-ledger-c4b). Incident: re-BEGIN orphan repaired per INCIDENTS row 7; pipeline-exit trap row 8. Next: MUST-FIX 5 (security seams: resolve verify targets relative to root, cap gate runs).
- 2026-08-12 c5: security seams (MUST-FIX 5) — resolve_target (canonicalize + containment, outside reject unless opt-in), gate via run_capped 120s cap, output truncated 8MiB. 61 tests, VERIFY-PASS security-seams-cycle. Remaining: MUST-FIX 2 init/.codex regeneration from 14-tool registry, MUST-FIX 3 eval::Run/MCP boundary coverage.
- 2026-08-12 c6: MUST-FIX 2 — init/.codex/config.toml regenerated from the 14-tool MCP registry (ToolDef.requires_approval, tool_names/approval_tool_names), stale 39-tool hardcoded list removed. 62 tests, VERIFY-PASS codex-registry-cycle. Remaining: MUST-FIX 3 eval::Run/MCP boundary tests.
- 2026-08-12 c7: MUST-FIX 4 completed (Run.golden/reflection/mast deleted) + MUST-FIX 3 extended (eval::Run boundary tests + MCP dispatch boundary tests: handshake gating, registry-exact tools/list, unknown tool/method, write tools need approve). 70 tests, VERIFY-PASS eval-run-tests-cycle. ALL 5 MUST-FIX RESOLVED.
- 2026-08-12 c8: E2E probe — loop dispatch/verify write+close the ledger per ARCHITECTURE-CONDENSED schema; outside verify_target rejected with resolved path; MCP stdio framing (initialize→tools/list) returns real inputSchema. No defects found.
