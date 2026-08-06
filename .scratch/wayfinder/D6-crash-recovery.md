# D6 — Crash recovery: respawn + rebuildable run-state index

Status: OPEN (recommend: BUILD — small, mechanical)
Date: 2026-08-06. Source: track-3.md §5, §7.

## Context
prime-agent's daemon keeps recoverable workers: JSONL trajectory + state
snapshot, resume from crash. Storage-layer audit (track-3 §5): we are ALREADY
crash-safer — every run is run.json on disk, the checkpoint journal bounds
edits, bg.rs runs survive parent exit, `--resume` exists for sessions. What is
actually missing for 24/7 unattended operation:
1. Respawn-on-crash: a crashed/failed worker run does not restart itself.
2. Rebuildable run-state index: active runs are discoverable from the
   filesystem, but there is no single `status` view of what is alive.

## Options
- (a) Respawn in the dispatcher (recommended): the loop/dispatch layer
  treats a crashed worker as a retryable unit (bounded retries, MAST-
  classified, never silently; matches loop dispatch's failure injection).
  Run-state index = D4's `status --json` reading run.json + journal.
- (b) External supervisor (systemd/daemon): out of kernel, fine for prod —
  but the kernel should not require it for correctness.
- (c) No-op: accept manual recovery — contradicts the 24/7 destination.

## Evidence
- track-3 §7: "we're already crash-safer than prime-agent at the storage
  layer; what's missing is respawn-on-crash and a rebuildable run-state
  index — small, mechanical."
- Existing machinery: run.json + verify_command (post-crash re-verification),
  loop dispatch MAST injection, checkpoint journal — all present, no new
  storage needed.

## Decision
OPEN. Recommended: (a) — bounded respawn in the dispatcher + run-state
index surface.

## Effort
S.

## Dependencies
None (can land in Phase 0 with D1).
