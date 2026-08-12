# Long-running goals: context retention across auto-compaction

Date: 2026-08-12. Grounded in the repo's own machinery (compact skill,
handoff skill, canonical memory, checkpoint journal) and the
ARCHITECTURE-CONDENSED/PIPELINE-DESIGN decisions.

## The problem

A goal with auto-continue keeps working after the context window is
compacted, but the WINDOW does not survive compaction. The agent that
continues must reconstruct state from durable artifacts, not from the
previous turn's message list.

## What survives (and what does not)

| Artifact | Survives | Role |
| --- | --- | --- |
| Goal objective | yes | The durable pointer: scope, discipline, terminal condition. Must be concise-but-complete. |
| Canonical memory | yes | Consolidated facts (decisions, findings, next actions) via `mem consolidate`. |
| Checkpoint journal | yes | The edit/recovery audit trail. |
| Change journal (this design) | yes | Rolling cycle log: per cycle, what changed + what is next. The resume read. |
| The context window | NO | The conversation text. Must never be the only place a decision lives. |

## The protocol (per cycle)

1. WORK on the goal (falsifier -> fix -> gates -> commit).
2. JOURNAL: append one entry to `memory/episodic/goal-journal.md`:
   `- <date> <cycle>: <what changed> | <next>`. Append-only, never rewrite.
3. CONSOLIDATE: `mem consolidate` the cycle's key facts (decisions,
   findings) into canonical so they survive.
4. KEEP the objective lean: update it only when the scope materially
   changes; the journal carries the detail.
5. RESUME rule: a fresh session reads, in order: the goal objective,
   the change-journal tail, `mem query` for the domain, then the commit
   history. State explicitly what it loaded.

## Why this resists slop

- The journal makes the goal's progress externally auditable (a change
  log, not a chat).
- Consolidating decisions means the next session does not re-derive
  them — the "told once, used forever" the kernel exists for.
- The anti-slop gate still applies per cycle (LESSONS.md): every change
  must feed a decision or reduce an observed failure.
