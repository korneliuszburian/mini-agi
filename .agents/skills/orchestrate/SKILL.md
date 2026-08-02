---
name: orchestrate
description: Pipeline driver for mini-agi. Use for multi-stage work: ticket -> research -> spec -> implement -> verify -> review -> retro. Coordinates subagents, enforces checkpoints, writes memory. Invoke on any new task entering the pipeline.
---

# Orchestrate

Run the full ticket-to-retro pipeline. You coordinate; delegates do the work.

## Stages

1. **TICKET** — read the ticket (from `tickets/TICKET-*.md` or the user
   message). Write it to `memory/episodic/YYYY-MM-DD-tickets.md`
   (append-only).
2. **MEMORY** — read `memory/derived/context-brief.md` and
   `memory/canonical/index.md`. If domain fragments exist under
   `memory/derived/per-domain/`, read the relevant ones. Facts already in
   canonical memory must NOT be re-researched or re-asked.
3. **RESEARCH** — delegate to the `researcher` subagent. Accept only a
   capped summary (<= 40 lines). Any raw dumps coming back = firewall
   breach, return them.
4. **SPEC** — write `artifacts/<ticket-id>/spec.md`. If the ticket has >2
   ambiguous decisions, interview the user directly, one question at a
   time — never invent the human's side of a decision.
5. **CHECKPOINT** — `scripts/checkpoint.sh begin spec-<ticket-id>`.
6. **IMPLEMENT** — delegate to `implementer`, one vertical slice per
   invocation, TDD first. Between slices: `scripts/checkpoint.sh begin
   slice-N` before, `scripts/checkpoint.sh verify slice-N` after.
7. **VERIFY** — delegate to `verifier`. Gate = `scripts/verify.sh`. A pass
   claim requires quoted exit codes. If red: route back to implementer
   (max 3 retries total), else STOP and report.
8. **REVIEW** — delegate to `reviewer`. Rubric in
   `.agents/checks/review-rubric.md`. The verdict must carry an `Anchors:`
   line with canonical fact ids (ADR-0003); zero anchors = failed review.
   APPROVE -> continue; FIX-MINOR -> one more implementer pass; REWORK ->
   stop, report to human.
9. **EVAL** — if this ticket belongs to a tracked eval case, run
   `mini-agi eval gate` and append the run JSON to `evals/cases/<case>/`.
10. **RETRO** — write `artifacts/<ticket-id>/retro.md`: what worked, what
    failed, what changed in the pipeline.
11. **MEMORY WRITE** — append the decision log to the episodic buffer,
    then `/compact` (consolidates to canonical + re-derives + provenance
    gate).

## Termination conditions

- Max 3 implementer retries per ticket. After that: STOP, report, do not loop.
- Max 40 steps per ticket (steps are stage boundaries + slices). Over
  budget: stop, compact, report.
- Goal check after every stage: re-read the ticket. If the work drifted
  from it, revert to last green checkpoint and report the drift.

## Communication

Lean. Facts and next actions. No summaries of completed stages, no filler.
If the user activated caveman mode, keep it on for all messages.

## Completion criteria

- [ ] Every stage boundary was checkpointed (begin before, verify after).
- [ ] Research summaries were capped; raw dumps were rejected.
- [ ] The reviewer verdict quoted the `Anchors:` line; zero anchors would
      have been a failed review.
- [ ] `scripts/verify.sh` output quoted as ALL GREEN before each handoff.
- [ ] Terminal conditions were enforced, not assumed: retry count and step
      count are stated in the final report.
- [ ] The decision log was appended and compacted; provenance passed.
