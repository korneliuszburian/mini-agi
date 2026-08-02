# PROVENANCE
# canonical_sha256: 8fb04615ad32f4af
# canonical_entries: 14
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# CONTEXT BRIEF (derived)

Read this before starting any session. Canonical wins over this file.

- `a49b169111deb842` [agent-behavior] Behavioral guideline (Karpathy): think before coding — state assumptions explicitly, present multiple interpretations instead of picking silently, push back when a simpler approach exists, and stop + name the confusion when something is unclear. enforced_by: review rubric (misdirection/missing tradeoff = FIX-MINOR)
- `4992782a5790d742` [agent-behavior] Behavioral guideline (Karpathy): simplicity first — minimum code that solves the problem; no speculative features, no single-use abstractions, no unrequested flexibility/configurability, no error handling for impossible scenarios. enforced_by: review rubric (overengineering = FIX-MINOR)
- `fa58509d3523cc84` [agent-behavior] Behavioral guideline (Karpathy): surgical changes — touch only what the request demands; do not improve adjacent code/comments/formatting, do not refactor what is not broken, match existing style, and mention (not delete) unrelated dead code. enforced_by: review rubric (scope creep = FIX-MINOR)
- `a588d4401a139d71` [agent-behavior] Behavioral guideline (Karpathy): goal-driven execution — transform tasks into verifiable goals (validation -> tests first), state a brief per-step plan with a verify check per step, and loop only until verified. enforced_by: review rubric (claiming without evidence = REWORK)
- `a573e7c2f8f88cae` [dogfood] the dogfood ticket proves the pipeline runs through the kernel CLI alone
- `4a5f57ac7c60c061` [strategy] Sequoia thesis (Dorsey & Botha, From Hierarchy to Intelligence, 2026-03-31): organizations built as intelligence, not hierarchy; AI replaces the information routing that middle management existed to provide.
- `649dd34541d01977` [strategy] Sequoia stack: capabilities (atomic, no UI, reliability targets) + world model (continuously updated from recorded actions) + intelligence layer (composes capabilities for specific moments, proactively) + interfaces (delivery surfaces only).
- `d7db7ff6b44040c9` [strategy] Sequoia feedback loop: when the intelligence layer cannot compose a solution because a capability is missing, that failure signal IS the roadmap — customer reality generates the backlog, not PMs.
- `6f4c17d5b59854e0` [strategy] Sequoia compounding test: "what does your company understand that is genuinely hard to understand, and is that understanding getting deeper every day?" — money is the honest signal for Block; for an agent kernel the honest signal is measured tokens/cost/score per run.
- `0d15dc7a5f566730` [eval] run real-ticket-008-v2 scored composite 0.9774 on 265897 tokens (0.6971 USD) with 0 scope violations and 0 tool mismatches.
- `6c7dd3b30d429ae4` [eval] run real-ticket-008-v2 is a strong run (composite >= 0.9).
- `24ca89466bb01359` [eval] run flailing scored composite 0.2851 on 9200 tokens (0.6100 USD) with 3 scope violations and 0 tool mismatches.
- `717835cda5492d7d` [eval] run harnessed scored composite 0.6141 on 2750 tokens (0.1800 USD) with 3 scope violations and 0 tool mismatches.
- `9cb30db774cf56e1` [eval] run reactive-loop scored composite 0.0000 on 14000 tokens (0.9300 USD) with 3 scope violations and 0 tool mismatches.
- `5491303b97d5f0bf` [eval] run reactive-loop is a failed run (composite 0.0).
- `9b114f49a878d2b0` [eval] run real-ticket-001-v2 scored composite 0.2402 on 106513 tokens (0.2772 USD) with 4 scope violations and 4 tool mismatches.
- `a08b4972b251ec1c` [eval] run real-ticket-002-v2 scored composite 0.2945 on 112411 tokens (0.2903 USD) with 3 scope violations and 4 tool mismatches.
- `0133392e91476d88` [eval] run real-ticket-003-v2 scored composite 0.3614 on 291156 tokens (0.7732 USD) with 0 scope violations and 6 tool mismatches.
- `1baffc8f38dd24b1` [eval] run real-ticket-004-v2 scored composite 0.4122 on 134212 tokens (0.3447 USD) with 0 scope violations and 5 tool mismatches.
- `2a08ff2b87eddc68` [eval] run real-ticket-005-v2 scored composite 0.4896 on 162500 tokens (0.4179 USD) with 0 scope violations and 4 tool mismatches.
- `552e4f04104ecf4d` [eval] run real-ticket-006-v2 scored composite 0.4437 on 126907 tokens (0.3322 USD) with 0 scope violations and 5 tool mismatches.
- `35295a4f074c3beb` [eval] run real-ticket-007-v2 scored composite 0.5220 on 1841123 tokens (1.3143 USD) with 0 scope violations and 4 tool mismatches.
