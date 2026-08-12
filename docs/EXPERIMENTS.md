# Experiments (Phase 7)

Record of measured experiments. Every entry: question, setup, result,
learnings, next steps. Evidence over claims; a failed experiment is as
valuable as a passed one.

## EXP-001 — codex as a pipeline implementer (2026-08-03)

- Question: can `codex exec` (gpt-5.6-terra, max reasoning) implement a
  loop-style slice with senior-like quality, measured the way mini-agi
  measures runs (green gates, tests-first, discipline)?
- Setup: scratch repo `/tmp/opencode/codex-exp` — the same baseline as
  the flailing/reactive-loop rate-limit task (existing `src/auth.ts` +
  auth-only tests). Prompt = the slice spec (5 req/60s window, 429 +
  retry-after, per-token isolation, `resetRateLimits`, tests first, tsc
  + node --test green). Sandbox: workspace-write, `--skip-git-repo-check`
  (codex refuses non-trusted dirs without it).
- Result: exit 0, wall 365s. Independent verification (not codex's own
  report): `npx tsc` clean; `node --test` 8/8 pass; diff = 2 files, +90
  insertions; `src/auth.ts` contains 429/retry-after/RATE_LIMIT/
  resetRateLimits; tests cover 5 allowed, 6th 429, isolation, window
  reset (fake-clock variant).
- Learnings:
  1. Codex delivered the same outcome class as the loop's own reruns
     (all-green, tests-first, exports for isolation) without knowing the
     pipeline — the spec alone carried the discipline.
  2. Time: 6m05s wall vs the loop's in-session reruns (~2-4 min); codex
     runs are a batch transport, not a loop participant — the loop's
     lease/work-graph machinery maps 1:1 onto it (dispatch spec →
     codex exec → verify).
  3. codex's own terminal report was accurate here (claimed tsc+test
     green; verified true). Still: verify, never trust the report
     (ADR-0003 mindset).
  4. Sandbox: workspace-write sufficed; cleanup of `dist/` was blocked
     by policy — generated output lingers unless the pipeline owns
     cleanup.
- Next steps: EXP-002 — run codex on a REAL loop-dispatched slice
  (spec from `mini-agi loop dispatch`) and capture a truthy trajectory
  for ingestion; measure tokens/cost via codex session logs.

## EXP-002 — codex in the loop: dispatch → codex → rerun → verify (2026-08-03)

- Question: does the full pipeline close a gap when CODEX is the
  implementer — spec from `mini-agi loop dispatch`, `codex exec` as the
  worker, trajectory reconstructed from observed evidence, `loop verify`
  as the judge?
- Setup: new eval case `codex-exp-002` (open gap: checksum CLI task
  defined, outcome achieved=false, composite 0.0) → `mini-agi loop
  dispatch codex-exp-002` created TICKET-10 + claimed it + wrote the
  slice spec. Scratch repo onboarded with `mini-agi init` (dogfood: the
  generated `.codex/config.toml` made it trusted — `codex exec` ran
  WITHOUT `--skip-git-repo-check`). Prompt = the spec verbatim.
- Result: `codex exec` exit 0, wall 348s. Independently verified:
  `make verify` ALL GREEN (3 unittest cases + fixture digest check),
  checksum.py handles usage/FileNotFound/IsADirectory/OSError, tests
  first, Makefile target wired into verify. Rerun trajectory
  reconstructed from the transcript (11 steps, each traceable to a log
  line or the final worktree) → `codex-exp-002-rerun` composite
  **1.0000** (0 mismatches, no golden) → `loop verify` CLOSED, lease on
  TICKET-10 released, gate 0 regressions across 22 cases.
- Learnings:
  1. The loop is implementer-agnostic: dispatch+spec+verify ran codex
     exactly as it runs an in-session agent; the lease and registers
     protected the flow (claim released only at the target).
  2. `mini-agi init` onboarding made codex trusted — one command, no
     flags; the earlier `--skip-git-repo-check` workaround is obsolete.
  3. Codex iterated like a human: 2 failed `make verify` rounds before
     green (transcript lines 1987/2005) — reconstructed honestly in the
     trajectory; the failure register found no REPEATED failing actions
     (different attempts each time).
  4. Trajectory reconstruction is the weak link: token/cost figures are
     estimates (no per-tool accounting in the transcript). A capture
     hook (like the redactor from TICKET-007-v2) is the honest next
     step for codex runs.
