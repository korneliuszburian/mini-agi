# AFK supervisor (`mini-agi loop run`) — AFK-SUPERVISOR

The **away-from-keyboard verified-iteration supervisor**: one command drives a
background worker (codex) to implement a goal under the kernel's
verified-iteration, ending in a reviewable artifact.

## The pattern (research-grounded)

Matt Pocock's AFK ("away from keyboard") model — give input before and after,
not during; fresh context per attempt; end in a reviewable artifact; a
deterministic completion signal. His Ralph loop uses PRD + progress.txt +
`<promise>COMPLETE</promise>`; his Sandcastle library (`@ai-hero/sandcastle`,
7.2k stars) is the successor — the kernel **independently converged on the
same de-facto standard**: the byte-identical `<promise>COMPLETE</promise>`
signal, `<result>` XML-tagged JSON, branch/workdir isolation, and a two-phase
timeout model.

What the kernel adds to Sandcastle's model: the **deterministic verifier** —
iteration continues only while a non-vacuous gate actually fails, and nothing
counts as done until the verifier passes (verified-iteration, EXP-013: 82.5%
vs 25% single-shot on below-bar tasks). A supervised run is NEVER claimed
successful on the worker's word.

## Usage

```
mini-agi loop run <goal-or-case> [--workdir <dir>] [--verify <cmd>]
    [--target <dir>] [--iterate N] [--max-wall <s>] [--max-idle <s>]
    [--blind-worker --hidden-dir <dir>] [--no-sandbox]
    [--on-done <cmd>] [--report <path>]
```

- `<goal-or-case>`: an existing case (evals/cases/<name> — its goal, scope,
  verifier AND its verify_target are reused, P0-3 enforced) or an ad-hoc goal
  (requires `--verify`; target defaults to the workdir).
- Artifacts: `progress.md` (per-attempt events), `REPORT.md` (reviewable:
  goal, attempt chain, verifier verdict, cost proxy, run.json path), and the
  run draft written to the case's run.json (when the goal is a case) or
  `workdir/run.json`.
