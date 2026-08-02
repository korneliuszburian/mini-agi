# ADR-0003: HITL = external LLM reviewer (memory-anchored); grilling removed

Status: accepted (2026-08-02)

## Context

PoC ships `grill-me` and `grill-with-docs` as 3-line stubs referencing a
`/grilling` command + `/domain-modeling` skill that DO NOT exist (dead
reference from v2 scaffold, commit 601e11c; never run, no transcripts).
Wayfinder lists "grilling (HITL)" as the default ticket type.

User decision: in THIS system, human-in-the-loop means an EXTERNAL LLM as
independent reviewer — not a Socratic interview with the human. The
reviewer must be genuinely intelligent: it needs knowledge + research
layers, and it must be anchored to VERIFIED canonical memory (it points at
facts already in the brain; it does not judge from vibes). A reviewer
without knowledge/creativity/research is a random verdict generator.

## Decision

1. REMOVE `grill-me`, `grill-with-docs` from the PoC skills set and from
   any Phase 2 port. Wayfinder ticket types drop "grilling"; default
   becomes research/prototype/task with review stage.
2. Reviewer requirements (binding for Phase 2/3 skill design):
   a. Independent session (fresh context), never self-review evidence.
   b. Memory-anchored: MUST cite canonical fact ids (`F-` entries /
      sha256[:16]) it relies on; a verdict without memory anchors is
      flagged and gated.
   c. Knowledge/research layer: reviewer may research external sources,
      but cost counts toward the run and every claim carries a citation.
   d. Deterministic gates run FIRST; LLM judge is calibrated on top
      (IBM/AAAI 2026: judge alone ~45% vs judge+tools ~94%).
3. Future grilling-style ideation is out of scope for the kernel; if ever
   wanted, it ships as a regular verifiable skill, not a bare pointer.

## Consequences

- Phase 2 skills list = PoC minus grilling stubs, rewritten from scratch
  with verify tests and checkable completion criteria (fixes the
  "we preach writing-great-skills but our skills violate it" finding).
- Reviewer skill needs a memory-anchor check (deterministic): grep fact ids
  in verdict, fail gate on zero anchors.
