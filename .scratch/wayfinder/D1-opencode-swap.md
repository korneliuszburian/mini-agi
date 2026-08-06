# D1 — Worker economics: opencode + deepseek-v4-flash swap

Status: OPEN (recommend: HYBRID — flash for loop work, strong model for judgment)
Date: 2026-08-06. Source: track-3.md §1, §7; observed runs in canonical memory.

## Context
Workers today = codex (gpt-5.6-terra). Verified rate cards (Aug 2026): terra
$2/$12 per M (miss/cached), v4-flash $0.14/$0.28, flash cached $0.0028.
Observed own runs: codex fresh $0.27-1.31/run, resumed $0.04-0.09/run.
Per-1,000-runs: codex ≈ $600, flash ≈ $25, hybrid ≈ $40. 24/7 at 24 runs/day:
$430/mo vs $18/mo vs $30/mo. The kernel already resolves worker_name + model
per call (worker.rs) — the swap is a config + adapter, not new machinery.
opencode ships the daemon pattern at worker level (serve + run --attach +
--format json), so the kernel needs a thin adapter, not a new runner.

## Options
- (a) Full swap: every run on flash. Cheapest; judgment quality ceiling.
- (b) HYBRID (recommended): flash for implement/verify/distill/link/
  summarize; strong model (terra or sonnet 5) for ~10% of runs — auditor,
  conflict resolution, promotions (ADR-0010 trust root), planner pass.
  ≈ $1/day at 24 runs/day.
- (c) Status quo: all-strong; $400+/mo, or subscription ($20-100/mo) with
  lost per-run cost telemetry.

## Evidence
- Rate-card arithmetic (track-3.md): 20-30x gap is arithmetic, not estimate.
- Our observed cache-first codex runs (T007: 1.7M/1.8M cached) prove the
  cache lever; flash's 1h cache TTL (Mastra activateAfterIdle mapping) makes
  it moot at idle-triggered cadence.
- prime-agent RLM: cheap model + context-as-variable survives compaction —
  validates a cheap default worker for long-horizon loops.
- RISK: DeepSeek officially announced "significant price increase expected" —
  re-check the rate card at build time; selection is already config.

## Decision
OPEN. Recommended: (b) hybrid — flash default worker via opencode adapter,
strong-model tier configurable per stage (auditor/conflict/promotion/planner).

## Effort
S. Adapter at the worker seam (worker.rs:900), cost/run telemetry into run.json.

## Dependencies
None. Blocks: everything 24/7 (the whole AGI affordance).
