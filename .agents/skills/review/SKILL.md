---
name: review
description: Rubric-based code review with default-to-action. Scores correctness, security, tests, scope 0-2 each, and MUST cite canonical memory anchors (ADR-0003). Use when a change is ready for review or before merge.
verify: .agents/checks/review-anchor-test.sh
version: 1.0.0
source: mini-agi repo (.agents/skills)
---

# Review

SEAM: kernel-bound review (mini-agi rubric, memory anchors, ADR-0003).
The generic two-axis review (Standards + Spec) is the separate
`code-review` skill — use code-review for generic diffs, this skill
for mini-agi-gated work. Read `.agents/checks/review-rubric.md`
first — it contains the memory-anchor rule that binds this skill.

1. Read the diff since the last green checkpoint.
2. Score each dimension 0-2:
   - Correctness: does it do what the spec says? edge cases?
   - Security: injections, secrets, authz, least-privilege tool use?
   - Tests: did tests actually run (quote output)? do they cover the change?
   - Scope: only assigned files changed? blast radius bounded?
3. Verdict: APPROVE >=7, FIX-MINOR 5-6, REWORK <5.
4. Memory-anchor gate (ADR-0003): end the verdict with an `Anchors:` line
   listing the canonical fact ids (16-hex, from
   `memory/canonical/index.md`) the review relies on. Zero anchors = the
   review fails, whatever the score. The anchor list must come from the
   canonical index — never invent or reuse ids from the diff.
5. Default to action:
   - APPROVE -> report ready.
   - FIX-MINOR -> provide the exact patch for each finding.
   - REWORK -> stop, report to human with the rubric sheet filled in.
6. Lead with findings (file:line). No style-only comments. No praise.
   Evidence first: cite the changed file/line, a reproducer, or verifier
   output for every finding. Do not infer a pass from an unrun check.

Reviews are fresh-session and independent; an implementer self-review is
not independent evidence.

## Completion criteria

- [ ] The diff since the last green checkpoint was read, not skimmed from
      commit messages.
- [ ] All four dimensions scored 0-2 with one quoted evidence per score.
- [ ] Verifier output (scripts/verify.sh tail) is quoted in the report.
- [ ] Verdict line is one of APPROVE / FIX-MINOR / REWORK with the total.
- [ ] `Anchors:` line present, only ids from the canonical index.
- [ ] FIX-MINOR findings each carry an exact patch; REWORK stops and goes
      to a human.
