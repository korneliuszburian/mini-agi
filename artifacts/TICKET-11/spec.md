# SLICE SPEC — TICKET-11 (case: codex-exp-003)

- source: `mini-agi loop dispatch` (Phase 6.4, no human routing)
- goal: Implement scripts/token-count.py — a zero-dependency Python CLI that prints the word count of a file (usage: token-count.py <path>; exit 0 on success, exit 1 with a clear message on a missing/unreadable file). Tests in tests/test_token_count.py (unittest, no dependencies) covering: known word count, missing file error, directory path error. Makefile target verify wiring tests + smoke check.
- scope: `scripts/token-count.py`, `tests/test_token_count.py`, `Makefile`

## Acceptance (measured by `mini-agi loop verify`)

1. composite >= 0.5 on the rerun case `codex-exp-003-rerun`
2. `outcome.achieved` and all outcome gates true (per run.json outcome)
3. `mini-agi run failures` on the rerun: no repeated failing actions
4. target repo `verify.sh` ALL GREEN (where applicable)

## Implementation discipline (fresh session)

- Plan first, tests first, then implement — never repeat a failing action.
- Read `memory/derived/failures.md` (do not repeat) and
  `memory/derived/mismatches.md` (match the golden step shape) before starting.
- Record the run truthfully as `evals/cases/codex-exp-003-rerun/run.json`
  (goal/scope identical to the original case; write/edit steps carry
  their `paths` inside scope).
- Then run: `mini-agi run ingest`, `mini-agi loop verify codex-exp-003-rerun`.
