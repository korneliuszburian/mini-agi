# ADR-0015: condensation — the kernel is a knowledge layer, not a worker

Date: 2026-08-12

## Status

Accepted.

## Context

The charter imagined a company-wide agentic pipeline. A solo-context
build drifted into measurement/verification machinery (616 tests, 39 MCP
tools, 16-step gate, eval scoring, registers, dashboard) whose parts did
not feed any decision the owner reads. The EXP-017 dogfood (ekologus-3d)
confirmed: the kernel does not make a product good; for visual
composition a blind worker plus a vision judge could not converge.

## Decision

The kernel is condensed to: research -> knowledge -> patterns ->
implementation. What remains: canonical memory (consolidate/query/derive),
the dream distiller, patterns (skills), the gap loop (open/closed +
deterministic gate verify), the harness counterfactual gate, the worker
execution seam, and a 14-tool MCP surface. Removed: eval scoring,
verifier/calibration, registers, reporting modules, dashboard/planner/
autoresearch, and the 16-step gate ceremony (now 9 steps).

Key semantics:
- A gap is OPEN when its run reports achieved=false; CLOSED only when the
  declared verify_command (run in verify_target) exits 0.
- loop verify never writes canonical memory (HITL stays at the write
  layer); dispatch refuses cases without both verify_command and
  verify_target; malformed budgets are errors, not unlimited.
- Verification = running the declared gate; measurement that does not
  change a decision is cut.

## Consequences

The kernel is small enough to reason about; the pipeline design and the
lifecycle (evals/ledger) are specified in docs/ARCHITECTURE-CONDENSED.md
and docs/PIPELINE-DESIGN.md; anti-slop rules in docs/LESSONS.md.
