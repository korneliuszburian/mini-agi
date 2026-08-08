# Gas Town / Beads architecture — what mini-agi can adopt

Research date: 2026-08-08. Source: yegge.ai/gastown (primary, author-owned),
gastownhall/gastown + gastownhall/beads (GitHub), community hub gastownhall.ai.
Status: audited (primary source, author = maintainer, single-source facts).

## What it is

- **Beads** (Oct 2025, MIT, ~23k stars) — a portable work ledger. Each unit
  of work (task, fix, merge request, agent note) is a *bead*: atomic,
  durable, version-controlled, audit-trailed, queryable across sessions.
  Stands alone as a coding-agent memory/persistence layer.
- **Gas Town** (Jan 2026, v1.0) — orchestrator built on the Beads ledger.
  Runs dozens of parallel coding agents with oversight. An early "Dark
  Factory": agents work autonomously in the background.
- **Gas City** — declarative orchestration SDK split from Gas Town; runs
  hundreds of concurrent agents.
- **Wasteland** (Mar 2026) — federation: link Gas Towns over the shared
  ledger, post/claim work on a wanted board, earn multi-dimensional
  *stamps* (quality/reliability/creativity) that compose into a portable
  character sheet (reputation from real work).

## Lexicon (roles/mechanisms that matter)

- **Bead** — atomic durable unit of work; version-controlled DB (git-backed).
- **Formula** — template for a piece of work.
- **Polecat** — worker agent; persistent identity, ephemeral sessions.
- **Witness** — patrol/watchdog agent per rig.
- **Refinery** — merge-queue processor; serializes merges (Bors-style).
- **Mayor** — chief-of-staff coordinating across rigs.
- **Stamp** — multi-dimensional attestation on completed work.

## Core thesis

"Durable memory plus parallelism win." The stack is K8s-like: workers
(polecats), serializing merge queue (refinery), per-group watchdog
(witness), cross-group coordinator (mayor). Session crash/handoff is
handled by the next session reading the ledger and continuing — no context
lost because the ledger, not the session, is the source of truth.

## Mapping to mini-agi (what we already have)

- Beads ledger      ~ checkpoint journal (memory/episodic/checkpoints.log) +
                      evals/results run ledger + memory/canonical facts.
- Refinery          ~ planner::finalize_and_merge (Bors-style merge queue).
- Witness           ~ bg worker supervision / loop run watchdog.
- Polecat           ~ bg workers with persistent session identity
                      (MINIAGI_SESSION_TAG, detached runs, respawns).
- Stamp             ~ eval gate multi-dim score (composite, outcome,
                      cost_usd, tokens, tool_mismatches).
- Persistent state ~ run-state index (status::index_runs).

## Gaps vs mini-agi (candidate adoptions)

1. **Ledger as one queryable unit.** Beads is a single version-controlled
   DB; mini-agi splits state across checkpoints.log + run.json + canonical.
   Adopt: keep run.json authoritative but consider a ledger summary index
   that is git-commit-friendly and queryable (status already partially does
   this).
2. **Multi-dimensional stamps with provenance.** Beads stamps are
   multi-axial and portable. mini-agi eval already has multi-dim score but
   no portable "attestation" concept across repos; Wasteland-style
   portable reputation is a federation feature (out of scope for a
   single-binary kernel).
3. **Formula template** — reusable task templates. mini-agi has
   evals/golden (template-like); a first-class "formula" for loop batches
   is a candidate feature.
4. **Watchdog per rig** — mini-agi has global supervision; per-batch
   witness with autonomous failure diagnosis is a candidate deepening of
   the planner/bg layer.

## Verdict for mini-agi

Core kernel ideas we already hold (durable ledger, serializing merge,
watchdog, multi-dim eval). The differentiated adoptions worth pursuing are
(1) a unified queryable ledger view and (4) per-rig witness semantics —
both are incremental, kernel-scoped, and match our charter. Federation
(Wasteland) and declarative SDK (Gas City) are out of scope for a
single-binary kernel.