- The draft's `outcome.achieved` = the kernel's in-loop verifier result
  (the claim is backed by the same deterministic verifier, but per
  ADR-0011 the TRUSTED verification record is written only by `run
  verify` / `loop verify` — the claim stays the run's own until then).

## Two-phase liveness

1. **Idle timeout** (`--max-idle` / config `max_idle_seconds`): the worker's
   output-file mtime is the liveness signal; silence for a full idle interval
   since the LAST output → killed as STUCK.
2. **Completion grace**: a cap-killed worker whose transcript already carries
   the completion marker resolves as success-with-warning — the file-redirect
   design keeps the full transcript readable after the kill (`attempt_grace`).

## The on-done hook

`--on-done <cmd>` runs `sh -c <cmd> on-done <report-path> <outcome>` with
outcome `0` (verifier passed) / `1` (exhausted) / `3` (aborted) in `$1` / `$2`
— the notification point (ping script, etc.).

## Self-hosting proof #1

`mini-agi loop run afk-max-idle` implemented a real kernel improvement (the
`--max-idle` flag), the kernel's verifier passed on attempt 1, `loop verify`
closed the gap (composite 0.8409). The dogfood surfaced three real fixes:
the supervisor now persists the run draft (run_out), claims
achieved=verifier_passed, and `audit_verifier_vacuous` uses a unique
per-call temp dir (concurrency). This is the "our system builds our system"
loop: kernel drives codex to build the kernel, verified by the kernel.

## v2 — session resume + sequential-reviewer (AFK v2)

**Session resume** (`--no-resume` disables): on verifier failure the next
attempt resumes the worker's OWN codex session (`codex exec resume <uuid>`)
with the distilled failure feedback instead of a cold re-invoke. Ownership is
established by CONTENT, not by the newest file: the worker's prompt embeds a
run-unique unpredictable marker (`SESS-OWN-<hash>`); the session whose
rollout file contains it is provably the worker's — concurrent codex
processes (IDE sessions etc.) can never be attributed. Falls back to a cold
re-invoke when no session was captured.

**`--template sequential-reviewer`**: after the verified iteration passes, an
INDEPENDENT read-only codex session reviews the produced work (rubric:
Correctness/Security/Tests/Scope, APPROVE >=7, FIX-MINOR 5-6, REWORK <5);
REWORK/FIX-MINOR triggers ONE fix attempt via the worker's session resume
with the findings, then the deterministic verifier re-runs. The FINAL outcome
is resolved from the fix result (`resolve_final_outcome`): a fix that fails
the verifier reverts the run to NOT PASSED (draft, report, hook, exit code);
a required-but-impossible fix (no session) is never silent. Verdict parsing
is strict (exact tokens, word boundaries) and tolerant (UNPARSEABLE records
the raw text, never blocks).

## The client surface (v3)

The MCP bridge — codex sessions launch, poll and read supervised runs
through the kernel (the supervisor CLI `mini-agi loop run`; MCP exposes `loop_dispatch`/`loop_verify`). Client
details live in `docs/CODEX-INTEGRATION.md`; this doc covers the
supervisor SEMANTICS only (cross-referenced, not duplicated).

## Deferred (with rationale)

- **Session resume via stdin prompts**: the prompt currently rides in argv;
  moving it to stdin would hide it from /proc — revisit when the marker
  needs to be a security boundary (it is an ownership token today).
- **Loop templates beyond sequential-reviewer / parallel-planner**:
  sequential-reviewer and parallel-planner are SHIPPED; parallel-planner
  scales when the backlog has many independent verifiable tickets.
- **Web dashboard**: rejected by research (MCP + CLI + run-report file is the
  right surface for a solo kernel; a dashboard is team tooling).

## v4 — the parallel-planner template (`loop parallel`)

One goal becomes N parallel verified tickets: a PLANNER pass (read-only
codex) decomposes the goal into a strict versioned JSON manifest; the
kernel validates it FAIL-CLOSED (typed deserialization with
`deny_unknown_fields` — unknown fields and duplicate keys are rejected;
ids charset-limited; scopes mutually disjoint and never touching the
PROTECTED paths: `scripts/verify.sh`, `gate-lib.sh`, `evals`, `memory`,
`tickets`, `docs/adr`); each ticket runs in its OWN git worktree
(detached `loop run`), admission-capped (`max_parallel`, default 2) with
per-ticket caps and an aggregate deadline; PASSING tickets are
kernel-committed (evidence files excluded), containment-checked
(`git diff base..HEAD` must be inside the declared scope), and merged
ATOMICALLY on a scratch branch — the target branch moves only via a
final fast-forward. The FINAL GATE is the goal's own verifier, executed
only when the protected gate inputs have not drifted (committed AND
dirty) from the base.

Failure semantics: ATOMIC — any ticket failure (verifier, containment,
merge conflict, protected drift, validation) fails the whole batch with
ALL evidence preserved (worktrees, branches, reports); teardown happens
only on success. `--no-sandbox` is an EXPLICIT opt-in (the Landlock
wrapper breaks the codex npm shim); the coordinator refuses otherwise.

Blind-worker is NOT used in v1: hidden suites run only once, after the
merge, from a controller-owned location — the `*.blind-hidden` rename
would race across parallel workers.

Second-opinion validated (VIABLE-WITH-CHANGES, 6 findings) and
codex-reviewed to APPROVE 8/8 (the review records are ephemeral, in
the gitignored `.krn/`; verdicts are summarized in the CHANGELOG).

## Self-hosting proofs

- **Proof #1** (`loop run afk-max-idle`, v1): built the real `--max-idle`
  flag; kernel verifier passed on attempt 1; `loop verify` closed (0.8409).
- **Proof #2** (`loop run verify-gate-full-output`, v2): closed the real gap
  the kernel itself exposed — verify.sh's `step()` truncated failure output
  behind `head -20` (the exact gap that hid the vacuous-audit flake for
  hours). The produced change (gate-lib.sh + full line-numbered failure
  output) was verified by the kernel (attempt 3; attempts 2-3 resumed the
  worker's own session), reviewed by an independent read-only pass
  (APPROVE 8/8), and closed via `loop verify` (0.6076).
