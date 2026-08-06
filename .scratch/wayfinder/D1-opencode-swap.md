# D1 — Worker economics: executor/reviewer layering

Status: OPEN (recommend: LAYERED — codex executes, opencode/flash plans+reviews)
Date: 2026-08-06 (amended: role model corrected by user). Source: track-3.md
§1, §7; observed runs in canonical memory.

## Role model (user, 2026-08-06, authoritative)

CODEX = main executor (owns ~/.codex/skills + krn-codex-skills catalog +
mini-agi MCP). OPENCODE (deepseek v4 flash) = REVIEWER/PLANNER — the
bounded read-only advisory role (opencode-second-opinion skill) plus the
cheap stages of the brain layer. NOT a swap: the executor stays codex.

## Context
Verified rate cards (Aug 2026): terra $2/$12 per M (miss/cached),
v4-flash $0.14/$0.28, flash cached $0.0028. Observed own codex runs:
fresh $0.27-1.31/run, resumed $0.04-0.09/run. Per-1,000-runs: codex
≈ $600, flash ≈ $25. 24/7 at 24 runs/day: codex ≈ $430/mo, flash
≈ $18/mo. The kernel resolves worker_name + model per call (worker.rs);
opencode ships serve + run --attach + --format json (thin adapter at
worker.rs seam, per track-3 §7). The distiller/planner/reviewer stages
(D2 dream-loop) run on flash; execution stays codex.

## Options
- (a) LAYERED (recommended, corrected): codex executes (implement/verify/
  promotion), opencode/flash runs planner passes, second opinions,
  distiller/linker/summarizer, review drafts. Cost: codex $430/mo at
  24 runs/day is the real envelope — lever = batch API (halves terra to
  $1/$6) or $20-100/mo subscription for execution, flash advisory ~$5/mo.
- (b) Full swap executor → flash: rejected by the role model (codex owns
  execution + its skill surface).
- (c) Status quo single-tier: no cheap layer at all; the brain layer's
  stages burn terra tokens.

## Evidence
- Rate-card arithmetic (track-3.md): 20-30x gap; flash 1h cache TTL.
- opencode-second-opinion skill (exists in BOTH ~/.agents/skills and
  ~/.codex/skills): the reviewer/planner role is already a designed seam.
- prime-agent RLM: cheap model + context-as-variable survives compaction —
  validates flash for the long-horizon cheap stages (distill/link).
- RISK: DeepSeek price increase announced — re-check at build time.

## Decision
OPEN. Recommended: (a) — codex executor with batch/subscription lever,
flash for planner/reviewer/distiller stages; adapter + cost/run telemetry
in run.json.

## Effort
S (adapter + telemetry). D2's cheap stages consume this.

## Dependencies
None. Feeds: everything 24/7 (the AGI affordance).
