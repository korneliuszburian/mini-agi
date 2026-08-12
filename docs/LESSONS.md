# LESSONS — mini-agi, what actually happened, and the rules that prevent it happening again

> Written 2026-08-12, at the `condense-core` marker (2792d5d). This document is the
> honest ledger of the whole arc: heavy hardening (17 defects, 616 tests, 16-step
> gate) → charter-gap audit → EXP-014/015/016 → EXP-017 dogfood on ekologus-3d that
> failed to improve a visual product → decisive condensation (13 modules cut,
> 616→45 tests, gate 16→9 steps). It exists because the owner asked, repeatedly and
> at the end: *"why did this turn into slop / a pathetic memory layer?"* and
> *"how is this supposed to help me work?"*
>
> Tone: direct, evidence-first, no consolation. Every failure mode cites the
> concrete symptom and the rule that would have caught it. The rules are meant to be
> checkable, not aspirational.

---

## 1. The recurring failure modes

Seven modes, roughly in the order they compounded. Each one is real and each one
cost real work. They are not seven separate mistakes; they are one mistake —
*verifiable machinery was treated as the deliverable instead of felt value* —
repeated at seven scales.

---

### FM-1. Building verifiable machinery instead of felt value ("the sandcastle")

- **Symptom.** The repo reached 616 tests, 39 MCP tools, a 16-step gate, 13
  modules, 10+ ADRs, an eval corpus, harness ledgers, ticket/lease lifecycles,
  MAST failure registers — and the owner's reaction to the whole thing was
  *"why did this turn into slop / a pathetic memory layer?"*. The system verified
  itself extensively and helped nobody measurably. The condensation removed 13
  modules and 571 tests because the measurement layer "protected nothing — the
  gate IS the verification" (`4b9445c`, `ARCHITECTURE-CONDENSED.md` §1).
