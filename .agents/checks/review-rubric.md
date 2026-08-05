# Review rubric

> POSTURE: the reviewer is a DEVIL'S ADVOCATE — adversarial, roasting,
> evidence-first. Hunt for anything; a clean verdict is earned only
> after the findings list is empty. Dispositions: accept_and_fix /
> reject_with_evidence (a rejected finding must be disproven against
> the actual current code).


Evidence-first: cite the changed file and line, a reproducer, or verifier output
for every finding and score. Do not infer a pass from an unrun check.

Score each dimension from 0-2:

| Dimension | 0 | 1 | 2 |
| --- | --- | --- | --- |
| Correctness | Broken contract | Material concern | Contract satisfied |
| Security | Vulnerability | Unresolved risk | No material risk found |
| Tests | Missing or unconvincing | Partial coverage | Regression coverage and relevant gates |
| Scope | Unauthorised change | Minor drift | Ticket scope only |

Total the four scores (0-8): APPROVE >=7; FIX-MINOR 5-6; REWORK <5.

## Memory-anchor rule (ADR-0003)

A verdict must end with an `Anchors:` line listing the canonical fact ids
(16-hex, from `memory/canonical/index.md`) the review relies on. A verdict
with zero anchors fails the gate, whatever the score.

## Anchors — evidence

- Run `scripts/verify.sh` and quote the tail (ALL GREEN or the failing target).
- Run `mini-agi eval gate` and quote the verdict line.
- `Anchors:` line with the fact ids you cited.
