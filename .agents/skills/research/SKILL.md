---
name: research
description: Auto-research a question against high-trust primary sources and land the findings in research/<slug>.md — then feed the dream-loop so the findings become memory. Use when the user wants a topic researched, docs/API facts gathered, or reading legwork delegated to a background worker.
version: 1.0.0
source: mini-agi repo (.agents/skills)
verify: sh -c "test -f .agents/skills/research/SKILL.md && grep -q 'PRIMARY SOURCES' .agents/skills/research/SKILL.md && grep -q 'Completion criteria' .agents/skills/research/SKILL.md"
---

# Research

Spin up the **auto-researcher** (an opencode flash worker via the D1
adapter) so you keep working while it reads. The kernel runs the worker,
captures the answer, and writes it to `research/<slug>.md`.

## Process

1. **Pin the question.** One question, stated precisely. The worker's
   bounded-scope contract refuses essays — the question IS the scope.
   **Done when:** the question is written down and matches what the
   decision actually needs.

2. **Run the researcher.** `mini-agi research "<question>"` — the worker
   follows the binding contract (below) against primary sources with its
   own tools. The findings land at `research/<slug>.md`.
   **Done when:** the findings file exists and the command reported the
   byte count + cost.

3. **Audit the findings.** Read the file before trusting it: every claim
   has a nearby source, `fact | estimate | opinion` labels are present,
   nothing looks invented. A fabricated claim invalidates the whole run —
   re-run with the same question (the worker re-investigates).
   **Done when:** the findings survive this check, or the run is redone.

4. **Feed the brain.** `mini-agi dream --source research/<slug>.md` —
   the distiller extracts the durable facts, the auditor judges them,
   and promotion lands them in canonical (enforced facts still need
   human signoff, ADR-0010).
   **Done when:** staging + verdicts exist and `mem verify` is clean.

## The worker contract (binding, embedded in the prompt)

- Primary sources only — official docs, source code, specs, first-party
  APIs; every claim carries its source (name + URL or path) beside it.
- Claims without a nearby source are labeled `opinion`.
- `fact | estimate | opinion` distinguished explicitly.
- NO FABRICATION: unverifiable facts are reported as
  `unknown — not verifiable from the sources I reached`. Never invent
  URLs, names, or numbers.
- Ends with a VERDICT section: established, uncertain, and what
  evidence would settle it.

## Completion criteria

- [ ] `research/<slug>.md` exists (the file path is the artifact) with
      the Findings / Sources / Verdict sections.
- [ ] Every claim in the file carries a nearby source citation; nothing
      in it is labeled as fact without one.
- [ ] The findings were fed through `dream --source` and the verdict
      manifest exists next to the staging file.
