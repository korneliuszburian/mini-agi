# mini-agi — Verifiable-Reward Research (2026-08-04)
> STATUS: historical research record (2026-08-04); findings are snapshots from that date. Current state: docs/README.md.



Research-grounded assessment of the kernel's "verified before trusted"
pattern against the current verifiable-reward / agent-eval literature,
plus an honest proof-of-advantage methodology and the next build.
Companion to docs/HARDENING-AUDIT.md and PRODUCTION-READINESS.md.
Unconfirmed items are marked.

---

## A. Where the kernel sits in the verifiable-reward landscape

**Verdict: the pattern matches current research practice; on one axis it
exceeds the median; on two axes it lags.**

- **Matches:** deterministic test-suite verification as ground truth is
  the community standard (SWE-bench patch + FAIL_TO_PASS/PASS_TO_PASS;
  the RLVR line — DeepSeek-R1, Kimi k1.5, SWE-RL). "A run's outcome is
  its own claim until the verifier confirms it" is exactly the
  F2P/P2P philosophy.
- **Matches:** calibrating the LLM judge against a deterministic signal
  is validated, named practice — "Calibrate, Don't Curate"
  (arXiv:2605.09702): keep weak judges, learn their biases. The kernel's
  judge-drift + calibration corpus is the concrete realization; most
  production harnesses lack it.
- **Exceeds (marginally):** probe-vs-gate composite scoring mirrors the
  2025-26 composite/hybrid verifier work (R2E-Gym's complementary
  execution vs execution-free axes). Most harnesses still collapse to a
  single binary signal.
- **Lags (the honest part):**
  1. The kernel trusts the declared `verify_command` as-is — but the
     2026 evidence says the TEST SUITE, not the model, is where
     "verified" goes wrong: ~28.5% of a SWE-bench sample passes a
     Docker-verified incorrect patch (arXiv:2606.16062, preprint), 7.8%
     of counted-correct SWE-bench patches fail the developer suite
     (arXiv:2503.15223), one-in-five "solved" patches are semantically
     wrong (SWE-ABS, 2603.00520). **The kernel must verify the verifier.**
  2. Judge drift is measured as a rate; the field treats disagreement as
     an investigable proxy-overoptimization anomaly (Gao 2210.10760;
     Correlated Proxies 2403.03185; AI Control's trusted/untrusted
     monitors, 2312.06942), not just a dashboard metric.
  3. No contamination/memorization guard on the verify target (SWE-bench
     Illusion, 2506.12286 — models ID buggy file paths at 76% from
     issue text).

**Hype vs evidence flags:** the 2026 suite-weakness numbers are
single-paper preprints (replicate before treating as calibration
targets); OpenAI's Codex-RL execution-verifier claims were unfetchable
(404) — unconfirmed.

---

## B. Proof-of-advantage: EXP-009 interpretation

**Verdict: "no delta on an easy task" is EXPECTED, and the 3.8x time
cost is the robust finding. EXP-009 was under-powered, not wrong.**

- Loop benefit is bounded by headroom = (verifier-grounded ceiling) −
  (solo pass rate). Reflexion's +11pp on HumanEval exists only because
  solo GPT-4 fails 20% of tasks. On a task plain resampling solves 3/3,
  the loop's only measurable effect is overhead.
- Documented, not just plausible: E3 (arXiv:2607.13034) — a maximal
  loop on simple tasks keeps 100% success at 85% more budget / 91% more
  tokens. CORVUS (2607.22711) cuts context 9-50% at equal pass rates.
- METR's RCT: AI tooling made developers ~19% slower on their own repos;
  RE-Bench finds plain best-of-k resampling is a strong, cheap
  competitor — confirming Arm P is the right baseline and the time/cost
  delta is the honest output.
- Under-powering: at N=3, only total separation (0/3 vs 3/3) is
  detectable (Fisher two-sided p≈0.10). "No delta" is a ceiling check,
  NOT evidence against the kernel.
- Nuance: self-critique loops can actively hurt (arXiv:2310.08118) —
  the kernel's choice of an EXTERNAL deterministic verifier over LLM
  self-judging is the design the literature supports.

### The N=5 harder-task protocol (pre-registered, pilot-gated)

