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

## EXP-002 — codex in the loop: dispatch → codex → rerun → verify (2026-08-03)

- Question: does the full pipeline close a gap when CODEX is the
  implementer — spec from `mini-agi loop dispatch`, `codex exec` as the
  worker, trajectory reconstructed from observed evidence, `loop verify`
  as the judge?
- Setup: new eval case `codex-exp-002` (open gap: checksum CLI task
  defined, outcome achieved=false, composite 0.0) → `mini-agi loop
  dispatch codex-exp-002` created TICKET-10 + claimed it + wrote the
  slice spec. Scratch repo onboarded with `mini-agi init` (dogfood: the
  generated `.codex/config.toml` made it trusted — `codex exec` ran
  WITHOUT `--skip-git-repo-check`). Prompt = the spec verbatim.
- Result: `codex exec` exit 0, wall 348s. Independently verified:
  `make verify` ALL GREEN (3 unittest cases + fixture digest check),
  checksum.py handles usage/FileNotFound/IsADirectory/OSError, tests
  first, Makefile target wired into verify. Rerun trajectory
  reconstructed from the transcript (11 steps, each traceable to a log
  line or the final worktree) → `codex-exp-002-rerun` composite
  **1.0000** (0 mismatches, no golden) → `loop verify` CLOSED, lease on
  TICKET-10 released, gate 0 regressions across 22 cases.
- Learnings:
  1. The loop is implementer-agnostic: dispatch+spec+verify ran codex
     exactly as it runs an in-session agent; the lease and registers
     protected the flow (claim released only at the target).
  2. `mini-agi init` onboarding made codex trusted — one command, no
     flags; the earlier `--skip-git-repo-check` workaround is obsolete.
  3. Codex iterated like a human: 2 failed `make verify` rounds before
     green (transcript lines 1987/2005) — reconstructed honestly in the
     trajectory; the failure register found no REPEATED failing actions
     (different attempts each time).
  4. Trajectory reconstruction is the weak link: token/cost figures are
     estimates (no per-tool accounting in the transcript). A capture
     hook (like the redactor from TICKET-007-v2) is the honest next
     step for codex runs.
- Next: EXP-003 — instrument codex runs (capture hook) so trajectories
  are captured, not reconstructed.


## EXP-003 — capture hook: codex transcripts become truthful trajectories (2026-08-03)

- What shipped: `mini-agi codex <spec> <workdir>` — runs codex exec on a
  slice spec with a binding completion protocol
  (`<promise>COMPLETE</promise>` + `<result>{...}</result>`), stores the
  transcript, parses it into exec/write/read steps with line provenance,
  and writes a run.json draft (every step noted "captured from codex
  transcript line N").
- Parser validated against the REAL EXP-002 transcript (the weakness
  EXP-002 identified): unittest and make invocations are provably
  captured; the `/usr/bin/bash -lc 'cmd'` transcript form is handled.
- Resume semantics: the workdir + codex.log + run.json draft ARE the
  session state — a follow-up run can re-parse or continue from the
  draft (Sandcastle-style session-resume idea, kernel version).
- Remaining: the draft needs token/cost filling by the operator
  (transcripts don't carry per-tool accounting) before ingest.
