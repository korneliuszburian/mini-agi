# Ticket

- id: TICKET-12
- title: Research decision: what-are-the-most-notable-and-interesting-ai-agent-developme
- goal (one sentence): Apply the researched findings at research/what-are-the-most-notable-and-interesting-ai-agent-developme.md — decide, implement, and measure the change they call for.
- scope: research
- domain: research
- source: research/what-are-the-most-notable-and-interesting-ai-agent-developme.md

## Closure evidence (2026-08-10, goal session) — DECISION: defer

- The research verdict (2026-08-09) documents the July-2026 agent
  intrusion, the 2026 model wave, and first-party agent-control
  frameworks. Nothing in it calls for a NEW mini-agi mechanism:
  fingerprint-bound run verification (kernel never trusts a bare
  claim), the checkpoint journal, enforcement-bound memory, and the
  Landlock sandbox (ADR-0012) already implement the defense-in-depth
  lessons (verified claims only; system-side supervision; guarded
  execution).
- Decision: DEFER any further change until a regulator/incident post
  (OpenAI technical report, METR/Redwood assessment) lands with a
  concrete, actionable mechanism gap; re-open with evidence if one
  appears. Tracked in this ticket's decision record.

Status: CLOSED (deferred — evidence above).
