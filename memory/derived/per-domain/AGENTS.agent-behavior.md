# PROVENANCE
# canonical_sha256: bb486b1e16d47524
# canonical_entries: 81
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# Domain: agent-behavior (derived from canonical memory)

Applies when working on this domain. Canonical memory wins on conflict.
- `a49b169111deb842` Behavioral guideline (Karpathy): think before coding — state assumptions explicitly, present multiple interpretations instead of picking silently, push back when a simpler approach exists, and stop + name the confusion when something is unclear. enforced_by: review rubric (misdirection/missing tradeoff = FIX-MINOR)
- `4992782a5790d742` Behavioral guideline (Karpathy): simplicity first — minimum code that solves the problem; no speculative features, no single-use abstractions, no unrequested flexibility/configurability, no error handling for impossible scenarios. enforced_by: review rubric (overengineering = FIX-MINOR)
- `fa58509d3523cc84` Behavioral guideline (Karpathy): surgical changes — touch only what the request demands; do not improve adjacent code/comments/formatting, do not refactor what is not broken, match existing style, and mention (not delete) unrelated dead code. enforced_by: review rubric (scope creep = FIX-MINOR)
- `a588d4401a139d71` Behavioral guideline (Karpathy): goal-driven execution — transform tasks into verifiable goals (validation -> tests first), state a brief per-step plan with a verify check per step, and loop only until verified. enforced_by: review rubric (claiming without evidence = REWORK)
