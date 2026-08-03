# RESEARCH 2026-08-03 — breakthrough pipeline: sources mapped to mini-agi

Deep research across 5 parallel tracks: (1) our architecture map,
(2) Karpathy + frontier practice, (3) scientific papers (memory/eval/
self-improvement), (4) industry SOTA + Sandcastle + Matt Pocock,
(5) evaluation science. Every idea below has a primary source; every
lift is mapped to a concrete seam in our codebase.

## 1. Our architecture (map + honest weaknesses)

Kernel modules: memory/store (canonical fact ids sha256[:16] + derived),
eval (4-dim scoring + gate), journal (T008 audit), ticket (work graph +
claims lease + lock), loopcmd (status/dispatch/verify), failure +
mismatch registers, health + audit, insights/backlog/resume, skills,
contract, MCP server. Data: memory/canonical|derived|episodic, evals/
cases|golden|baseline, tickets/claims.md, artifacts/spec, scripts/verify.

Weaknesses found (with file:line):
- Self-reported outcomes: eval.rs:235-250 trusts run.outcome; the spec's
  "target repo verify.sh ALL GREEN" (loopcmd.rs:247) is prose — the
  kernel never runs the target-repo verification itself.
- Trajectory capture fidelity unenforced (run.json authored externally).
- Duplicated ticket numbering (insights.rs:410 vs loopcmd.rs:324);
  `backlog()` without claims lock; `consolidate` without lock.
- Scattered thresholds (0.5 / 0.05 / tolerance).
- Resume truncates registers to 5; advisory, not enforced.
- MCP mirror partial (7 tools); single-root assumption.

## 2. Karpathy + frontier practice (17 ideas, top lifts)

