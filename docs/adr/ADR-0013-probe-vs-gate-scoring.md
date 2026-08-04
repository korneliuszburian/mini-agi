# ADR-0013 — probe-vs-gate step scoring

Status: accepted (2026-08-04)

## Context

The eval composite is `outcome × trajectory-geomean × ticket-score`, and
the geomean is **zeroed by any step score <= 0** (`eval.rs`, PoC port).
A step is 0 when its deterministic gates failed (`ok: Some(false)`).

The honest codex capture (Phase 10) records `ok` from transcript exit
codes: a failed diagnostic command — a `sed` probe on a file that does
not exist yet, a `which` check, an exploratory `ls` — produces
`ok: false`. Under the current rule ONE such probe zeroes the entire
trajectory, so a run whose real work and final gate both succeeded scores
0. This is noise, not a signal: the run is being punished for a command
that was never meant to succeed and never touched the work.

The PoC's all-or-nothing step scoring was reasonable when trajectories
were hand-authored and every step was deliberate. The kernel's own
honest capture makes probes a routine event, so the rule must
distinguish *probe failure* from *gate failure*.

## Decision

A step with `ok: Some(false)` is scored as a **real gate failure (0)**
only when it is a *gate* step; otherwise it is a **probe failure**
scored as ungated (0.5) and flagged:

- **Gate failure** (score 0, unchanged): the step touches a path inside
  the run's declared `scope`, **or** its action matches a gate/test/
  verify command (`make verify`, `cargo test`, `cargo clippy`,
  `cargo build`, `pytest`, `python -m unittest`, `npm test`,
  `node --test`, `npx tsc`, `mini-agi verify`, `checkpoint.sh verify`,
  and the like).
- **Probe failure** (score 0.5, flagged `probe_failure`): any other
  failing step — a diagnostic whose failure carries no information about
  whether the work succeeded.

This is a deliberate, documented divergence from the PoC's step scoring
(allowed only via this ADR, per the charter's divergence rule). It is
behavior-preserving for the committed 24-case corpus (no case contains
an `ok:false` step), so the baseline does not change; it future-proofs
honest captures where probes fail routinely.

## Consequences

- `eval::score_steps` uses the run's `scope` (already threaded) and a
  small deterministic gate-command list; `step_score` (the PoC port)
  is untouched and its unit tests keep passing.
- Probe failures appear in the step report with a `probe_failure` flag
  so process supervision can still see them as a signal.
- A failing scope-touching step or a failing test/gate command still
  zeroes the run — the discipline of "verified before trusted" is
  unchanged.

## Related

- Hardening audit `docs/HARDENING-AUDIT.md`, flaw C.4.
- Phase 10 honest capture (`capture.rs`) that surfaces probe exits.