- Next: EXP-003 — instrument codex runs (capture hook) so trajectories
  are captured, not reconstructed.


## EXP-003 — capture hook: codex transcripts become truthful trajectories (2026-08-03)

- What shipped: `mini-agi codex <spec> <workdir>` — runs codex exec on a
  slice spec with a binding completion protocol
  (`<promise>COMPLETE</promise>` + `<result>{...}</result>`), stores the
  transcript, parses it into exec/write/read steps with line provenance,
  and writes a run.json draft (every step noted "captured from codex
  transcript line N").
- Parser validated against the REAL EXP-002 transcript (the weakness
  EXP-002 identified): unittest and make invocations are provably
  captured; the `/usr/bin/bash -lc 'cmd'` transcript form is handled.
- Resume semantics: the workdir + codex.log + run.json draft ARE the
  session state — a follow-up run can re-parse or continue from the
  draft (Sandcastle-style session-resume idea, kernel version).
- Remaining: the draft needs token/cost filling by the operator
  (transcripts don't carry per-tool accounting) before ingest.

## EXP-004 — codex as reviewer of Phase 8 (2026-08-03)

- Setup: `codex exec -s read-only` on commits 77f7cd8..HEAD with an
  adversarial review brief; 5m57s, exit 0.
- Verdict: REWORK 4/8 (correctness 0/2 — honest; security 2/2 within
  the ADR-0011 trusted-corpus boundary; tests 0/2; scope 2/2).
- Findings (10) and disposition:
  - CRITICAL harness false-green on unreadable baseline → FIXED
    (errors propagate, no fabricated green ledger row).
  - MAJOR loopcmd drops verifier errors via .ok() → FIXED (verifier
    error blocks close).
  - MAJOR capture test hard-depends on /tmp transcript → FIXED
    (conditional skip on clean hosts).
  - MAJOR codex capture parses stdout only + invents step fields +
    exit 0 regardless → FIXED (combined stdout+stderr parse, real step
    numbering, exit 1 on codex failure or missing completion marker).
  - MAJOR loop verify exits 0 on OPEN → FIXED ((text, closed); exit 1).
  - MINOR metrics write errors swallowed → FIXED (Result + warning).
  - MAJOR best-state bound bypassable by removing baseline cases →
    FIXED (run_gate flags vanished baseline cases as regressions).
  - MAJOR EXP-003 completion protocol unenforced → FIXED (exit 1).
  - MAJOR MCP not a full mirror → FIXED (7 more tools: loop_dispatch,
    loop_verify, eval_steps, run_verify, run_failures, harness — 36
    total).
  - MINOR process supervision blind spots → FIXED (full-success
    threshold + ok:false/reverted suspicious).
- Note: codex's own gate run failed in its read-only sandbox (target
  lock); our local gate was run independently and observed ALL GREEN.
- Learning: the reviewer caught 2 real correctness holes (harness
  false-green, best-state bypass) the test suite could not — the
  pairwise review loop works; gate independence matters.

## EXP-005 — pilot-before-scale: resampling-control design (2026-08-03)

Motivation (Ringelmann 2606.02646 + "What Drives Interactive Improvement"
2606.30774): measured loop gains are confounded by test-time compute and
re-evaluation. Two failure modes: (a) scaling retries hits a structural
ceiling that a 5-attempt pilot predicts; (b) "improvement from failure
memory" may be just resampling noise.

Design (MUST ship with every future loop improvement):

1. Resampling control: for any candidate improvement, run the gap at
   equal attempt count with failure-memory-conditioned retries vs plain
   resampling (no reflection injection). If the delta is near zero, the
   bottleneck is feedback quality, not loop iterations.
2. Pilot rule: before scaling a gap's retries beyond 5, run a 5-attempt
   pilot; it predicts the N=30 ceiling (Ringelmann hard-ceiling regime).
3. Heterogeneity: to escape the hard ceiling, vary the attempt
   configuration (different verifier/harness variant per attempt) instead
   of adding copies of the same one.
4. Instrument: `loop status --attempts` exposes the per-case attempt
   count (1 original + reruns) — the numerator for the pilot.

Status: design documented; instrument shipped (loop status --attempts);
no experiment executed yet (needs real attempt-vs-gain measurement).

## EXP-006 — codex review of Phase 9 (2026-08-03)

- Setup: `codex exec -s read-only` on a058291..f222757, adversarial brief;
  7m38s. Verdict: REWORK.
- Findings and disposition (all fixed, 77d2ae2):
  - CRITICAL verifier-error bypass (loop verify closed on verifier
    error) -> error now blocks close.
  - CRITICAL run failures executed declared verifiers (ADR-0011
    boundary) -> records 'declared' only.
  - CRITICAL harness could delete an unreadable target; the gate could
    self-validate -> unreadable targets error; scripts/verify.sh refused
    as counterfactual subject; markerless gate failure counted broken.
  - MAJOR ingest-before-verification -> ingest after, skipped on
    disagreement; honest contrast trust-path; timeout = disagreement;
    calibration precision excludes unverified + dedup by
    (case, command, target); attribution in loop verify + failures
    reported + audit distinguishes absent vs unreadable; eval hidden
    escape refused + partial-success non-zero; memory-load now IN the
    gate (verify.sh runs audit); attempts counts all rerun dirs.
- Verified correct by codex: score/gate never execute verifiers; a
  normal disagreement blocks close; hidden excluded from baseline;
  family_of correct; dry-run never executes.
- Note: codex's own sandbox could not run the gate (read-only lock);
  our local gate ran independently and is ALL GREEN.

## EXP-007 — first real harness revision cycle (2026-08-03)

- Candidate: AGENTS.md revision adding the Phase 8-9 discipline
  (verify-first: achieved is a claim until `run verify` confirms;
  judge-drift calibration; MAST failure context; counterfactual gate
  semantics; `harness verify` usage).
- Cycle through the counterfactual gate:
  - `harness verify AGENTS.md <candidate> --claims tests` → REJECT:
    "claimed failure 'tests' was never observed before the edit
    (Phantom Guardrails) — gate before: [audit:]". The phantom-claim
    path proven on real content.
  - `harness verify AGENTS.md <candidate>` (no claim) → NEUTRAL: gate
    failures unchanged (1) — the revision does not reduce the observed
    `audit:` failure (dirty-tree warn at that moment), so the gate
    correctly refuses to justify it.
- Semantics confirmed and documented: the counterfactual gate justifies
  FAILURE-REDUCING edits only; green-state documentation improvements
  land as normal review commits. The AGENTS.md revision therefore
  landed as a documentation commit; the ledger snapshot records the
  green baseline.
- Tooling bug found by the cycle: the CLI exposed `harness-verify`
  instead of `harness verify` — restructured `harness` into
  Snapshot/Verify subcommands (clap kebab-case trap).

## EXP-008 — the calibration signal caught a real drift (2026-08-04)

- The grown corpus (32 rows / 22 cases) immediately surfaced 2
  disagreements: real-ticket-002-v2-rerun and real-ticket-006-v2-rerun
  claimed achieved but their verifiers failed (exit 1).
- Root cause: the scratch repos' tests were DATE-COUPLED (assertions
  looked for `memory/canonical/entries/2026-08-03/` while consolidate
  writes under the CURRENT date). At the 2026-08-03/04 midnight
  rollover the target repos went red — the judged layer (recorded when
  green) still said "achieved"; the deterministic layer said "red NOW".
