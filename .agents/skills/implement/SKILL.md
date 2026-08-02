---
name: implement
description: Implement a piece of work based on a spec or set of tickets.
disable-model-invocation: true
---

Implement the work described by the user in the spec or tickets.

Use /tdd where possible, at pre-agreed seams.

Run typechecking regularly, single test files regularly, and the full test
suite once at the end. Checkpoint before every edit step and after gates
(/checkpoint).

Once done, use /code-review to review the work.

Commit your work to the current branch.

## Completion criteria

- [ ] Every edit step was checkpointed (begin/verify journal entries exist).
- [ ] Tests were written at the pre-agreed seam, red before green.
- [ ] `scripts/verify.sh` output quoted, ALL GREEN, before handoff.
- [ ] The diff touches only the ticket's scope.
- [ ] Work is committed to the current branch.
- [ ] A code-review pass was requested or explicitly waived by the human.
