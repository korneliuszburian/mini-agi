# D7 — Agent-managed harness CRUD vs append-only guarantees

Status: OPEN (recommend: KEEP append-only canonical + gate-bound agent
edits; no agent-owned CRUD on the harness)
Date: 2026-08-06. Source: prime-agent Continual Harness; ADR-0010; F-012.

## Context
prime-agent lets the agent CRUD its own harness state (prompts, skills,
memory, sub-agents) from inside its trajectory — self-modifying system.
We have deliberate counterweights: canonical memory is append-only with
human signoff for enforced facts (ADR-0010), skills are one-owner contracts
with hooks in the deterministic gate, tickets route through kernel tools,
the journal is never edited. The devils-advocate review culture + the
CI-as-enforcement are the same trust root prime-agent lacks (they
reward-hacked their own refine loop in Factorio — track-3 §7).

## Options
- (a) KEEP current model (recommended): agents edit the world through
  kernel tools (tickets, skills add/verify, memory signoff flows), canonical
  stays append-only, soft-delete/supersede (D3) is the only mutation path,
  skills remain gate-bound contracts. Self-modification only via gated,
  reviewed, one-owner surfaces.
- (b) Partial CRUD: allow agent soft-edits (supersede/provenance-chained)
  without signoff — erodes the ADR-0010 trust root for marginal latency.
- (c) Full Continual-Harness CRUD: maximal autonomy, proven unsafe by
  prime-agent's own reward-hack incident.

## Evidence
- prime-agent Factorio reward-hack: auto-learned self-modification without
  gates produced goal-misaligned behavior — our deterministic gates +
  human signoff exist exactly for this.
- Mastra's Observer REPLACES raw history with a derived dense log — the only
  precedent for agent-side "history mutation", and it is write-through by
  the system, not the agent editing its own harness.
- ADR-0010: enforced facts are human-signed by design — a decision, not a
  gap.

## Decision
OPEN. Recommended: (a). D7 stays a decision-doc unless evidence changes.

## Effort
None now.

## Dependencies
D3 (supersede = the sanctioned mutation path).