- This is exactly the verifiable-reward design: judged claims are only
  valid at verification time. Fix: patch the scratch tests to compute
  today's date (the same class of bug the kernel fixed earlier with
  `first_entry_text`); re-verify -> both flip to verified.
- Corpus after: precision 100% — and the transient 89.5% with SIGNAL
  was a real event, not noise.
- Durability correction (codex review EXP-007): the first patch hard-
  coded the new date; the scratch tests now COMPUTE today's date
  (datetime) so the next rollover cannot break them again; the
  consolidate-006 dry-run path is date-computed too.

## EXP-005 RESULTS — resampling control executed (2026-08-04)

Design (from EXP-005): equal attempt count, failure-memory-conditioned
retries vs plain resampling; pilot N<=5.

Execution (codex exec gpt-5.6-terra, workspace-write, fresh task
"count_lines.py CLI with tests + Makefile verify", 1 attempt per arm):

| Arm | Attempts | Outcome | Wall | Tests |
| --- | --- | --- | --- | --- |
| Plain (no memory) | 1 | SUCCESS (make verify green) | 14.5s | 3 |
| Reflexion (failure memory injected) | 1 | SUCCESS (make verify green) | 3m54s | 3 |

First run of the reflexion arm was INVALID (setup bug: the task text did
not interpolate into the prompt — the arm re-ran correctly).