- **Context engineering > prompt engineering; context rot is real.**
  Smallest high-signal token set per call; JIT retrieval over preload.
  (anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- **"Give the agent a check it can run"** — tests/build = the difference
  between supervised and unattended work; fresh-context reviewer subagent
  ("the worker isn't the grader"). (code.claude.com claude-code-best-practices)
- **Verifiability is the axis of automation** — "Software 1.0 automates
  what you can specify; Software 2.0 what you can verify"; RLVR made 2025
  models reason. (karpathy.bearblog.dev/verifiability)
- **Simple composable patterns > frameworks; invest in the ACI as much as
  the prompt** (Anthropic spent more on tools than prompts for SWE-bench).
- **Multi-agent = 15x token burn; coding is sequential — single-agent
  loops remain right.** (anthropic.com/engineering/built-multi-agent-research-system)
- **Start evals with ~20 queries; evaluate end state, not process.**
- **Agents improve their own tools/context** (tool-testing agent cut task
  time 40%); "benchmaxxing" warning — build YOUR OWN verifiable
  environment; capability ≈ verifiability × attention × coverage × value.
- **Dec 2025 = agentic inflection; Software 3.0: the context window is
  the program; agent-native infra is the moat** (agent-legible docs/APIs).
- **The append-and-review note** (Karpathy) — exactly our canonical+
  review shape.

## 3. Papers (top lifts, arXiv)

- **MemGPT (2310.08560)** — memory paging as an AGENT decision: add a
  working-memory tier with page-in/page-out actions to the action space.
- **Generative Agents (2304.03442)** — retrieval = recency × importance ×
  relevance + scheduled reflection distilling insights into canonical.
- **A-MEM (2502.12110)** — fact-linking pass on append: derive cross-fact
  links, re-contextualize DERIVED views only (canonical stays append-only).
- **CoALA (2309.02427)** — classify our stores (working/episodic/semantic/
  procedural); memory ops as first-class internal actions.
- **Reflexion (2303.11366)** — verbal self-reflection per failure, stored
  and injected into the next attempt. CHEAPEST high-ROI change: our
  failure register + rerun context.
- **Voyager (2305.16291)** — versioned skill cards (code + verification)
  composed by the loop; automatic curriculum by difficulty.
- **STaR/V-STaR (2203.14465 / 2402.06457)** — failed trajectory vs fixed
  slice = ready-made DPO pair corpus; verifier ranks candidate slices.
- **SWE-bench (2310.06770)** — build mini-SWE-bench from OUR git history
  (regression commits + fixing slices): executable oracle for the judged
  dimensions.
- **Agent-as-a-Judge (2410.10934)** + **Let's Verify Step by Step
  (2305.20050)** — step-level process supervision; active learning picks
  WHICH steps to verify (judge budget where scorer is most uncertain).
- **Self-Taught Evaluators (2408.02666)** + **RLVR (2501.12948)** — split
  reward into verifiable (deterministic) + judged; disagreement = judge
  training data. THE highest-leverage eval move.
- **MAST (2503.13657)** — 14 failure modes as the canonical failure
  taxonomy for our mismatch register; kappa-gate the annotator.
- **AlphaEvolve (2506.13131)** — elitist evolution: population of candidate
  slices per gap, strict regression gate, journal as the evolution ledger.
- **Auto-Dreamer (2605.20616)** + **TrustMem (2606.25161)** — offline
  consolidation with supersede-pointers; verifier-before-write on derived
  regeneration.
- **RSIBench-Data (2607.25886)** — THE empirical study of our exact loop:
  78% of continuing searches end lower; enforce best-state preservation
  with a regression bound.
- **KSI (2607.19592)** — improvement must land in the KNOWLEDGE base
  (transferable), not the agent; attribute score deltas to knowledge-state
  changes.
- **RHI (2607.15524)** — harness as data: versioned prompt/harness specs,
  pairwise revision eval over a frozen suite; checkpoint journal = the
  revision substrate.
- **RLSVR (2607.23802)** + **s1 budget forcing (2501.19393)** — decompose
  goals into verifiable sub-checks; scale judge effort with difficulty.

## 4. Industry SOTA + Sandcastle + Matt Pocock

- **Sandcastle (github.com/mattpocock/sandcastle, ~7.2k stars)** —
  Matt Pocock's TS library orchestrating Claude Code/Codex/Cursor/opencode
  in isolated sandboxes. Directly liftable ideas:
  - **Branch strategies as sandbox semantics** (head / merge-to-head /
    branch; git worktrees bind-mounted into containers; commits merged
    back) — maps onto our loop's lease/work-graph.
  - **Deterministic completion signaling** — agent emits
    `<promise>COMPLETE</promise>` to end the loop early; separate idle and
    completion timeouts (stuck vs hanging-child).
  - **Structured output** — XML-tagged JSON, schema-validated, extracted
    to result.output; on failure `maxRetries` resumes the SAME session.
  - **Lifecycle hooks + warm reusable sandboxes** (onWorktreeReady,
    onSandboxReady; exec() gates: implement -> `npm test` gate -> review);
    session resume/fork.
  - This is the missing EXP-003 machinery: capture + completion protocol
    + sandbox lifecycle for codex runs.
- **Matt Pocock** — "7 Phases of AI Development" (grill-me -> research.md
  cached in-repo -> prototype variants -> PRD -> kanban with blocking ->
  execution -> QA loop) — our pipeline already mirrors it; "Three Types
  Of Evals" (deterministic pass/fail > LLM-judge > human) and Evalite
  scorers (exactMatch/contains/levenshtein, faithfulness, toolCallAccuracy)
  are a scorer menu for our judged dimensions.
- **Industry**: Anthropic workflows (prompt chaining, routing, parallel,
  orchestrator-workers, evaluator-optimizer), OpenAI Agents SDK guardrails
  (parallel fail-fast input/output validation), Google ADK (context as
  source code, session rewind, eval suite), MCP as the second surface,
  observability (LangSmith/Braintrust/Evalite), Letta "Context
  Repositories" (git-based memory — our canonical is already git-versioned).

## 5. Synthesis — the 3 structural moves

1. **Verifiable-vs-judged split + kernel-run verification.** Stop trusting
   self-reported outcomes: the kernel runs the target repo's own gate
   (deterministic verifiable layer) and uses disagreement with the judged
   composite as judge-calibration data. Fixes our #1 weakness; grounded in
   RLVR + Self-Taught Evaluators + OSReward leniency-bias findings.
2. **Reflexion in the loop.** Structured verbal reflections in the failure
   register, injected into rerun context; step-level process supervision
   with active budget on divergent steps; MAST taxonomy for the mismatch
   register. Cheap, paper-backed, touches our two weakest surfaces.
3. **Harness + memory self-improvement (mini-agi improving mini-agi).**
   Versioned harness/prompt specs with pairwise eval over the frozen
   suite (RHI), knowledge-state attribution (KSI), best-state regression
   bound (RSIBench), offline consolidation with supersede semantics
   (Auto-Dreamer) — with the checkpoint journal as the evolution ledger
   (AlphaEvolve) and Sandcastle-style sandbox/completion machinery as the
   codex integration layer (EXP-003).

## Phase 8 candidate roadmap (ranked by leverage/effort)

1. **Verifiable reward layer** — `loop verify` runs the target repo's own
   gate; outcome dimension gains a deterministic component; disagreement
   report (judge vs verifier). (medium effort, highest trust gain)
2. **Reflexion register upgrade** — failure entries gain reflection +
   MAST classification; rerun dispatch injects top-K reflections. (small)
3. **Sandcastle-style codex integration (EXP-003)** — completion
   protocol, structured output schema, branch/merge semantics, capture
   hook for truthful trajectories. (medium; unblocks codex trajectories)
4. **Process supervision** — per-step verdicts + active selection of
   divergent steps; step-level judge budget. (medium)
5. **Best-state regression bound** — accept slices only when they beat
   the frozen suite; published pass-rate time series (Compounding Test
   discipline). (small-medium)
6. **Memory evolution** — fact-linking pass, importance-weighted
   retrieval, offline consolidation with supersede pointers, retrieval-
   confidence gates. (medium-large)
7. **Harness evolution** — versioned harness specs + pairwise eval;
   mini-agi improving mini-agi with the journal as ledger. (large, the
   AlphaEvolve endgame)
8. **Multi-repo + MCP completion** — multi-root support, full tool mirror.

Phase 8 status (2026-08-03): slices 1-8 implemented (verifiable
reward layer ADR-0011, Reflexion+MAST, regression bound+METRICS, codex
capture hook EXP-003, process supervision, fact-linking, harness
evolution, MCP completion); codex review of the delta (EXP-004) returned
REWORK 4/8 — all 10 findings dispositioned and fixed (harness
false-green, verifier-error bypass, best-state bypass, capture honesty,
exit codes, MCP mirror). Multi-root (AGENTIC_ROOT list) remains the only
open sub-item of slice 8.

Sources ledger: all arXiv IDs verified against primary pages; AlphaEvolve
= 2506.13131 (2506.13120 is an unrelated PDE paper); OpenAI agent-practices
post removed from the web (characterizations flagged); Sequoia
"Compounding Test" unverifiable (substitute: Huang & Grady, "The Compound
Lever", 2024-06-25).
