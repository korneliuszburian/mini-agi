# ADR-0005 — the intelligence layer: runs compound into the world model

Status: accepted (2026-08-02)

## Context

Product direction comes from the Sequoia thesis "From Hierarchy to
Intelligence" (Dorsey & Botha, 2026-03-31): a company organized as an
intelligence, not a hierarchy, has four layers — capabilities, a world
model, an intelligence layer, and interfaces. Two properties define the
model:

1. **The world model is built from recorded actions.** Every decision,
   code change, plan, and outcome exists as an artifact; the model is the
   continuously updated picture the hierarchy used to carry.
2. **The compounding test**: "what does your company understand that is
   genuinely hard to understand, and is that understanding getting deeper
   every day?" For Block the honest signal is money. For an agent kernel
   the honest signal is *measured* — tokens, cost, composite score, gate
   verdicts, retry counts — and it is honest because it is captured, not
   reported.

mini-agi already has the four layers: kernel modules exposed as MCP tools
are capabilities; canonical memory (provenance-gated facts) is the world
model; orchestrate composes skills per ticket (the intelligence layer);
CLI/MCP/adapters are interfaces. What is missing is the compounding loop:
**every run must leave a trace in the world model automatically.**

## Decision

1. **`mini-agi run ingest <run.json> [--retro <md>] [--ticket <id>]`** —
   turns a scored run (trajectory + 4D score) plus its retro into
   canonical facts with provenance: score/composite, tokens, cost,
   regressions, retry count, outcome. The model deepens per run without
   human writing.
2. **`mini-agi insights`** — aggregates `evals/cases/*/run.json`, tickets,
   journal, and memory into one report: cost/tokens per ticket, composite
   trend, gate verdicts, memory growth (facts added), capability gaps
   (failing eval cases = roadmap items, per the Sequoia failure-signal
   loop). Wired into `scripts/verify.sh`.
3. **The failure signal is the roadmap.** A failing eval case, a REWORK
   verdict, or a budget overrun is a roadmap item — surfaced by
   `insights`, never hidden.
4. Divergences from the PoC are additive (new commands), not semantic
   changes; the eval engine itself is untouched.

## Consequences

- The world model grows measurably: `canonical facts` in `stats`/`insights`
  increases with every run.
- Provider gain: agents stop re-learning what the system already measured;
  the brief contains the system's own score history.
- The gate reports whether understanding is compounding (facts added,
  scores stable) instead of only whether the build is green.