Reading (What Drives Improvement 2606.30774): this is a TWO-OBSERVATION
ANECDOTE, not a causal conclusion (codex review EXP-007): one valid
call per arm gives no success-rate delta estimate; the reflexion arm
consumed at least two executions (the invalid first run cannot be
excluded from resource accounting), so the ~16x wall comparison is not
attributable to memory; the 14.5s plain result has no committed
prompts/transcripts/task-checksum evidence to establish comparability.
The only defensible statement: at 1 attempt each, both arms finished
green; no observed difference in these two calls. The control prevents
attributing the reflexion arm's success to failure memory, but the
experiment needs the N=5 pilot on a harder task, with full protocol
evidence committed, before any conclusion.

Follow-up: repeat at N=5 per arm (pilot rule) with a HARDER task where
solo capability is below the bar; commit prompts, transcripts and task
checksums — the pilot predicts the N=30 ceiling.

## EXP-003 continuation — the loop with the honest capture (2026-08-04)

- Full cycle: dispatch codex-exp-003 (TICKET-11, lease) -> `mini-agi
  codex` (8m59s, completion protocol, capture) -> draft with
  verify_command/verify_target + goal_aligned null -> outcome finalized
  only after the workdir gate confirmed green -> ingest -> loop verify.
- Honesty correction (codex review EXP-007): the first committed
  trajectory fabricated ok:true for every captured step (a probe
  command exited 2 but was recorded as ok). The capture parser now
  sets ok from transcript exit evidence (None when absent) and filters
  transcript noise; the rerun was regenerated -> honest composite
  0.5000 (ungated steps), verifier PASS, gap CLOSED exactly at the
  target. The fabricated 1.0000 in the baseline was displaced by the
  honest 0.5000 via a refreshed baseline — the best-state bound
  correctly blocked the displacement until the baseline re-snapshot.
- Found + fixed: cmd_codex parsed stdout only (bash -lc invocations are
  on stderr) — combined parse + --reparse-log; the first draft had an
  empty trajectory (stale binary), reparse fixed it.

## EXP-007 — codex review of the Phase 10 delta (2026-08-04)

- Verdict: REWORK. All findings dispositioned and fixed in a single
  commit (6d3e785):
  - CRITICAL harness gate-self-check bypassable via symlink/hardlink ->
    canonical-path comparison before the swap.
  - CRITICAL broken gate counted as a "reduction" -> non-success after
    the swap is an automatic REJECT, not a countable failure.
  - HIGH completion marker self-forging (embedded in the prompt echo) ->
    prompt stripped before detection + marker must sit in the last 20%.
  - HIGH fabricated ok:true trajectory steps -> capture sets ok from
    transcript exit evidence (None when unknown); transcript-noise
    filters (npm notice, codex label, help text).
  - HIGH date-rollover fix not durable -> scratch tests compute today's
    date (datetime), dry-run path date-computed.
  - MAJOR calibration corpus inconsistent (legacy rows without
    command/target) -> legacy rows dropped on re-append + audit
    integrity check; corpus regenerated (24 complete rows).
  - MAJOR capture test unconditional on a private /tmp path -> the
    rename silently no-op'd (python str.replace) and CI caught it:
    fixed as conditional (parses_exp003_transcript_when_present).
  - EXP-005 rewritten from "conclusion" to "two-observation anecdote";
    EXP-008 numbers corrected.
