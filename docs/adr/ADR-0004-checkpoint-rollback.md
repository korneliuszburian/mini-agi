# ADR-0004 — checkpoint rollback always lands on the last BEGIN checkpoint

Status: accepted (2026-08-02)

## Context

`scripts/checkpoint.sh` is ported 1:1 from the PoC (`mini-agi`
`v1-spec-reference`). The PoC's `verify` branch had a NO-ROLLBACK
dead-end: when the last `BEGIN` rev equals `HEAD` (the common case —
edits between `begin` and `verify` are usually uncommitted), a red gate
left the broken working tree as-is.

A codex review (second opinion, 2026-08-02) flagged this against our own
`checkpoint` skill contract, which states: "on red, hard-resets to the
last green checkpoint".

## Decision

1. On a red `verify`, always `git reset --hard` to the last `BEGIN` rev
   (the BEGIN commits current state, so its rev is the recovery point) —
   including when it equals `HEAD`, in which case the reset discards the
   uncommitted broken edits.
2. NO-ROLLBACK now applies only when the journal has no `BEGIN` at all.
3. The `VERIFY-FAIL` journal line is written AFTER the reset, because
   `reset --hard` restores the journal to the checkpoint commit's version
   and would swallow a line journaled before it. The journal must record
   the outcome (T008: the journal entry is the recovery point).

Deviation from the frozen PoC script is documented here, not silently:
the PoC's rollback tests that assert "rollback to the begin revision" and
"journaled before reset" are superseded by the semantics above.

## Consequences

- Coherence Collapse is recoverable even without intermediate commits.
- The journal always contains the `VERIFY-FAIL` for any red gate.
- The `git clean -fd` after reset keeps only `checkpoints.log`; the
  `mkdir -p` re-creates the journal directory.
