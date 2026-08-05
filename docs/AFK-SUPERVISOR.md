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

## Self-hosting proof (S6)

`mini-agi loop run afk-max-idle` implemented a real kernel improvement (the
`--max-idle` flag), the kernel's verifier passed on attempt 1, `loop verify`
closed the gap (composite 0.8409). The dogfood surfaced three real fixes:
the supervisor now persists the run draft (run_out), claims
achieved=verifier_passed, and `audit_verifier_vacuous` uses a unique
per-call temp dir (concurrency). This is the "our system builds our system"
loop: kernel drives codex to build the kernel, verified by the kernel.

## Deferred to v2 (with rationale)

- **Session resume** (`codex exec resume` on verifier failure instead of a
  cold re-invoke): Sandcastle does this; our cold re-invoke already recovers
  (EXP-013 evidence) and resume has CODEX_HOME gotchas — revisit when
  iteration cost dominates.
- **Loop templates** (sequential-reviewer / parallel-planner): issue-tracker
  shaped; single-loop first, templates when a second consumer exists.
- **Web dashboard**: rejected by research (MCP + CLI + run-report file is the
  right surface for a solo kernel; a dashboard is team tooling).