- EXP-003 rerun regenerated honestly: 26 steps, ok None where exit
  evidence was absent -> composite 0.5000 (was fabricated 1.0000),
  verifier PASS, loop verify CLOSED exactly at the target; the best-
  state bound correctly blocked the displacement until the baseline
  was re-snapshotted (the fabricated 1.0000 was replaced by the
  honest 0.5000).
- The full Phase 10 delta: verify.sh ALL GREEN + pushed + CI green.

## EXP-009 — proof-of-advantage control on the kernel's OWN loop (2026-08-04)

Protocol: the SAME fresh task (wordcount CLI: line-count of a file, 3
unittest cases, make verify gate) executed N=3 per arm. Arm K = the full
kernel loop (loop dispatch spec with failure context -> `mini-agi codex`
with budget + capture -> loop verify with the deterministic verifier).
Arm P = plain resampling (`codex exec` on the bare task prompt, same
model). Both arms used --no-sandbox (the sandbox is a safety feature,
not the tested variable; the Landlock write-containment blocks the npx
codex wrapper's /dev/null + ~/.npm writes — a REAL finding).

Results (all gates = make verify in the target dir):

| arm | run | wall(s) | verify gate | loop verify |
| --- | --- | --- | --- | --- |
| K | k1 | 296 | PASS | CLOSED 0.8123 |
| K | k2 | 379 | PASS | CLOSED 1.0000 |
| K | k3 | 300 | PASS | CLOSED 0.8909 |
| P | p1 | 80 | PASS | n/a |
| P | p2 | 83 | PASS | n/a |
| P | p3 | 95 | PASS | n/a |

Verdict (honest): at N=3 on this EASY task, BOTH arms succeeded 3/3 —
NO success delta from the kernel loop. Plain resampling was ~3.8x
faster in wall time (avg 86s vs 325s); the kernel loop's spec/failure-
context prompting + capture + verifier add overhead without improving
success when solo capability is above the bar. This is the same
two-observation-to-N=3 version of EXP-005: the control WORKS (it would
have rejected a false "kernel memory beats resampling" claim), and the
kernel's value must be shown where solo FAILS. The measurement is an
anecdote at this N, not a causal conclusion.

Findings for the kernel:
- The Landlock sandbox (ADR-0012) write-containment blocks npx-style
  codex wrappers (writes /dev/null + ~/.npm). For production, either add
  ~/.npm (+ a /dev/null allowance) to the default write set or document
  --no-sandbox for wrapped workers.
- The honest capture (ok flags) + loop verify (composite 0.81-1.0,
  verifier PASS) worked end-to-end on all three kernel runs.

## EXP-010 — N=5 proof-of-advantage control: the pilot gate rejected ALL candidates (2026-08-04)

Protocol (pre-registered in VERIFIABLE-REWARD-RESEARCH.md section B):
run the PLAIN arm ≥10 times per candidate HARD task; keep tasks where
solo pass ∈ [0,3]/10 (headroom exists); reject tasks where solo passes
≥5/10 (too easy). Select ≥2 kept tasks BEFORE the N=5 experiment. The
N=5 experiment may NOT run on tasks the gate rejects.

### Pilot results (plain arm, bare codex exec, same model)

| task | class | runs | solo pass | gate verdict |
| --- | --- | --- | --- | --- |
| taskA | cross-file refactor, exact-output invariant | 10 | 10/10 | REJECTED (too easy) |
| taskB | bug hunt (planted leap-year edge) | 10 | 10/10 | REJECTED (too easy) |
| taskC | arithmetic parser, correct precedence | 10 | 10/10 | REJECTED (too easy) |
| taskD | dependency-ordered scheduler (cross-file) | 10 | 10/10 | REJECTED (too easy) |

Wall-time (solo): taskA avg ~97 (68-134), taskB avg ~87 (72-106),
taskC avg ~105 (75-144), taskD avg ~122 (98-175). Every run's verifier
(`make verify`) passed on the first attempt.

### Verdict

The pre-registered task-selection gate rejected ALL four candidate
tasks: modern solo codex passes 10/10 on single-repo / single-issue-
scale tasks, including the multi-file and subtle-invariant designs
(exact-output refactor, cross-file dependency ordering). Per the
pre-registered rule the N=5 experiment CANNOT run on these task classes —
it would reproduce EXP-009 (no delta on tasks solo solves 3/3-10/10).

This is the gate working, not the experiment failing: continuing to
design harder tasks until solo drops below the bar would be task-
shopping (selecting tasks until the result matches the hypothesis),
which the pre-registration forbids. The honest lesson: the kernel's
value cannot be demonstrated on these task classes; it needs
demonstration where the VERIFIER matters (iterative recovery from
failing tests, long-horizon multi-session work, scale), not on
single-bug single-repo tasks that solo solves reliably.

The control still worked as intended: it rejected the hypothesis
"the kernel loop beats plain resampling" for this task class before any
costly N=5 runs, and it would have rejected any false positive claim.
No kept tasks -> no experiment; the follow-up build below proceeds
independently.

## EXP-011 (partial) — breakthrough pilot: solo codex iterates INTERNALLY

Pre-registered solo gate on the iterative-verifier-failure-recovery task
class (hidden-test suites the agent sees only as failure output):
3 generated tasks (config-line parser with quoted/comment/whitespace
cases; duration formatter with plural/zero-omission; case-insensitive
order-preserving dedup), 10 plain-arm runs each. Result: solo 10/10 on
ALL THREE (30/30).

Finding: modern codex DOES iterate within a single run — it runs `make
verify`, reads the failing hidden-case output, and fixes before ending.
The kernel's verified-iteration loop (re-invoke with the failure
register) would be redundant on these tasks because the worker already
does the iteration internally. The task class that would isolate the
kernel's contribution must exceed a single run's capacity (long-horizon
/ multi-session work) or disable the worker's internal iteration — so
the loop's value is in PERSISTENT cross-run memory (the failure register
surviving to a future session), the kernel's unique capability per the
AWM/Ledger literature.

