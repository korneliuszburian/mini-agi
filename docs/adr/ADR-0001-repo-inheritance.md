# ADR-0001: Repo Inheritance — three generations, one brain

Status: accepted (2026-08-02)

## Context

The user maintains three repos that are ONE lineage:

| Gen | Repo | Role |
|---|---|---|
| v1 | `agentic-core` | Proof of loop (6 runs, 5 independent reviews). Knowledge source for ingest (ADR-0012 v2, pkt 5). Legacy stack (Python scripts, old checkpoint semantics). |
| v2 | `mini-agi` (tag `v1-spec-reference`) | FROZEN behavioral spec: 82 tests, 11 eval cases, golden trajectories, ADR-0001..0012, 17 skills. The contract we port. |
| v3 | `mini-agi-rs` | Rust product under construction. Spec = v2 PoC; knowledge = v1 canonical facts (agentic-core@HEAD). |

## Decision

1. Behavioral contract comes from `mini-agi` (PoC), NOT from `agentic-core`.
   Any semantic divergence between the two: PoC wins.
2. Canonical facts for dogfooding/memory import come from `agentic-core@HEAD`
   (its canonical memory predates v2 and was the ingest source for v2).
3. Checkpoint semantics are ported from PoC `scripts/checkpoint.sh`
   (BEGIN/VERIFY/FAIL, ADR-0003 v2) — NOT from v1's legacy cascade.
4. Charter (the founding user prompt) lives verbatim in `docs/CHALLENGE.md`.
   Never paraphrase it; changes only via ADR.

## Consequences

- No mixing of legacy behavior into v3: ports are test-locked to PoC outputs.
- Cross-repo fact IDs must match: `sha256(body)[..16]` everywhere, so
  canonical memory from v1/v2 carries into v3 unchanged.
