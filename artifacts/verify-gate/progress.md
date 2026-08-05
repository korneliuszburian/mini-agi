# progress — TICKET: verify.sh's step() hides failures behind `head -20` — the real failure output (often mid-log) is invisible in the gate log, and this cost hours of flake diagnosis this session (the vacuous-audit race: the gate log showed only 14 green test lines, no failure). Contract:
1. Extract step() and skip() from scripts/verify.sh into a new scripts/gate-lib.sh (sourced by verify.sh with `. scripts/gate-lib.sh` — use the portable `source`/`.` that works in POSIX sh).
2. In gate-lib.sh, a failing step prints the FULL captured output — no head -20 truncation (keep the [FAIL] <label>: header).
3. verify.sh keeps its exact step order and behavior otherwise (ALL GREEN on success).
Do NOT run checkpoint.sh and do NOT commit: the supervised loop is the gate. Run nothing that writes outside scripts/. The verifier (repo gate behavior on a synthetic failing step + full build/test suite) is run by the kernel, not by you.

- 2026-08-05T12:01:26Z attempt 1 started
- 2026-08-05T12:07:22Z attempt 1: verifier FAILED — remaining cases: 
- 2026-08-05T12:07:22Z attempt 2 started
- 2026-08-05T12:07:22Z attempt 2: RESUMING worker session 019fd1d0-d5ea-7de0-8fd0-daefd3d65b51
- 2026-08-05T12:08:24Z attempt 2: verifier FAILED — remaining cases: 
- 2026-08-05T12:08:24Z attempt 3 started
- 2026-08-05T12:08:24Z attempt 3: RESUMING worker session 019fd1d0-d5ea-7de0-8fd0-daefd3d65b51
- 2026-08-05T12:09:46Z attempt 3: VERIFIER PASSED
- 2026-08-05T12:11:23Z REVIEW: APPROVE (8/8)