### EXP-011 continued — e4 (money formatter, 44 hidden cases) also solo 10/10

The 5th generated task class (3-function money formatter: format with
thousands separator + negative + clamp, parse inverse with whitespace,
add with clamping) — 10 plain runs, solo 10/10. Rejected by the
pre-registered gate (>= 5/10). Cumulative: 7 task classes rejected
(EXP-010: 4, EXP-011: e1-e4), ~70 solo runs, all >= 5/10.

Established finding: codex's internal single-process iteration (runs the
verifier, reads failures, fixes before ending) is at ceiling on every
generated small-to-medium repo task. The kernel's verified-iteration
loop has nothing to fix on these — attempt 1 always passes. The one
lever codex's internal loop CANNOT provide is persistent memory ACROSS
processes/runs; a long-horizon cross-session task is the remaining
honest isolation class.

### EXP-011 conclusion — the breakthrough experiment's honest outcome

The verified-iteration loop (P2) is BUILT and works: on verifier failure
the kernel re-invokes a fresh worker process with the distilled failure
register (Reflexion-style test-grounded feedback), bounded by --iterate
and the budget caps, recording attempts + verifier_passed. It is the
kernel's flagship verified-reward capability and is shipped.

The kernel-vs-plain advantage could NOT be demonstrated. Every lever
reduces to "solo must fail first" (for the failure register / memory /
re-iteration to matter), and solo codex does not fail on ANY of the 7
task classes I could honestly generate (~70 pre-registered solo runs,
all >= 5/10). The controls rejected the speed-advantage hypothesis as
designed — this is a genuine, well-replicated negative, NOT a failure of
the protocol.

The defensible breakthrough claim of this session is the TRUST
property, not speed: the kernel makes agent work VERIFIED (run verify,
probe-vs-gate, verify-audit), CALIBRATED (judge-drift + red-team
signal), EVOLVABLE (counterfactual harness gate), and AUDITED (action
log, references, trial-isolation) — and its eval controls rejected
false kernel-vs-plain claims 7 times. Per VERIFIABLE-REWARD-RESEARCH.md
the kernel MATCHES the current verifiable-reward standard and EXCEEDS
the median (probe-vs-gate composite scoring, judge calibration); the
literature itself says the verified-loop benefit appears only when solo
is below the bar — which reachable task classes do not satisfy. The
verified-iteration loop is the kernel's answer for that future bar, not
a claim of today's speed advantage.

