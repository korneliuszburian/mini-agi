# ADR-0006 — tool parity: `edit` and `write` are one tool family

Status: accepted (2026-08-03)

## Context

D3 (tool-use score) compares each run step against the golden trajectory
by index and counts a mismatch when `step.tool != golden.tool`
(`tool_score`, exact port of PoC `tool_score`). Measurement across 7
instrumented real-ticket runs (32 mismatches total, Phase 6.2 diagnosis,
`4b10819`):

- **Alignment drift** — real runs are 21-167 steps vs 7-step goldens;
  extra setup reads/execs shift indices, so early steps compare against
  the wrong golden step. Real signal about behavior shape, kept as-is.
- **Genuine tool-kind differences** — e.g. `exec` where the golden says
  `edit` (the agent ran `sed`/checkpoint instead of editing directly),
  `read` vs `write` at the same index. Semantic, kept as mismatches.
- **A latent comparability artifact**: the goldens use `edit` for
  file-modification steps, but the instrumented harness only ever emits
  `read`/`exec`/`write` — it cannot produce an `edit` step. Any future
  run that modifies a file (as `write`) against an `edit` golden would
  be penalized forever through no fault of its own. Measured impact on
  the current 11 cases: **0 of 32 mismatches** are pure `write`↔`edit`
  pairs, so no existing score changes. `write` and `edit` both mean
  "modify a file" (the scope-violation checker already treats them
  identically, `find_scope_violations`).

Phase 6.2 target is ≤ 1 tool mismatch per run; the metric must be free
of harness-impossible comparisons for that target to be meaningful.

## Decision

1. Tool parity compares *tool families*, not raw tool names:
   `write` and `edit` normalize to `file-modify`; `read`, `exec` and any
   future tool normalize to themselves. A mismatch is counted only when
   the families differ.
2. The mismatch *detail* (`tool_mismatches_detail`) keeps raw names, so a
   report still shows exactly what the run did vs what the golden says.
3. Scope-violation semantics are unchanged (`write`/`edit` were already
   the same there).
4. This is a semantic change to a v1-derived scorer behavior; it is
   authorized by this ADR. Historical canonical facts (ingested run
   scores) are append-only and stay as recorded under the old semantics;
   `insights` re-scores `run.json` files live and therefore reports the
   new semantics from the next run on.

## Consequences

- **No existing run's score changes**: measured 0 pure `write`↔`edit`
  mismatches across the 11 baseline cases. The committed baseline
  (`evals/results/baseline.json`) was refreshed via `eval gate
  --write-baseline` and differs only by the additive `reactive-loop-rerun`
  case from the earlier snapshot.
- The fix is prospective: future goldens may use either name without
  penalty, and harness runs are never penalized for `write` vs golden
  `edit`.
- The remaining mismatch signal is alignment drift plus genuine
  tool-kind differences (e.g. `exec` instead of `edit`) — actionable
  coaching: prefer the golden step shape.
- Future goldens should use `write` for consistency, but old goldens
  keep `edit` (they are PoC artifacts).
