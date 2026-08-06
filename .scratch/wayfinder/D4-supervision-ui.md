# D4 — Supervision surface: files-first read mirror + worker status

Status: OPEN (recommend: kernel exposes machine-readable status; USER builds
the UI — frontend is HITL-required, F-011)
Date: 2026-08-06. Source: track-2.md (7-panel design); track-3.md §6; F-008.

## Context
TRACK 2 recommended a local web app, files-first, 7 read-only panels (runs,
gaps, memory, journal, skills, tickets, workers) with writes staying in the
terminal/MCP; HTTP would need an ADR (kernel std-only, ADR-0012) — the mirror
reads the filesystem, no server in the kernel. TRACK 3 §6 sharpened this:
prime-agent's Agents View is a STATE VOCABULARY (live/idle/inactive), not a
new panel — one worker column on the run board covers it. Steering UI was
rejected (HITL stays terminal). Needs-you pings remain --on-done → ntfy.

## Options
- (a) Kernel status surface (recommended): a `mini-agi status --json`
  command (workers, runs, gaps, journal tail) reading the existing files —
  tiny, std-only, gives ANY future UI a stable contract. UI itself = user.
- (b) UI-first now: violates F-011 (frontend is user-only) and ships before
  the brain layer exists to show.
- (c) Terminal-only: ntfy pings forever — no history, no gap overview.

## Evidence
- track-2: CC agent view / Codex cloud board / OpenWebUI precedents — read
  mirrors are the proven solo-agent surface; steering stays out.
- track-3 §6: Agents View vocabulary fits in one column; ≤5 workers does
  not need a dashboard.
- F-008: full web dashboard rejected for the solo kernel — same reasoning.

## Decision
OPEN. Recommended: (a) — kernel-side `status --json` (S effort); UI deferred
to the user as the HITL domain.

## Effort
S (kernel side).

## Dependencies
D1/D6 (status must report real workers + recovery state). D2 (memory stats
panel needs the dream-loop data).
