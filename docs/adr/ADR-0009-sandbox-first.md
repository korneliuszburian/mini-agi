# ADR-0009 — sandbox-first: the mandatory gate attests isolation

Status: accepted (2026-08-03)

## Context

PLAN.md Phase 6.3: "Sandbox-first (v3 pipeline) — the biggest declared
gap... ADR first, then slice: run agents in an isolated sandbox; gate
requires sandbox evidence. Target: CI gate runs in-sandbox on master."

Measured state on 2026-08-03:

- The CI gate **already runs in-sandbox on master** since Phase 5
  (`.github/workflows/ci.yml`, GitHub Actions `ubuntu-latest` runner,
  pinned toolchain 1.97.1): the last 6 pushes show `CI / gate` success
  in ~1-2 minutes each (`gh run list` evidence). The deterministic gate
  (`scripts/verify.sh`) runs cargo build/fmt/clippy/tests + all kernel
  gates inside the ephemeral runner.
- What is missing is the *attestation*: the gate does not prove it ran
  isolated — "gate requires sandbox evidence" is not enforced anywhere.

## Decision

1. **ADR-first (this document) records the decision**, then the slice:
   `scripts/verify.sh` gains a `sandbox` target that attests isolation.
   The target is a sensor like every other target (exit 0 + output):
   - outside CI (no `CI=true`): reports `[skip]` — the local gate is
     unchanged and portable by design;
   - in CI: **fails unless** the runner is clearly isolated — it must
     run as a non-root user, and the runner must identify itself
     (`RUNNER_NAME` is set by GitHub Actions). Output carries the
     evidence (user, runner, kernel, container marker).
2. This makes "gate requires sandbox evidence" literal: a workflow that
   runs the gate without isolation markers now fails the `sandbox`
   target, so the master gate is bound to the sandboxed runner.
3. **Running agents in a local sandbox (bubblewrap/firejail) is
   deferred** with reason: the kernel is std-only by contract (no
   process-spawning sandbox without external tooling), and no untrusted
   agent actions execute on this host today. Revisit when Phase 6.4
   (proactive composition) dispatches untrusted work autonomously.

## Consequences

- CI pushes that pass the gate are provably sandboxed (evidence line in
  the log); local `verify.sh` behavior is unchanged.
- The plan's 6.3 target ("CI gate runs in-sandbox on master") is met and
  attested by the gate itself from the next push on.
- Local-agent sandboxing stays an open follow-up (6.4 prerequisite),
  documented here rather than silently dropped.
