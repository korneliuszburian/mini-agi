# Review rubric

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