1. **Task selection (pilot-gated):** pilot the PLAIN arm, ≥10 runs/task.
   Keep tasks where solo pass ∈ [0,3]/10 (headroom exists). Reject 0/10
   only when the verifier shows the task is broken (RE-Bench rule:
   humans must be able to make progress). Require the deterministic
   verifier to REJECT wrong outputs (EvalPlus: weak tests inflate
   pass@k by up to 19-29pp). Require multi-step/multi-file work
   (HumanEval-style one-shots are the wrong class). Recruit ~10 pilots,
   drop to 5 by the pre-registered rule.
2. **Measurement:** paired same-task-per-arm, ≥5 runs per task per arm.
   Metrics: pass@1 with Wilson CI, pass@5 (best-of-k — the resampling
   competitor), and time-to-success + token cost as co-primary
   (continuous — statistically tractable at N=5; report median + range,
   paired Wilcoxon). Pre-register pass thresholds before running.
3. **Controls:** same model/temperature both arms (variance persists at
   temp 0 — arXiv:2602.07150), same system prompt, counterbalanced
   order, verifier blind to arm, full input snapshotting, divergence
   logs.
4. **Interpretation rule:** report "directional, not significant" unless
   total separation (5/5 vs 0/5: p≈0.008) or a continuous-metric gap
   with non-overlapping CIs; treat time/cost as primary, not success.

---

## C. Memory value (when to inject failure context)

- **Benefit is real but headroom-bound:** Reflexion +11pp (episodic
  failure memory), AWM +24.6/51.1% relative with gains widening as the
  distribution gap widens (memory pays exactly when solo can't lean on
  memorized routines), Ledger +6-8pp at 29-32% lower cost (memory-as-
  state, not raw history).
- **Cost is load-bearing:** one irrelevant clause drops math accuracy up
  to 65% (GSM-Symbolic 2410.05229); Lost-in-the-Middle (2307.03172) —
  pin task spec + decisive failure-memory to the START/END of injected
  context, never a raw dump mid-context; over-anchoring is documented —
  preserved failure traces "can constrain a capable agent from stepping
  outside the prior-run box" (ARA region paper).
- **Kernel guidance:**
  - Inject COMPACT, high-precision context (state summaries over full
    histories), positioned at start/end.
  - The failure-register format matters: distilled
    "this failed, here's the deterministic check that caught it" entries
    (Reflexion/AWM-style) beat verbatim logs. The kernel's MAST +
    one-line reflections already trend this way.
  - Memory is mostly overhead for agents that finish quickly — the
    failure-memory injection in `loop dispatch` specs should be short
    and only for cases with recorded failures (already the case).

---

## D. Eval hygiene to adopt

- **Fresh-task ledger:** every eval task must be authored/derived
  in-repo, never scraped from a public issue (contamination via
  pretraining, DeepSWE 2607.07946).
- **Verifier-quality audit (THE next build):** the verifier must reject
  a planted broken solution and accept an alternative correct one — not
  just the reference (EvalPlus; METR "Algorithmic vs Holistic").
- **Variance reporting:** pass@1 from ≥3 runs with CI + pass@k and
  pass^k envelopes (2602.07150); state the power analysis.
