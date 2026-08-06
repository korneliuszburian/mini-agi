# PROVENANCE
# canonical_sha256: 14fc1eb2516c2324
# canonical_entries: 43
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# Domain: general (derived from canonical memory)

Applies when working on this domain. Canonical memory wins on conflict.
- `38e05948dad83b29` docs/MEMORY-RESEARCH.md (deep memory+skills pass): Anthropic context
- `7cd074b862efb583` docs/SKILLS-RESEARCH.md: 7 skill-design defects, routing gaps, dogfood
- `4ae6e387c68760c2` User direction: FULL AGI with a brain/memory layer so good it all ties
- `786416899b0bd2c2` Method: wayfinder skill (map + decision tickets). .scratch/wayfinder-
- `eb88bee62f23ab41` Research tracks: TRACK 1 DONE (brain/memory: CoALA, MemGPT/Letta,
- `7dfd93402ddc4aaf` Prime Agent (primeintellect.ai/blog/prime-agent, Aug 05 2026,
- `b9b79e0f2cdaacf4` Mastra Observational Memory (mastra.ai/docs/memory/observational-
- `0fc5cf16dc6d0efd` REJECTED: web dashboard for solo kernel (AFK-SUPERVISOR v2 doc) —
- `da7617f1e8f01004` REJECTED: parallel-planner "needs a second consumer" deferral —
- `11e6d3fb133e330e` DECIDED: reviewers = devils-advocate (roast, evidence-first);
- `d406cf06c1a1df90` DECIDED: per-domain human-review gate — frontend = mandatory HITL
- `4efccafe428586c9` DECIDED: memory consolidation = fact merge/supersede + selective
- `e40bb0673ce72f70` DECIDED: skills = contracts with hooks in the gate (built);
- `6ae0cbe9df28755d` DECIDED: opencode as the worker harness (deepseek v4 flash economics)
- `497abf139d49369e` Source integration (2026-08-06): prime-agent (RLM+Continual Harness) and Mastra Observational Memory (observer+reflector, dense observation log) — both researched, dispositions in wayfinder D8
- `98478c8b1daccdeb` ADOPTED: prototype rule 6 — capture prototype as primary source on a THROWAWAY BRANCH with a context pointer on the implementation issue; main keeps only the validated decision. Adds provenance.
- `8ffcf6eceee36cf2` ADOPTED: writing-great-skills renamed to writing-for-agents, scope widened to anything agents read (skills, AGENTS.md, CLAUDE.md, docs); ported doc-writing machinery (context pointers, two loads context/cognitive, information hierarchy, leading words, negation, no-ops test) as reference while KEEPING our kernel-seam machinery (contract, one-owner, hooks, versioning — which Matt lacks).
- `da7d898b4b623136` ADOPTED: to-questionnaire — new repo-local, hook-bound skill for decisions a THIRD PARTY holds: decision the user can't answer becomes a questionnaire DOC (async), grill only the SEND (who + what you need back), most-important-first, one idea per question, disable-model-invocation. Composes with domain-modeling.
- `3cb84b83835fb03b` ADOPTED: wait-what — new global mode skill for session clarity when a message did not land: re-pitch using context + ASD-STE100 simplified English + ubiquitous language from CONTEXT.md. Complements caveman; no hook needed.
- `1177064e1ab14cee` LAB-TEST: prototype LOGIC branch — build one shareable HTML prototype (free-play + tabbed walkthroughs, drivable by a non-developer) vs our TUI on the next logic-prototype task; no observed local TUI failure so no forced change.
- `e38ecd5bb62762ce` DEFERRED: wizard — trigger is the next multi-step HUMAN-ONLY provisioning (GH runner setup, domain/credential provisioning, CI secrets); boundary rule never-for-steps-the-agent-can-perform is its transferable mechanism.
- `4d375d777705576e` DEFERRED: research skill — trigger is an auto-researcher worker (AGI Phase 2); wayfinder track pattern covers research ad-hoc today.
- `3358e0f2e6ca0096` REJECTED: grilling rounds (our domain-modeling is already a superset, only missing non-blocking fact dispatch), and catalog/marketplace/distribution (popularity is not a mechanism; our enforced kernel registry with hooks/one-owner/versioning is deliberately stronger).
- `282a35001a555f06` Pipeline map (ORIENT/PLAN, DECIDE, RESEARCH, BUILD, VERIFY, KNOWLEDGE, ORCHESTRATE, MODE) shows the real gap is RESEARCH; name-overlaps were not where gaps were. to-questionnaire closed the only chain-break in DECIDE.
- `ccc01d856b49501a` Skill-layer changes are USER-governed (HITL): consumer and sole writer is the USER; the falsifier is gate-bound — mini-agi skill verify --all must stay 14+ PASS and one-owner lint must pass after any skill edit; to-questionnaire hook must verify the template + grill-the-send rule.
- `54652132a3129b58` D8 dispositions (2026-08-06) live per-decision in entry 2026-08-06-003 (F-001..F-010); this meta-summary is superseded by them
- `c93ad27aa8c7bb3a` deepseek-v4-flash API pricing (USD per 1M tokens, official DeepSeek pricing page, fetched 2026-08-06): input cache-hit $0.0028, input cache-miss $0.14, output $0.28. Source: https://api-docs.deepseek.com/quick_start/pricing
- `b9a0c323a1475dc2` The deepseek-v4-flash API alias currently points at model version DeepSeek-V4-Flash-0731 (confirmed on DeepSeek's pricing and 'Your First API Call' docs pages).
- `d5b02e7e2030b2b9` DeepSeek officially warns it plans to raise overall API pricing significantly in the near future, subject to official notice — so deepseek-v4-flash rates are time-sensitive and only valid until the pricing page changes.
- `e9e5cfc1d0b1386a` deepseek-v4-flash pricing has exactly three cells per model (cache-hit input, cache-miss input, output) — cache-hit vs cache-miss is the only split; there is no separate standard-vs-thinking input tier, though thinking mode is billed at the same input/output rates.
