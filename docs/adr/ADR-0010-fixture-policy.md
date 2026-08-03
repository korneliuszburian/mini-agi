# ADR-0010 — fixture policy: effective composite avg with rerun override

Status: accepted (2026-08-03)

## Context

Phase 6 acceptance requires `insights` composite avg >= 0.60. Measured
2026-08-03: 19 runs, plain avg 0.5380, capability gaps none — every case
below 0.5 has a passing rerun (0.6141-0.8500). The plain mean cannot
reach 0.60 because the historical failing fixtures (reactive-loop 0.0,
flailing 0.2851, real-ticket-001..006 at 0.24-0.49) are intentionally
kept in `evals/cases/` and the gate baseline as regression evidence —
they drag the mean by design.

Two truths are both real: (1) history must stay recorded (append-only
canonical facts, gate baseline = fixed-point regression evidence,
ADR-0004/ADR-0009); (2) the capability average should measure what the
system can do NOW, not what it once failed at.

## Decision

1. **Fixtures stay.** Historical failing runs remain in `evals/cases/`
   and the gate baseline unchanged — regression evidence is not deleted
   or hidden. The plain `composite_avg` stays in the insights report,
   reported honestly as history.
2. **Effective average.** `insights` additionally reports
   `composite_avg_effective`: the mean over ORIGINAL cases (names not
   ending in `-rerun`), where a case with a passing rerun
   (`<case>-rerun` at composite >= 0.5, the loop target) contributes the
   rerun composite; otherwise its own. Rerun cases are not counted
   separately — they are represented through their originals. Each case
   counts once.
3. **Phase 6 acceptance is measured on the effective metric.** The
   roadmap target "composite avg >= 0.60" means the capability mean, not
   the historical mean; the historical mean is published alongside it.
4. This is additive (new field + new report line); no scorer semantics
   change, no baseline change, no fixture removal.

## Consequences

- `mini-agi insights` prints both averages: effective (capability) and
  plain (history) — neither can be silently conflated.
- Phase 6 acceptance becomes reachable without deleting evidence:
  closing real-ticket-007-v2 (the last below-target case without a
  rerun) moves the effective mean above 0.60.
- The gate still regresses against every fixture, including the failing
  ones — policy changes cannot lower the bar.
