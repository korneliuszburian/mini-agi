# D2 — Dream-loop cadence + model assignment

Status: OPEN (recommend: event-triggered cheap distiller + gated promotion +
nightly strong audit)
Date: 2026-08-06. Source: track-3.md §2, §4; MEMORY-RESEARCH.md; Mastra OM.

## Context
Consolidation today = manual `mem consolidate` after a session (write-through).
Research candidates: Karpathy append-and-review (gravity/promote, nothing
deleted), E-mem/D-MEM cheap extractors + merge/supersede, Letta dreams on
triggers, MemoryBank sleeps nightly, Mastra runs Observer+Reflector
CONTINUOUSLY and the dense observation log REPLACES raw history, prime-agent
refines the harness plan-strong/apply-cheap. Our canonical memory is
append-only with human signoff for enforced facts (ADR-0010) — promotion is
a trust boundary, not just a perf feature.

## Options
- (a) EVENT-TRIGGERED + GATED (recommended): cheap distiller extracts facts
  after each run + on idle (idle trigger grounded in provider cache TTL:
  flash 1h / mastra activateAfterIdle); candidates staged; STRONG auditor +
  human signoff for enforced facts; nightly full audit pass closes drift.
- (b) Nightly-only batch: simplest, but facts go stale intra-day and the
  cache-TTL window is wasted.
- (c) Continuous background observer (Mastra-style): highest freshness,
  highest token burn and contention with foreground runs on one machine.

## Evidence
- Mastra defaults Observer to a cheap model (gemini-2.5-flash) and Reflector
  to a strong one — the same cheap/strong split, validated in production.
- Mem0 runs ALL memory ops on gpt-4o-mini (cheap) — extraction quality
  saturates cheap; judgment (what to promote, what conflicts) does not.
- Karpathy: append + promote, never delete — matches our append-only
  canonical + soft-delete direction (F-012).
- prime-agent reward-hacked its own auto-refine loop (Factorio) — evidence
  FOR keeping deterministic gates + human signoff in front of auto-learning.

## Decision
OPEN. Recommended: (a) — distiller=flash (event-triggered: post-run + idle),
auditor/promotion=strong + human signoff for enforced facts, nightly audit.

## Effort
M. Distiller/auditor/linker processes in the kernel + scheduler hooks +
staging area between episodic and canonical.

## Dependencies
D1 (economics: the cheap distiller only makes sense with a cheap worker).
D3 (the merge/supersede machinery the auditor promotes into).
