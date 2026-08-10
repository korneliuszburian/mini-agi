# Ticket

- id: TICKET-10
- title: Fix capability gap: codex-exp-002 scores below the loop target
- goal (one sentence): Bring codex-exp-002 composite above 0.5 by fixing the failing run.
- domain: eval

## Closure evidence (2026-08-10, goal session) — ABANDONED

- The gap target (composite of `codex-exp-002`) cannot be closed: the
  experiment's verifier target `/tmp/opencode/codex-exp2` no longer
  exists on disk (checked 2026-08-10), so neither re-running the
  verifier nor re-scoring the run is possible; `run verify` fails with
  "verify target ... is not a directory" by design.
- The kernel now classifies the run honestly: `codex-exp-002-rerun` is
  `unverified` with `target_missing=true` (dashboard shows the reason;
  the run claims nothing verified because nothing can be).
- A fix would require re-creating the whole experiment checkout, which
  is out of scope for a capability-gap ticket without a living run.

Status: CLOSED (abandoned — evidence above).
