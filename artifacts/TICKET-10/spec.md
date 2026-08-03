# SLICE SPEC — TICKET-10 (case: codex-exp-002)

- source: `mini-agi loop dispatch` (Phase 6.4, no human routing)
- goal: Implement scripts/checksum.py — a zero-dependency Python CLI that prints the sha256 hex digest of a file (usage: checksum.py <path>; exit 0 on success, exit 1 with a clear message on a missing/unreadable file). Add a Makefile target `checksum` that validates the script against a known fixture, wired into `verify`. Tests in tests/test_checksum.py (unittest, no dependencies) covering: known digest value, missing file error, directory path error.
- scope: `scripts/checksum.py`, `tests/test_checksum.py`, `Makefile`

## Acceptance (measured by `mini-agi loop verify`)

1. composite >= 0.5 on the rerun case `codex-exp-002-rerun`
2. `outcome.achieved` and all outcome gates true (per run.json outcome)
3. `mini-agi run failures` on the rerun: no repeated failing actions
4. target repo `verify.sh` ALL GREEN (where applicable)

## Implementation discipline (fresh session)

- Plan first, tests first, then implement — never repeat a failing action.
- Read `memory/derived/failures.md` (do not repeat) and
  `memory/derived/mismatches.md` (match the golden step shape) before starting.
- Record the run truthfully as `evals/cases/codex-exp-002-rerun/run.json`
  (goal/scope identical to the original case; write/edit steps carry
  their `paths` inside scope).
- Then run: `mini-agi run ingest`, `mini-agi loop verify codex-exp-002-rerun`.
