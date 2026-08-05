# Incidents (standards-polish S3)

Postmortem detail behind the AGENTS.md Process rules. Each cost real
work once; the rules exist so they cost nothing twice.

| # | Date | Incident | Loss | Rule |
| --- | --- | --- | --- | --- |
| 1 | 2026-08-05 | `pkill -f "codex exec"` fused with an edit in one shell line — the pkill matched the shell's own cmdline and killed it before the python edit ran. The stdin-null fix for the MCP bridge was silently not applied; three e2e runs hung before diagnosis. | ~1h debugging the 'fixed' hang | never fuse destructive commands with edits |
| 2 | 2026-08-05 | `git checkout -- memory/episodic/checkpoints.log` during a batch-cleanup — destroyed a working-tree VERIFY-FAIL line, leaving an orphan BEGIN that failed the checkpoint audit; the audit stayed red until the label was re-verified. | journal repair cycle | never restore the journal through git |
| 3 | 2026-08-05 | `grep -c` chained with `&&` — 0 matches exit 1, breaking the chain repeatedly (verify/clippy runs skipped silently). | repeated silent chain breaks | never chain 0-matches commands |
| 4 | 2026-08-05 | python heredocs with asserts before the final `open(w)` — an abort loses the whole edit (the F2 module doc was lost TWICE to aborted scripts; the F1 scratch-merge was lost once; a reviewer caught the missing code). | lost edits, rework rounds | edit scripts write at the end, one fix per script |
| 5 | 2026-08-05 | A reviewer finding (F3, v3 review) was wrongly rejected as 'already integrated' — the guards existed only in tests, not in the reviewed functions. The re-review's evidence disproved the first impression. | one extra review round | a disposition must disprove with evidence |