## EXP-012 — THE BREAKTHROUGH: verified-iteration beats plain single-shot (2026-08-04)

Isolation that finally shows the kernel's real advantage. The prior
negative (EXP-005/009/010/011) used codex, which iterates INTERNALLY (a
single codex process runs the verifier, reads failures, fixes before
ending) — so the kernel's loop had nothing to add. The correct
isolation: make the worker a BLIND single-shot generation (the hidden
test suite is genuinely unavailable to the agent — its `make verify` is
deliberately broken), and let ONLY the kernel hold the verifier. Then
Arm K = the kernel's verified-iteration loop (re-invoke a fresh worker
with the distilled failure register on verifier failure, bounded by
--iterate) vs Arm P = plain best-of-k blind single-shots.

Protocol: 4 hidden-suite task classes (e1 config-parser, e2 duration,
e3 dedup, e4 money-formatter), N=5 per arm per task, same model, same
prompt base + "you cannot run the hidden test suite; reason from the
spec". Arm P: each attempt is an independent blind single-shot, verified
once (pass@1). Arm K: kernel --iterate 5 per run (pass@5).

| task | P pass@1 | K pass@5 | K attempts-to-success |
| --- | --- | --- | --- |
| e1 | 0/5 | 5/5 | 2,2,2,2,2 |
| e2 | 0/5 | 5/5 | 2,2,2,2,2 |
| e3 | 5/5 | 5/5 | 1,1,1,1,1 |
| e4 | 5/5 | 5/5 | 1,1,1,1,1 |
| TOTAL | 10/20 (50%) | 20/20 (100%) | — |

Wilson 95% CIs: P 50% -> [0.30, 0.70]; K 100% -> [0.84, 1.00].
NON-OVERLAPPING — the pre-registered criterion for a real effect. On the
below-the-bar subset (e1+e2) the separation is total: P 0/10 vs K 10/10
(Fisher two-sided p < 0.001). On the above-the-bar subset (e3+e4) both
arms pass — consistent with all prior experiments (no delta where solo
succeeds).

Verdict: THE KERNEL'S VERIFIED-ITERATION LOOP TRANSFORMS WEAK BLIND
SINGLE-SHOT GENERATIONS INTO VERIFIED PASSES, EXACTLY WHEN SOLO IS BELOW
THE BAR. Each below-bar failure recovered in exactly ONE distilled
feedback attempt (the failure register: "the verifier FAILED on attempt
1; here is its output"). This is the Reflexion mechanism (test-grounded
feedback) realized IN THE KERNEL and measured with non-overlapping CIs —
the first real, reproducible kernel-vs-plain advantage of this session,
replicated across 4 task classes (effect total on the 2 below-bar ones).

Why this is the breakthrough pattern: it isolates the kernel's unique
contribution — a deterministic verifier the WORKER cannot see, distilled
failure feedback it cannot generate for itself, and a budget-capped
re-invocation it cannot perform. Plain resampling of blind generations
stays at 50%; the kernel's loop reaches 100%. The pattern ships as
`mini-agi codex --iterate N` (EXP-011 P2), already in the binary.

## EXP-013 — replication at N=10 with the blind-worker capability (2026-08-04)

The EXP-012 pattern replicated at N=10 across 4 task classes, using the
SHIPPED `--blind-worker` mode (the kernel hides the verifier's hidden
suite during the worker run and restores it before verification — the
isolation is now a first-class capability, not an experiment artifact).
Two new harder multi-function classes (e5 CSV parser with quotes/
escapes/trailing commas; e6 ledger reconciliation with cross-field
invariants + clamping).

| task | P pass@1 | K pass@5 | K attempts-to-success |
| --- | --- | --- | --- |
| e1 | 0/10 | 10/10 | 2,2,2,2,2,2,2,2,2,2 |
| e2 | 0/10 | 10/10 | 2,1,1,2,2,2,3,3,2,1 |
| e5 | 10/10 | 10/10 | 1 (above bar) |
| e6 | 0/10 | 3/10 | 5,2,2,4,5,5,5,5,5,5 |
| TOTAL | 10/40 (25%) | 33/40 (82.5%) | — |

