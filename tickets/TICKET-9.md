# Ticket

- id: TICKET-9
- title: Fix capability gap: reactive-loop scores below gate
- goal (one sentence): Bring reactive-loop composite above the gate tolerance by fixing the failing run.
- domain: eval

## Closure evidence (2026-08-03, Phase 6.1)

- Root cause: the run repeated the identical failing edit 3x with zero
  reflection (reactive loop) and never achieved the goal (D1 = 0.0).
- Fix: failure register (Reflexion) — `mini-agi run failures` hashes
  repeated failing actions into `memory/derived/failures.md`; `resume`
  surfaces them ("do not repeat") so a fresh session never repeats a
  recorded failure.
- Rerun: the same task ("Add rate limiting to auth endpoint", same scope,
  same golden=null) was executed with the register discipline (plan first,
  tests first, no repeated failing actions). Real scratch project:
  TS + node:test, typecheck clean, 9/9 tests green.
- Score: `reactive-loop-rerun` composite 0.7225 (D1 1.0, D2 1.0, D3 0.7225
  — 2 scope violations for tsconfig.json, toolchain file outside the
  declared scope, recorded honestly). Target was > 0.5.
- Register after rerun: no repeated failing actions (clean).
- World model: run ingested (15 entries, 23 facts); insights avg composite
  0.4218 -> 0.4469.

Status: CLOSED (evidence above).