- **Saturation masking:** track per-failure-type + a shrinking-budget
  diagnostic, not just aggregate outcome ("Flat Score, Amplified
  Failures" 2607.27275).
- **Judge shortcut probes:** cue-injection probes on the judge with
  acknowledgment requirements (2602.07996); keep the deterministic layer
  authoritative.
- **Canary strings** in task specs + periodic audit for output that
  satisfies the verifier without doing the work (METR MALT).

---

## E. Recommended next build (implemented in this goal)

**Verifier-strength audit — `run verify-audit <run.json>`.** Before
trusting a declared `verify_command`, the kernel checks the verifier is
not vacuous: it must (a) pass on the real target (the known-good work)
and (b) FAIL on a counterfactual target where the deliverables are
missing/broken. A verifier that "passes" empty work is a fake gate.
Rationale: the 2026 literature's core finding — the test suite, not the
model, is where "verified" goes wrong; the kernel must verify the
verifier. Recorded FPR/FNR per target in the calibration corpus.

Follow-ups (listed, not implemented): calibrated multi-judge pool with
learned per-judge bias; differential/mutation test-strengthening in run
verify; disagreement-as-red-team-signal feeding loop dispatch;
process/diagnostic supervision in the run record; fresh-task ledger +
canary strings.

## Status
Grounded in the current worktree and the fetched sources
(arXiv/ICLR/NeurIPS/METR/Anthropic). Unconfirmed items marked. The
recommended build (verify-audit) ships as the slice in this goal;
remaining items are tracked as follow-up tickets.

## Addendum (2026-08-04) — breakthrough experiment outcome

The verified-iteration loop (BREAKTHROUGH P2) shipped: `mini-agi codex
--iterate N` re-invokes a fresh worker on verifier failure with the
distilled failure register, bounded by budget caps, recording the
attempt chain. The kernel-vs-plain advantage could not be demonstrated:
7 task classes, ~70 pre-registered solo runs, all >= 5/10 — solo codex's
internal single-process iteration is at ceiling on every generated
small-to-medium repo task. The pre-registered gates rejected the speed-
advantage hypothesis as designed. The session's defensible breakthrough
is the trust property (verified/calibrated/evolvable/audited agent
work) and the honest eval-control methodology, both evidenced.

## Addendum (2026-08-04) — THE BREAKTHROUGH: EXP-012

The verified-iteration loop (`mini-agi codex --iterate N`) BEATS plain
resampling when the worker is a blind single-shot generation that cannot
self-iterate. Isolated: P (blind best-of-k) 10/20 = 50% (Wilson CI
[0.30, 0.70]) vs K (kernel loop) 20/20 = 100% (CI [0.84, 1.00]) — NON-
OVERLAPPING CIs. Below-bar subset: P 0/10 vs K 10/10 (total separation,
p < 0.001), each failure recovered in exactly one distilled-feedback
attempt. Replicated across 4 task classes. This is the first real
kernel-vs-plain advantage of the session — the kernel's deterministic
verifier + distilled failure feedback + bounded re-invocation transform
weak generations into verified passes exactly where solo is below the
bar, confirming the literature's headroom prediction (Reflexion/AWM).

## Addendum (2026-08-05) — S8 research: improving the verified-iteration boundary

Grounded upgrades from a dedicated research pass (fetched, cited):
- FEEDBACK QUALITY IS THE BOTTLENECK, not capacity: repair gains are
  driven by the feedback's content, not the model (Olausson 2306.09896:
  artificially stronger feedback gives 'substantially larger' gains;
  Falsification-Not-Exposure 2606.31511: code-plus-facts beats a
  generic-bullet placebo, p=0.0041). The kernel's per-case checklist is
  the right lever; escalate specificity across attempts — attempt 1:
  checklist; attempt 2: checklist + expected/actual + traceback line;
  attempt 3: block-level runtime trace (LDB 2402.16906; Self-Debug
  2304.05128; LeTI 2305.10314).
- FEW HIGH-QUALITY ATTEMPTS, THEN SWITCH: repair worth ~1-3 feedback
  attempts (Olausson; Huang 2310.01798: self-correction without external
  feedback degrades), then parallel sampling+filtering (AlphaCode
  2203.07814; Falsification: blind resampling TIED stalled repair at
  matched budget). s1 budget-forcing (2501.19393) + difficulty-adaptive
  allocation (Snell 2408.03314) justify a mode-switch governor: after 2
  failed repairs, switch to N fresh blind workers filtered by the
  verifier.
- MULTI-FUNCTION BOUNDARY (the e6 failure): documented as the hard case
  (SWE-bench 1.96%; BigCodeBench <=60% vs 97% human; LiveCodeBench Pro
  0% hard). The evidence-backed treatment is per-function test
  partitioning (FunPRM 2601.22249 function-as-step; CodePlan 2309.12499
  staged validation; AlphaCodium 2401.08500) — map failing cases to
  functions via coverage, repair per function, re-verify composite.
- THE PATTERN'S PLACE: no canonical paper names the 'blind worker +
  kernel-held hidden verifier + distilled failing-case feedback'
  construction — it appears to be novel in this exact form; closest
  relatives are oracle-guided APR and Prover-Verifier Games (2108.12099,
  which argues FOR keeping the verifier deterministic and outside the
  worker — exactly the design). Verifier completeness is a real
  property (EvalPlus: small suites overstate pass@k by up to 29%) — the
  kernel's verify-audit + EXP-012/013 (P 25-50% vs K 82.5-100%,
  non-overlapping CIs, below-bar total separation) carry the evidence.
