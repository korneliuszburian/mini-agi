# ADR-0011 — verifiable reward layer: the kernel verifies outcomes

Status: accepted (2026-08-03)

## Context

The eval harness trusts `run.json`'s self-reported `outcome` — nothing
runs the work's own gate. The deep-research synthesis
(docs/RESEARCH-2026-08.md) is unanimous: verifiable rewards beat judged
ones for improvement loops (RLVR, arXiv 2501.12948; SWE-bench's
executable oracle, 2310.06770), judges drift and are lenient (OSReward,
2607.28609), and self-improvement loops without a deterministic gate
regress (RSIBench-Data, 2607.25886: 78% of continuing searches end
lower). Anthropic: "give the agent a check it can run."

## Decision

1. **Runs may declare a deterministic verifier** — two optional fields:
   `verify_command` (e.g. `make verify`, `npx tsc && node --test`) and
   `verify_target` (the target repo directory, absolute or relative to
   the kernel root).
2. **`mini-agi run verify <run.json>`** executes the command in the
   target repo and reports `verified` (gate passed AND outcome claims
   achieved), `verified-failed` (gate failed AND outcome claims failed),
   `disagrees` (gate and claim disagree — a judge-calibration signal;
   exit 1), or `unverified` (no verifier declared).
3. **`loop verify` closes a gap only when the composite reaches the
   target AND the verifier passes** (or none is declared). A
   self-reported outcome is not trusted when a verifier is available
   and disagrees.
4. **Trust boundary**: the kernel executes `verify_command` ONLY on
   explicit `run verify`/`loop verify` invocation — never during
   `eval score`/`eval gate` (those stay pure). Runs are trusted
   eval-corpus documents; the operator controls which runs get verified.
5. Scoring semantics are unchanged; verification is an orthogonal,
   additive signal. Backfilled: all 9 existing rerun cases now declare
   their real gates; all 9 verify PASS against their scratch repos.

## Consequences

- Every rerun in the corpus now carries executable proof of its
  outcome; `loop verify` refuses to close gaps whose claimed outcome is
  contradicted by the target repo's own gate.
- The disagreement signal (judge vs verifier) is the calibration data
  for the judged dimensions (Self-Taught Evaluators, 2408.02666).
- Existing runs without a verifier stay `unverified` — the judged
  composite remains, but the report says so explicitly.
