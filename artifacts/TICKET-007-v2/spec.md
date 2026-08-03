# SLICE SPEC — TICKET-007-v2 (case: real-ticket-007-v2)

- source: `mini-agi loop dispatch` (Phase 6.4, no human routing)
- goal: You are implementing TICKET-007-v2 in this repo (/home/krn/coding/krn/mini-agi, branch pipeline-v1). Read tickets/TICKET-007.md and follow it exactly — the CONTEXT-BUDGET RULES section is binding. Doctrine (AGENTS.md): scripts/checkpoint.sh begin <label> BEFORE every edit step, scripts/checkpoint.sh
- scope: `scripts/capture-trajectory.py`, `tests/test_capture_trajectory.py`, `tickets/TICKET-007-gates.md`, `artifacts/TICKET-007-v2/`
- golden: real-ticket-redaction2.json

## Acceptance (measured by `mini-agi loop verify`)

1. composite >= 0.5 on the rerun case `real-ticket-007-v2-rerun`
2. `outcome.achieved` and all outcome gates true (per run.json outcome)
3. `mini-agi run failures` on the rerun: no repeated failing actions
4. target repo `verify.sh` ALL GREEN (where applicable)

## Implementation discipline (fresh session)

- Plan first, tests first, then implement — never repeat a failing action.
- Read `memory/derived/failures.md` (do not repeat) and
  `memory/derived/mismatches.md` (match the golden step shape) before starting.
- Record the run truthfully as `evals/cases/real-ticket-007-v2-rerun/run.json`
  (goal/scope identical to the original case; write/edit steps carry
  their `paths` inside scope).
- Then run: `mini-agi run ingest`, `mini-agi loop verify real-ticket-007-v2-rerun`.
