# D8 — Matt Pocock skills v1.2 (mattpocock/skills @ 8b36d4f, 2026-08-05)

Status: OPEN (dispositions decided below; implementation pending user choice)
Date: 2026-08-06. Method: source-to-decision. Source: newsletter + repo clone
(/tmp/opencode/mattpocock-skills, commit 8b36d4f, Wed Aug 5 2026).

## Mechanism extraction (per skill)

| Matt v1.2 skill | Stated mechanism | Local counterpart | Gap / overlap |
|---|---|---|---|
| /prototype LOGIC branch | single shareable HTML file (free-play + tabbed walkthroughs), drivable by a NON-developer | our LOGIC.md = tiny TUI over a pure module | TUI not shareable/non-dev-drivable; we lack the "capture as primary source" step |
| /prototype rule 6 | capture: commit prototype to a THROWAWAY BRANCH + context pointer on the implementation issue; main keeps only the validated decision | ours: "fold validated decision into real code" only | NEW mechanism, adopt |
| /writing-for-agents (renamed from /writing-great-skills) | scope: ANYTHING agents read (skills, AGENTS.md, CLAUDE.md, docs). Machinery: context pointers (wording decides reaching), two loads (context/cognitive), information hierarchy (in-file step/ref → disclosed → external), progressive disclosure, co-location, sprawl, completion criteria (clarity/demand), LEADING WORDS, negation, pruning (SSOT, environment-as-truth, relevance, no-ops test) | our writing-great-skills = kernel-seam skill authoring (contract, one-owner, hooks, versioning) | ours has the kernel machinery Matt lacks; Matt has doc-writing machinery we lack (leading words 1 mention, no loads/hierarchy/negation/no-ops) |
| /grilling rounds | design tree + frontier rounds + numbered Qs with recommended answers + NON-BLOCKING fact subagents | our domain-modeling: frontier rounds + recommended answers (already superset) | only missing: non-blocking fact dispatch during rounds |
| /wizard (NEW) | bash wizard with template.sh: stages, progress+time-remaining, confirmation gates, URL opening (WSL ok), hidden secrets, idempotent .env upserts, gh secret writes; agent scopes + authors stages only; bash -n + shellcheck; static trace; boundary rule: never for steps the agent can do itself | none | no current consumer; trigger = next multi-step human-only provisioning |
| /to-questionnaire (NEW) | decision the user can't answer → questionnaire DOC for a third party (async); grill only the SEND (who + what you need back); most-important-first; one idea per question; template given; disable-model-invocation | to-spec (synthesis) / to-tickets (decomposition) | distinct slot: pulls knowledge a THIRD PARTY holds; composes with domain-modeling |
| /wait-what (NEW) | human says "wait what" → re-pitch: context + ASD-STE100 simplified English + ubiquitous language from CONTEXT.md; disable-model-invocation | none | tiny mode skill, complements caveman; STE100 + CONTEXT.md ubiquitous language |

## Dispositions

1. prototype rule 6 — ADOPT: add "capture as primary source" (throwaway
   branch + context pointer on the implementation issue) to our prototype
   SKILL.md. Cheap, concrete, adds provenance (fits our memory-first culture).
2. prototype LOGIC branch — LAB-TEST: next logic-prototype task, build one
   HTML per Matt's template and compare against the TUI experience. No
   observed local failure of the TUI (no counterexample), so no forced
   change; the shareable/non-dev value is real but marginal for a solo user.
3. writing-great-skills → writing-for-agents — ADOPT: rename + widen scope
   (AGENTS.md, CLAUDE.md, docs) + PORT the doc-writing machinery (context
   pointers, two loads, information hierarchy, leading words, negation,
   no-ops test) as reference material; KEEP our kernel-seam machinery
   (contract, one-owner, hooks, versioning — Matt has none of these).
   Version bump + hook update (gate-bound, our rules).
4. grilling — REJECT (we are a superset via domain-modeling); optionally
   port the non-blocking fact-dispatch nuance into frontier-rounds.md (S).
5. wizard — DEFER with named trigger: next multi-step human-only
   provisioning (GH runner setup, domain/credential provisioning, CI
   secrets). Boundary rule ("never for steps the agent can perform")
   recorded — it is the transferable mechanism.
6. to-questionnaire — ADOPT: new skill (disable-model-invocation, small,
   template vendored). Composes with domain-modeling ("grilling question
   you can't answer" → questionnaire for the colleague).
7. wait-what — ADOPT: new mode skill (type: mode, like caveman; no hook
   needed). ASD-STE100 + CONTEXT.md ubiquitous language.
8. Catalog/marketplace/distribution (Claude plugin, Codex yaml, 13.5M
   downloads) — REJECT as mechanism: popularity is not a mechanism; our
   enforced kernel registry (hooks/one-owner/versioning) is a deliberately
   stronger model; agents/openai.yaml routing we already have.

## Decision return
Consumer and sole writer: USER (HITL — skill-layer is user-governed; the
previous skill-layer goal closed with the same gate).
Falsifier: each adoption is gate-bound — `mini-agi skill verify --all`
must stay 14+ PASS (hooks) and one-owner lint must pass after any edit;
the to-questionnaire hook must verify the template + grill-the-send rule.
Does not prove: that HTML logic prototypes are better for OUR solo use
(lab-test will answer); that v1.2's popularity validates its mechanics.
Tracker record: pending (this ticket) — implementation chosen by user.