- **Root cause.** A test that passes and a gate that goes green are cheap to
  manufacture and self-rewarding. Felt value ("does my work get done faster /
  better / without me repeating myself?") is hard to measure, so it was optimized
  away. The charter demands "mierzalny dowód" — measurable proof — and the system
  delivered measurable proof *of itself*, never of value to the owner. EXP-012/013
  measured a real advantage, but only in a synthetic blind-worker setup; everything
  else was the system grading its own homework.
- **Rule (checkable).** Every module, MCP tool, and gate step must name the
  *decision of the owner* it feeds (`PIPELINE-DESIGN.md` §5 table is the template).
  A tool with no named consumer is removed. A gate step with no red in N runs is
  cut. If a session's own docs cannot show the owner reading a piece, that piece is
  slop. When the owner asks *"how is this supposed to help me work?"*, stop adding
  and run the condensation pass — do not argue with the question.

---

### FM-2. Testing a company-scale system in a solo context

- **Symptom.** Multi-agent orchestration, ticket/lease registries, human signoff
  queues, per-domain memory fragments, shared-intelligence framing — machinery
  whose value scales with *N agents × N projects* — operated for one user in one
  repo. The charter's founding vision is a company-scale mini-AGI (Block/Sequoia
  framing), and the architect built for the vision while living in a solo daily
  reality. The same scale-blindness shows at the eval level: EXP-005/009/010/011
  ran controls designed for statistical comparison at sample sizes where the honest
  reading is "anecdote" (EXP-009 explicitly: "no success delta… under-powered, not
  wrong"; EXP-005: "a TWO-OBSERVATION ANECDOTE, not a causal conclusion").
- **Root cause.** Scale economies appear only at N>1. At N=1, coordination
  overhead (signoff ceremonies, review queues, claim lifecycles, derived-view
  regeneration) is pure tax. The memory layer's headline promise — knowledge given
  once works across projects and domains — was never exercised at the scale that
  would pay for it.
- **Rule (checkable).** Any feature whose value requires N>1 agents or N>1
  projects is backlog, not build, until the workload actually has N>1. Run the
  EXP-017 weekly cost audit (`usage-log.md`, W/C/B). If cost > 30 min/week with
  fewer than 2 attributable wins, cut ceremony before expanding — that is the
  pre-registered PARTIAL/STOP rule, and it must be treated as an execution
  decision, not a research question.

---

### FM-3. Dogfooding on the wrong substrate

- **Symptom.** EXP-017 dogfooded the kernel on `ekologus-3d`, a Three.js **visual**
  product. The kernel's only measured advantage is blind-worker verified-iteration
  driven by a *deterministic hidden verifier* (EXP-012/013). A visual product has
  no deterministic gate for "does the shape look right" — `npm run check`
  (typecheck + vitest + vite build) passes while the product is aesthetically
  wrong. The vision-judge loop could not fix the shape; a blind worker cannot see
  the shape; the owner watched a loop fail to improve a visual artifact and asked
  *"how is this supposed to help me work."*
- **Root cause.** The dogfood subject was chosen for "deliberately-complex build",
  not for *kernel-strength alignment*. The task's real difficulty was
  visual/aesthetic correctness, which has no verifier the kernel can hold; the
  pipeline's success criteria were gated on a verifier the product could not
  supply. The kernel was applied to the one class of work where its strengths do
  not apply — the exact inverse of the EXP-012/013 precondition (solo-below-bar +
  deterministic verifier the worker can't see).
- **Rule (checkable).** Before dogfooding or dispatching to any substrate, answer
  the pre-flight: *does this task have a deterministic verifier that (a) exists, (b)
  the worker cannot see, and (c) rejects genuinely wrong outputs?* If the substrate
  fails any of the three, the kernel adds only overhead there — use the host agent
  directly. Visual/aesthetic/UX work is explicitly out of scope for the verified
  loop; there is no `npm run check` for "looks right."

---

### FM-4. Blind work without a vision judge

- **Symptom.** `--blind-worker` (kernel hides the verifier's hidden suite during
  the worker run) is the flagship capability — the EXP-012/013 isolation made
  first-class. In EXP-017 the same blind loop was pointed at a visual product
  where the judge was absent or inadequate: the gate passed while the product
  stayed wrong. The loop dutifully iterated against the weakest check, producing
  verified passes of the wrong thing.
- **Root cause.** Blind-worker isolation is a strength **only** when the
  kernel-held verifier is a faithful oracle of the actual objective. The design
  never enforced "verify the verifier against the objective" (the verify-audit
  idea existed in `VERIFIABLE-REWARD-RESEARCH.md` §E but was never applied to the
  dogfood's real objective — the shape, not the build). Blindness plus a weak or
  misaligned judge equals resampling blind guesses at the ceiling of the weakest
  check.
- **Rule (checkable).** `--blind-worker` may be used only when the hidden
  verifier is a faithful oracle of the stated objective. Before every blind run,
  run the counterfactual: plant a visibly-broken deliverable and confirm the gate
  rejects it; plant a correct alternative and confirm the gate accepts it (EvalPlus
  rule). If the gate cannot distinguish wrong from right on the *objective*, blind
  work is forbidden on that substrate.

---

### FM-5. Over-verification that measures but doesn't decide

- **Symptom.** Composite scoring, judge calibration, mismatch registers,
  best-state bounds, health, metrics, audit, eval-gate — a self-consistent
  measurement layer that protected nothing. The condensation's own commit message
  names it: the 16→10 gate cut "reporting steps that protected nothing"; the 13
  modules cut were "measurement/verification slop". 616→45 tests is the arithmetic
  of a system that spent most of its budget proving its own correctness.
- **Root cause.** Producing numbers feels like progress without forcing a
  decision. Judge-drift precision, MAST classifications, and composite scores were
  signals nobody acted on; a signal with no decision-loop attached is theater. The
  only decision that ever mattered — close / retry / exhaust — is binary and is
  made by the gate, not by any metric.
- **Rule (checkable).** Every metric must name a decision it changes and the
  owner who reads it. For each metric ask: *"what would this number make me do
  differently?"* — if the answer is "nothing", the metric is cut. A gate step that
  never goes red in N runs is cut. Measurement is a privilege earned by changing a
  decision, never a monitoring entitlement.

---

### FM-6. Partial delivery / mid-refactor broken states

- **Symptom.** INCIDENTS #1–4: destructive commands fused with edits (the pkill
  killed the shell before the edit ran), `git checkout --` on the journal
  destroying a working-tree VERIFY-FAIL line (INCIDENT #2), `grep -c` chained with
  `&&` silently breaking verify/clippy chains (INCIDENT #3), and heredoc scripts
  aborting before the final write, losing whole edits twice (INCIDENT #4). And the
  condensation itself was delivered as one big-bang 34-file commit (`4b9445c`) —
  precisely the mid-refactor state that the checkpoint journal was built to
  prevent. Between the EXP-017 delivery and the condense, the repo sat carrying the
  old bloated machinery while the owner was already frustrated.
- **Root cause.** Cleanup done as a big-bang rather than as working slices, and
  operational sloppiness (fusing destructive commands with edits) under
  high-volume pressure. The checkpoint journal existed for exactly this class of
  state loss — and still experienced it, because the journal's own repair path was
  itself broken and had to be hand-repaired (INCIDENT #6).
- **Rule (checkable).** Big-bang refactors are forbidden; every cut must land as a
  working slice with the gate green at each step. The INCIDENTS rules are law, not
  style: never fuse destructive commands with edits in one shell line; never
  restore the journal through git; edit scripts that write files must write at the
  end. A condensation that cannot be sliced into green steps has not been designed
  yet.

---

### FM-7. The sandcastle staying machinery, not experience

- **Symptom.** The session consumed itself: 17-defect hardening cycles, the
  charter-gap audit, EXP-014 (memory zero-loss), EXP-015/016 (MCP portability) —
  then, last, a dogfood that failed. From the owner's side: a "pathetic memory
  layer" — canonical facts nobody used, tools nobody read, a system whose only
  real test was against its own machinery. EXP-017 was designed as a month-long
  experiment with a STOP criterion, but the failure was visible immediately (a
  blind worker cannot fix a shape), and the owner left before the verdict.
- **Root cause.** Sequence inversion: harden → measure → experiment → dogfood,
  when the only honest sequence is dogfood → harden only what the dogfood proves
  necessary. The pipeline's own business model is
  *research → knowledge → patterns → implementation*; the pipeline's development
  ignored it and built the cathedral before there was a bench to sit at.
- **Rule (checkable).** Real work precedes machinery, always. The first consumer
  of every new kernel feature is a piece of the owner's actual work with a
  deterministic gate; a feature that survives no real-work proving round is not
  built. When the owner's question is "how is this supposed to help me work", the
  answer is demonstrated, not explained — and if it cannot be demonstrated, the
  condensation pass runs that day.

---

## 2. The honest value proposition

### What mini-agi IS

- **A knowledge + verification layer**, not an agent. It owns memory
  (append-only canonical facts with provenance, derived views, fact ids
  sha256[:16] matching the PoC), deterministic verification (*verified before
  trusted*, ADR-0011: `outcome.achieved` is a claim until the declared gate runs),
  a pattern registry of skills with verify hooks, and a checkpoint journal.
- **Its one measured, reproducible advantage** is below-the-bar verified
  iteration (EXP-012/013, pre-registered, non-overlapping Wilson CIs):
  blind single-shot generation recovers via the kernel's hidden verifier +
  distilled failure feedback + bounded re-invocation. P 50%→25% vs K 100%→82.5%;
  below-bar subset total separation. That advantage exists **only** where all of
  these hold: (a) a deterministic hidden verifier the worker cannot see, (b) a
  blind worker, (c) solo capability below the bar.
- **Memory matters at multi-agent / multi-project scale**, not solo: the
  literature's memory gains are headroom-bound (Reflexion +11pp, AWM, Ledger
  +6–8pp) and the controls confirmed there is no headroom solo — EXP-010/011
  rejected all 7 generated task classes at 10/10 solo (~70 pre-registered runs).
  The kernel's honest role at N=1 is narrow: cross-session knowledge reuse and
  verified closure of gaps, at a measured ceremony cost.

### What mini-agi IS NOT

- **Not a product-maker.** It cannot fix a shape, a layout, or a design; EXP-017
  is the proof. Visual/aesthetic/UX correctness has no verifier, so the kernel's
  only advantage does not apply.
- **Not a general agent, not a speedup.** EXP-009 measured ~3.8× wall time at
  zero success delta where solo succeeds. The kernel never claims to beat solo
  codex on tasks solo solves — the pre-registration rejected that claim 7 times.
- **Not a self-justifying artifact.** 616 tests and 39 tools were slop; 45 tests
  and 14 tools are the honest size of a knowledge+verification layer for one user.
  The condensation is not a loss — it is the re-scope to what the kernel actually
  is.

---

## 3. Decision discipline — when to add, when to cut

The anti-slop gate. Every proposed piece of work or machinery answers three
questions; if it fails all three, it is cut:

1. **Does it feed a decision the owner actually reads?** (name the decision and
   the reader; "monitoring" is not a decision)
2. **Does it produce knowledge, patterns, or verified closure?** (a canonical
   fact with provenance; a skill with a verify hook; a gap closed by the gate)
3. **Is it justified by an observed failure reduction?** (the counterfactual
   harness gate — a claim with no observed failure is Phantom Guardrails)

**Add** only when: a substrate has a deterministic verifier that rejects wrong
outputs (verify-audit first); a tool has a named consumer; a metric changes a
decision; a feature's value exists at the actual N (agents/projects) of today.

**Cut** when any of: measurement no decision reads; gate step never red; tool
without a consumer; staging with no promotion receipt; review-queue traffic
manufactured to justify the signoff ceremony (an empty queue is health); a feature
whose value needs N>1 that the workload does not have; a dogfood substrate with no
verifiable objective.

---

## 4. "Do not repeat" checklist

For AGENTS.md / the condensed docs. Check every one on each new session; a red
line is a stop-and-report, not a note.

1. No new module, tool, or gate step without a named consumer and the owner
   decision it feeds.
2. No dogfood/dispatch on a substrate without a deterministic verifier that
   exists, is hidden from the worker, and rejects visibly-wrong outputs.
3. No `--blind-worker` without the planted-broken-solution counterfactual passing
   for the actual objective.
4. Real work precedes machinery — dogfood before hardening, never the reverse.
5. Metrics decide or die; a gate step that never goes red is cut.
6. No big-bang refactors; every cut lands as a green working slice.
7. No kernel-vs-plain advantage claims on tasks solo solves — pre-registration
   already rejected that 7×; do not re-litigate with new tasks.
8. Solo/above-bar: the kernel's role is knowledge reuse + verified closure, at a
   measured W/C/B cost; >30 min/week with <2 wins means cut ceremony.
9. Operational law (INCIDENTS): never fuse destructive commands with edits; never
   git-restore the journal; edit scripts write at the end, one fix per script.
10. When the owner asks "how is this supposed to help me work", demonstrate it on
    real work within the session — or run the condensation pass. Never answer the
    question with more machinery.
