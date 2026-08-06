# D5 — A2A / daemon shape

Status: OPEN (recommend: DEFER A2A wire protocol; existing daemon-ish seams
are enough)
Date: 2026-08-06. Source: track-3.md §3, §7; prime-agent; our bg.rs/planner.rs.

## Context
prime-agent's Continual Harness routes agent-to-agent messages through a
daemon over a local socket, restricted to a "nuclear family" (parent/sibling/
child) — they built isolation because their agents mutate a shared harness.
We have: kernel-owned shared state (canonical memory, tickets, skills,
journal) as the coordination substrate, parallel dispatch (planner.rs) with
per-worker names, detached runs (bg.rs), MCP bridge. Workers never own the
state — the kernel does.

## Options
- (a) DEFER A2A (recommended): files + planner manifests + kernel state
  already beat a wire protocol for ≤5 single-machine workers. Workers
  communicate by what they write (facts, tickets, runs), not by messages.
- (b) Daemon with socket: adopt the prime-agent pattern — new surface, new
  failure modes, no demonstrated gap at our scale.
- (c) bg.rs-only: current detached runs already survive parent exit — but
  nothing respawns a crashed worker (that is D6, not D5).

## Evidence
- track-3 §7: "A2A messaging is mostly noise even for prime-agent (nuclear
  family); files + planner manifests beat a wire protocol for ≤5 workers."
- Our worker_state lives in the kernel (worker.rs), not in workers — the
  coordination substrate exists; a protocol would duplicate it.
- opencode already ships worker-level serve/attach — the daemon pattern is
  present where it matters (the runner), not in our coordination layer.

## Decision
OPEN. Recommended: (a) DEFER. Revisit only if workers exceed ~5 concurrent
or need live turn-taking.

## Effort
None now.

## Dependencies
D6 covers the real gap (respawn). D4 covers the visibility gap.
