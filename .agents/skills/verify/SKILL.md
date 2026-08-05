---
name: verify
description: Deterministic verification contract. Runs scripts/verify.sh and reports exact exit codes. Use whenever code changed or tests were claimed. Verification is a requirement, not a declaration.
verify: sh -c "test -x scripts/verify.sh && sh -n scripts/verify.sh && grep -q 'verify: ALL GREEN' scripts/verify.sh"
version: 1.0.0
source: mini-agi repo (.agents/skills)
---

# Verify

Run the full deterministic gate. Do not skip, do not trust prior claims.

1. Run `scripts/verify.sh` from the repo root.
2. Record per-target exit codes and the first 10 lines of any failure.
3. Verdict:
   - PASS — every target exited 0 with output.
   - FAIL — any target non-zero, or any target produced no output at all
     (a silent target is a failing target: it may not have run).
4. On FAIL: quote the failing output verbatim, then route to implementer.
   Never "fix and claim" — rerun the gate and quote the new exit codes.

Silent-failure rule: if you cannot observe the verifier output, the
verification did not happen. Report it as a failed gate.

## Completion criteria

- [ ] `scripts/verify.sh` was run from the repo root, not from a subdirectory.
- [ ] Every target's verdict (ok/fail + exit code) is quoted in the report.
- [ ] The report names one explicit PASS or FAIL verdict.
- [ ] On FAIL, the failing target's first 10 output lines are quoted verbatim.
- [ ] No "should be green" claims without quoted rerun output.
