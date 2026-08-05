---
name: compact
description: Two-stage context compaction. Stage 1: checkpoint + episodic buffer append. Stage 2: consolidate to canonical memory, re-derive views, run provenance gate. Use when a session grows long, before /clear, or at stage boundaries.
version: 1.0.0
source: mini-agi repo (.agents/skills)
---

# Compact

Two-stage compaction (checkpoint + summary resumability + live-tail).

Stage 1 — nothing is lost:
- Write the session's decision log to `memory/episodic/YYYY-MM-DD-buffer.md`
  (append-only; include decisions, rejected options, and their reasons).
- `scripts/checkpoint.sh begin compact-<stamp>`

Stage 2 — knowledge is stabilized (all through the CLI, one command each):
- `mini-agi mem consolidate memory/episodic/YYYY-MM-DD-buffer.md
  --domain <domain>` — episodic -> canonical, deduped by content hash,
  provenance written.
- `mini-agi derive` — regenerates `memory/derived/context-brief.md` and the
  per-domain AGENTS.md fragments.
- `mini-agi provenance` — drift gate; the fingerprint must match the
  committed index.
- `scripts/checkpoint.sh verify compact-<stamp>` — journal closes the BEGIN.

After compaction:
- The session may be `/clear`ed. Resumability = `memory/derived/context-brief.md`
  + `memory/canonical/index.md` + the checkpoint journal (live-tail of last
  entries). State it explicitly when you resume: which entries you loaded.
- Never compact without the checkpoint commit: the journal entry is the
  recovery point if consolidation mis-fires.

## Completion criteria

- [ ] The decision log was appended to the episodic buffer, not overwritten.
- [ ] `checkpoint.sh begin compact-<stamp>` ran before consolidation.
- [ ] `mini-agi mem consolidate` reported no crash and facts carry
      provenance (source, date, domain).
- [ ] `mini-agi derive` regenerated the brief and fragments.
- [ ] `mini-agi provenance` printed the canonical fingerprint and matched.
- [ ] `checkpoint.sh verify compact-<stamp>` journaled VERIFY-PASS.
- [ ] On resume, the session states which entries it loaded.