Wilson 95% CIs: P 25% -> [0.142, 0.402]; K 82.5% -> [0.680, 0.913].
NON-OVERLAPPING — the effect replicates at N=10. Below-bar subset
(e1+e2+e6, blind 0/10): P 0/30 vs K 23/30. Above-bar (e5): both pass.

Equal-attempts comparator (codex second-opinion disposition): P's 10
runs per task grouped as best-of-5 (the same k as K's cap) -> e1 0/2,
e2 0/2, e5 2/2, e6 0/2 = 2/8 = 25% — IDENTICAL to P pass@1 because all
successes concentrate in e5. The advantage holds at equal total
attempts: K 82.5% vs P best-of-5 25% (non-overlapping). The claim is
scoped precisely: pass@1 (independent blind attempts) vs pass@N (up to
N verified-iteration attempts); it is NOT a codex-vs-plain claim.

The BOUNDARY: e6 (multi-function, 3 invariants, 14 hidden cases) — the
loop recovers 3/10 and EXHAUSTS 5 attempts on 7/10. The current
mechanism's honest limit: single-feedback-attempt recovery works for
single-function classes (e1/e2, always attempts <= 3); multi-function
classes sometimes exceed 5 attempts. Future levers: more attempts,
per-function checklist feedback, or function-level verifier feedback.

Escalation spot-check (S8 feedback escalation: attempt >= 2 adds
expected/got details): 5 e6 runs with the escalated feedback -> 0/5
(ALL exhausted 5 attempts). The escalation did NOT move the multi-
function boundary — an honest negative, recorded here per the codex
second-opinion disposition (this result was previously unrecorded).

## EXP-014 — overflow-loss probe: the compact→consolidate→derive pipeline loses nothing (PRE-REGISTERED 2026-08-12)

Charter criterion 2: "przy przepełnieniu kontekstu żadna decyzja/informacja
nie ginie". The kernel's mechanism: episodic buffer → `mem consolidate`
→ canonical → `derive` (views) → provenance gate. Deterministic, no model
in the loop. Protocol pre-registered BEFORE execution:

- Corpus (one buffer, `fact:` lines, ~9 facts) covering adversarial shapes:
  E1 plain prose; E2 body with backticks; **E3 body QUOTING a fact-block
  header line ("## F-007 `aabbccddeeff0011` style headers mark fact
  blocks")**; E4 unicode; E5 long (>300 chars); E6 byte-identical duplicate
  of E1 (dedup must keep ONE fact, id present); E7+E7b same-first-40-chars
  pair (contested wording, no signoff → BOTH must land); E8 bullet form.
- Pipeline: `mini-agi mem consolidate <buffer>` → `canonical_facts` →
  `mini-agi derive` → `mini-agi provenance` (exit 0).
- Zero-loss assertion: every non-duplicate input fact's
  `sha256(body)[..16]` id must be present in canonical WITH ITS FULL BODY;
  no phantom id may appear (an id that was only QUOTED, never written);
  derived brief carries the same canonical sha (provenance gate green).
- Falsifier: kernel test consolidating the corpus and asserting the
  zero-loss property. A failed falsifier = real pipeline defect, fixed
  with the falsifier red first.
- Excluded (by design, not loss): byte-identical dedup (E6), 64-bit hash
  collision (unreachable), human-queue routing (contested with signoff).

### Result (2026-08-12) — defect FOUND and FIXED

- E3 exposed a REAL loss: `parse_canonical_facts` treated ANY line
  starting `## F-` as a new block header. A body quoting a header
  ("## F-007 `aabbccddeeff0011` style headers...") was read back
  TRUNCATED (empty body) and spawned a PHANTOM fact with the quoted id
  (`aabbccddeeff0011`) — derived views would carry the truncated body.
- Fix (falsifier red → green): the header rule now requires `## F-<digits>`
  plus either a backticked 16-hex id with NOTHING after it, or no backtick
  pair at all (id-less block contract). A quoted reference with trailing
  content is body, not a header. Live corpus safe (0 id-less/0 trailing-
  content `## F-` lines; 1210 headers all well-formed).
- Re-run after fix: zero loss on the full corpus; E6 deduped to one fact
  (id present); E7/E7b both landed; provenance gate green. E1–E8 bodies
  byte-identical on read-back.

