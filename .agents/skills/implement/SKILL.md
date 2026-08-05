---
name: implement
description: Implement a piece of work based on a spec or set of tickets.
disable-model-invocation: true
version: 1.0.0
source: mini-agi repo (.agents/skills)
---

Implement the work described by the user in the spec or tickets, as a
procedure with checkable phases (modeled on diagnosing-bugs: every
phase ends in an artifact; a phase gate is hard).

## Phase 0 — Scope pin

The spec/ticket defines the scope. Before ANY edit, produce the scope
line: the exact file set the diff may touch.

**Done when:** you can quote the scope from the ticket/spec verbatim.
No scope quote → do not edit.

## Phase 1 — Red (test at the seam)

Use /tdd where possible, at pre-agreed seams: write the failing test
for the first behavior, run it, capture the RED.

**Done when:** you have quoted the red output of the failing test
(`file:line` + the failure text). No red output → do not implement.

## Phase 2 — Green (one behavior at a time)

Implement the minimum that turns the red green. Run typechecking and
the single test file regularly.

**Done when:** you have quoted the green output of the same test
(`file:line` + the pass line).

## Phase 3 — Checkpoint cascade

`checkpoint.sh begin <label>` BEFORE every further edit step;
`checkpoint.sh verify <label>` after each gate.

**Done when:** `checkpoint.sh status` shows no open BEGIN (or the
literal last line is the in-progress one).

## Phase 4 — Full gate + review + commit

1. Run the full suite: `cargo test --all` (or the repo's equivalent).
2. Run the repo gate: `./scripts/verify.sh` and QUOTE the tail.
3. Request `/code-review` (or the mini-agi `review` skill for
   kernel-gated work), or record the explicit human waiver.
4. Commit to the current branch.

**Done when:** the gate's `verify: ALL GREEN` line is quoted AND the
diff touches only the Phase-0 scope AND the review verdict or the
human waiver is recorded.

## Completion criteria (all artifact-bound)

- [ ] Phase 0: the scope line is quoted from the ticket/spec.
- [ ] Phase 1: the red output is quoted (test file + failure).
- [ ] Phase 2: the green output is quoted (test file + pass).
- [ ] Phase 3: checkpoint journal shows no orphan BEGIN.
- [ ] Phase 4: `verify: ALL GREEN` quoted, diff within scope, review
      verdict or human waiver recorded, work committed.
