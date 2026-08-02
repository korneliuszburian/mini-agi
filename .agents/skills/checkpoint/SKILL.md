---
name: checkpoint
description: Edit-commit checkpoint cascade (ECC). Commit state before every further edit; roll back to last green checkpoint when the verifier fails. Use before and after every edit step.
verify: scripts/checkpoint.sh status
---

# Checkpoint

Protects against Coherence Collapse: an agent that writes a correct edit and
then overwrites/destroys it. Each edit step is a recoverable transaction.

Protocol (every edit step, no exceptions):

1. BEFORE editing anything: `scripts/checkpoint.sh begin <label>`
   (commits current state, writes journal entry).
2. After the gates pass: `scripts/checkpoint.sh verify <label>`
   (runs `scripts/verify.sh`; on red, hard-resets to the last green
   checkpoint).
3. If you are about to revert or rework a change and are unsure of the last
   good state: `scripts/checkpoint.sh status` to read the journal.

Journal semantics (T008 amendments): a BEGIN is resolved by a subsequent
VERIFY-PASS or VERIFY-FAIL; an unpaired BEGIN is an anomaly unless it is the
literal last line (verification in progress). Never edit the journal by hand.

Destructive operations (reset --hard, force push, branch delete) additionally
require: state committed, journal entry exists, and human confirmation.

## Completion criteria

- [ ] `checkpoint.sh begin <label>` ran and returned a rev BEFORE the first edit.
- [ ] Every edit step has its own begin; no edits happen between steps
      without a checkpoint between them.
- [ ] `checkpoint.sh verify <label>` ran after the gates passed and the
      journal shows the matching VERIFY-PASS for that label.
- [ ] On a red gate the rollback command (`git reset --hard <last_green>`)
      is quoted from the script's own output, not inferred.
- [ ] The checkpoint journal was never hand-edited.
- [ ] No destructive git operation ran without committed state + journal
      entry + human confirmation.
