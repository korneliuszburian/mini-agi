# Experiments (Phase 7)

Record of measured experiments. Every entry: question, setup, result,
learnings, next steps. Evidence over claims; a failed experiment is as
valuable as a passed one.

## EXP-001 — codex as a pipeline implementer (2026-08-03)

- Question: can `codex exec` (gpt-5.6-terra, max reasoning) implement a
  loop-style slice with senior-like quality, measured the way mini-agi
  measures runs (green gates, tests-first, discipline)?
- Setup: scratch repo `/tmp/opencode/codex-exp` — the same baseline as
  the flailing/reactive-loop rate-limit task (existing `src/auth.ts` +
  auth-only tests). Prompt = the slice spec (5 req/60s window, 429 +
  retry-after, per-token isolation, `resetRateLimits`, tests first, tsc
  + node --test green). Sandbox: workspace-write, `--skip-git-repo-check`
  (codex refuses non-trusted dirs without it).
- Result: exit 0, wall 365s. Independent verification (not codex's own
  report): `npx tsc` clean; `node --test` 8/8 pass; diff = 2 files, +90
  insertions; `src/auth.ts` contains 429/retry-after/RATE_LIMIT/
  resetRateLimits; tests cover 5 allowed, 6th 429, isolation, window
  reset (fake-clock variant).
- Learnings:
  1. Codex delivered the same outcome class as the loop's own reruns
     (all-green, tests-first, exports for isolation) without knowing the
     pipeline — the spec alone carried the discipline.
  2. Time: 6m05s wall vs the loop's in-session reruns (~2-4 min); codex
     runs are a batch transport, not a loop participant — the loop's
     lease/work-graph machinery maps 1:1 onto it (dispatch spec →
     codex exec → verify).
  3. codex's own terminal report was accurate here (claimed tsc+test
     green; verified true). Still: verify, never trust the report
     (ADR-0003 mindset).
  4. Sandbox: workspace-write sufficed; cleanup of `dist/` was blocked
     by policy — generated output lingers unless the pipeline owns
     cleanup.
- Next steps: EXP-002 — run codex on a REAL loop-dispatched slice
  (spec from `mini-agi loop dispatch`) and capture a truthy trajectory
  for ingestion; measure tokens/cost via codex session logs.
