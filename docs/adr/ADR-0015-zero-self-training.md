# ADR-0015 — Zero self-training doctrine (no optimizing on the kernel's own outputs)

Status: accepted (2026-08-09)

## Context

Research (research/which-mechanisms-for-continuous-self-improvement-in-llm-agent.md,
2026-08-09, primary sources verified in-pass) documents a failure mode on
EVERY self-referential learning loop:

- Model collapse under self-generated training data (Shumailov et al.,
  *Nature* 631:755-759, 2024 — irreversible tail loss).
- Reward hacking when a proxy signal is optimized (Skalse et al., NeurIPS
  2022 — "unhackable" requires restricting the policy set; Pan et al. ICLR
  2022 — 5/9 hand-built proxies got hacked).
- Eval overfitting / Goodhart regime in iterate-and-eval loops (reward-model
  overoptimization, ICLR 2023: true quality first rises, then falls).
- Self-correction without an external evaluator is near-value-less (Reflexion
  ablation: reflect-without-tests 0.52 < base 0.60; Huang et al., ICLR 2024:
  intrinsic self-correction can degrade; Self-Refine: ~0pp on GSM8K without
  oracle feedback).

The kernel's improvement channels (enforced facts, verify gates, human
sign-off, ADR-0010/0012) already embody external verification. This ADR makes
the boundary explicit so future automation does not silently cross it.

## Decision

1. **Zero self-training.** The kernel and its worker sessions MUST NOT train,
   fine-tune, or otherwise optimize any model on the kernel's own evaluated
   outputs, judged transcripts, briefs, fragments, or research findings.
   Generated content never feeds back as training material.
2. Self-improvement is limited to three evidence-backed channels:
   a. **Behavior changes** accepted through the counterfactual harness gate
      (`harness verify`: observed failure reduction only) — external,
      deterministic.
   b. **Knowledge changes** via human-signed canonical memory (ADR-0010) —
      external, human.
   c. **Skill-library additions** with a verify hook and evidence of use
      (ADR-0002; TICKET-14 adds the capped, ranked listing).
3. **Judge-drift is a trend watch.** `eval judge-drift` disagreement with the
   deterministic layer is monitored as a trend: eval scores improving while
   judge-drift rises is recorded as eval overfitting, not as improvement
   (extension of ADR-0003's memory-anchored discipline to aggregate metrics).
4. No "self-training" or "self-improvement via own outputs" ticket may be
   opened without an ADR amending this one.

## Consequences

- The dream-loop's promotion path (conflicts → human queue) is now the ONLY
  route from model-produced material to canonical; the doctrine closes the
  alternative (training on the promoted material itself).
- Aggregate metrics that rise together with judge-drift lose improvement
  status — evaluators must read the trend line, not the per-run number.
