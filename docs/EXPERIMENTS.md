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
