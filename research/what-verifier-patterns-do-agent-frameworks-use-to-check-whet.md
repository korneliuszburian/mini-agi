## Findings

**Scope note:** claims are sourced to documents I fetched and read in this session (URLs in `## Sources`), plus local files in this repo (`mini-agi`). Claims whose only reachable source is a document citing yet another paper are labelled accordingly.

### A. Deterministic verifier commands — "the run is verified by an executable gate the kernel/benchmark runs itself"

1. **mini-agi (`mini-agi-core`) — declared `verify_command` + `verify_target`.**
   - ADR-0011 (accepted) states: runs may declare two optional fields, `verify_command` (e.g. `make verify`, `npx tsc && node --test`) and `verify_target`; `mini-agi run verify <run.json>` executes the command in the target repo. Source: `docs/adr/ADR-0011-verifiable-reward-layer.md` (lines 18-25).
   - The implementation (`crates/mini-agi-core/src/verifier.rs`) runs `sh -c "<verify_command>"` with `current_dir(target_path)` and maps exit code vs claimed outcome: `verified` (gate passed AND run claims achieved), `verified-failed` (gate failed AND run claims failed), `disagrees` (mismatch), `unverified` (no verifier declared). A 120s timeout kills the process and reports `disagrees`. Source: `crates/mini-agi-core/src/verifier.rs` lines 1-26, 62-74, 117-148.
   - The verifier is itself audited for vacuity: `verify-audit` requires the gate to PASS on known-good work and FAIL on an empty counterfactual target, recording FPR/FNR. Source: `crates/mini-agi-core/src/verifier.rs` lines 163-170; `docs/VERIFIABLE-REWARD-RESEARCH.md` lines 148-157.
   - The repo-wide gate is `scripts/verify.sh`; AGENTS.md binds `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` to it ("A pass you did not observe is a failed gate"). Source: repo `AGENTS.md` ("Verification is deterministic").
   - **fact** (observed in code/docs).

2. **SWE-bench — executable test oracle in Docker.**
   - "SWE-bench evaluates models by applying their generated patches to real-world repositories and running the repository's tests to verify if the issue is resolved" inside "a containerized Docker environment." Source: `docs/guides/evaluation.md` in the SWE-bench repo.
   - Each instance carries `FAIL_TO_PASS` (tests resolved by the PR, which must pass after the patch) and `PASS_TO_PASS` (tests that must pass before and after). Source: SWE-bench dataset card on Hugging Face (dataset fields and data-instance description).
   - "Evaluation is performed by unit test verification using post-PR behavior as the reference solution." Source: SWE-bench dataset card.
   - **fact**.

3. **Terminal-Bench / Harbor — per-task `tests/test.sh` producing a reward file.**
   - Each task = instruction + a test script + an oracle solution; the harness connects a model to a sandboxed terminal. Source: `harbor-framework/terminal-bench` README ("Core Components").
   - The Harbor task format requires `tests/test.sh`, which must write a reward to `/logs/verifier/reward.txt` (e.g. `1`/`0`) or `reward.json`; the doc shows the canonical `pytest`-exit-code → reward mapping. Verifier and agent have separate `timeout_sec`, and the verifier can run in a **separate environment** so proprietary grading code is hidden from the agent. Source: harbor docs "Task Structure".
   - **fact**.

4. **Anthropic Claude Code — "give Claude a check it can run."**
   - "Give Claude a check it can run: tests, a build, a screenshot to compare. It's the difference between a session you watch and one you walk away from." The check is "anything that returns a signal... a test suite, a build exit code, a linter, a script that diffs output against a fixture, or a browser screenshot." Source: "Best practices for Claude Code", section "Give Claude a way to verify its work".
   - Enforcement tiers documented: in-prompt iteration; `/goal` condition re-checked by a separate evaluator every turn; a **Stop hook** that "runs your check as a script and blocks the turn from ending until it passes" (Claude Code overrides the hook and ends the turn after 8 consecutive blocks); and a verification **subagent** where "a fresh model try[ies] to refute the result, so the agent doing the work isn't the one grading it." Source: same page, "Once the check exists, decide how hard it gates the stop."
   - **fact** (this is guidance/recommendation from the vendor — see Verdict).

5. **OpenAI Evals — deterministic string/JSON comparators.**
   - Templates `basic/match.py` (`any([a.startswith(b) for b in B])`), `basic/includes.py` (`b in a`), `basic/fuzzy_match.py` (`a in b or b in a`), `basic/json_match.py` (JSON equality ignoring key order). Source: `docs/eval-templates.md` in the `openai/evals` repo.
   - **fact**.

6. **Karpathy — verifiability as the axis of automation (opinion, primary source).**
   - "Software 1.0 easily automates what you can specify. Software 2.0 easily automates what you can verify"; a verifiable environment must be resettable, efficient, and rewardable. Source: "Verifiability", karpathy.bearblog.dev, 2025-11-17.
   - **opinion** (author's framing; blog post).

### B. Self-report auditing — "the run's own claim, tracked as such, with the audit boundary explicit"

1. **mini-agi — self-reported outcome is a first-class audited field, not trusted.**
   - The `run.json` `outcome.achieved` is "the run's OWN claim until `mini-agi run verify <run.json>` confirms it." Source: repo `AGENTS.md` ("Verification discipline").
   - ADR-0011 context records the prior state as "The eval harness trusts `run.json`'s self-reported `outcome` — nothing runs the work's own gate," and the decision makes verification an additive signal so a run without a verifier stays `unverified` ("the judged composite remains, but the report says so explicitly"). Source: `docs/adr/ADR-0011-verifiable-reward-layer.md` lines 5-7, 28-34, 46-47.
   - Implementation: `verify_run` returns `unverified` with the excerpt "no deterministic verifier declared (outcome is the run's own claim)". Source: `crates/mini-agi-core/src/verifier.rs` lines 62-73.
   - **fact**.

2. **Voyager — "self-verification" as an explicit loop component.**
   - "a new iterative prompting mechanism that incorporates environment feedback, execution errors, and self-verification for program improvement." Source: Voyager paper abstract (arXiv:2305.16291).
   - **fact** (that the paper describes it; the internal mechanism detail is from the paper body I did not fetch in full — see Verdict).

3. **Reflexion — verbal self-reflection on task feedback, where feedback may be self-generated.**
   - "Reflexion agents verbally reflect on task feedback signals... Reflexion is flexible enough to incorporate various types (scalar values or free-form language) and sources (external or internally simulated) of feedback signals." Source: Reflexion paper abstract (arXiv:2303.11366).
   - **fact**.

4. **OpenAI Evals — model-graded evals (LLM grades the completion).**
   - "we have found that using the model to grade itself is a viable strategy for automated evaluation... the evaluation model and the model being evaluated don't have to be the same." Output is parsed from a steerable choice format; non-conforming output parses to `__invalid__`. Source: `docs/eval-templates.md`, "The model-graded eval template".
   - **fact**.

5. **Anthropic research system — LLM-as-judge for free-form outputs, plus human audit.**
   - Research outputs "are difficult to evaluate programmatically... LLMs are a natural fit"; a single judge prompt scoring 0.0-1.0 plus a pass/fail grade was most consistent, and "human evaluation catches what automation misses" (e.g. hallucinated answers on unusual queries). Source: "How we built our multi-agent research system", section "Effective evaluation of agents".
   - **fact**.

6. **LATS — LM-powered value functions as in-loop verifiers.**
   - "we integrate Monte Carlo Tree Search... along with LM-powered value functions and self-reflections," with "an environment for external feedback." Source: LATS paper abstract (arXiv:2310.04406).
   - **fact**.

### C. Disagreement handling — what happens when verifier and self-report clash

1. **mini-agi — `disagrees` is a first-class outcome with hard consequences.**
   - Status logic: `verifier_pass == claims_achieved` → `verified`/`verified-failed`; otherwise `disagrees`. Source: `crates/mini-agi-core/src/verifier.rs` lines 137-148.
   - ADR-0011: `disagrees` is "a judge-calibration signal; exit 1", and "`loop verify` closes a gap only when the composite reaches the target AND the verifier passes... A self-reported outcome is not trusted when a verifier is available and disagrees." Source: `docs/adr/ADR-0011-verifiable-reward-layer.md` lines 22-30.
   - `loop verify` keeps the claim held on disagreement (test: "claim must stay held when the verifier disagrees"). Source: `crates/mini-agi-core/src/loopcmd.rs` around line 706 and the test at 1071.
   - Disagreement rate is monitored as **judge-drift**: `eval judge-drift` computes how often judged outcomes disagree with the deterministic layer against a calibration corpus; below a precision floor it appends a recalibration trigger to the audit log. Source: `crates/mini-agi-core/src/audit.rs` lines 375-391; repo `AGENTS.md` (`eval judge-drift`); ADR-0011 line 44-45 names Self-Taught Evaluators as the underlying idea.
   - The verifier-of-verifier audit records "Recorded FPR/FNR per target in the calibration corpus" — disagreement diagnostics, not just pass/fail. Source: `docs/VERIFIABLE-REWARD-RESEARCH.md` lines 150-157.
   - **fact**.

2. **OpenAI Evals — disagreement is labelled inside the grader.**
   - The `fact.yaml` model-graded eval returns distinct labels including `"D"` for "there is a disagreement between the submitted answer and the expert answer" and `"E"` for "the answers differ, but these differences don't matter." Source: `docs/eval-templates.md`, example model-graded evals.
   - **fact**.

3. **SWE-bench — the oracle can disagree with a "solved-looking" patch; the mismatch literature is only reachable second-hand here.**
   - The harness counts `resolved` only when the post-patch repo passes the required tests (dataset card + eval guide). I could not fetch the primary papers quantifying wrong-but-passing patches (SWE-bench Illusion 2506.12286; suite-weakness preprints 2606.16062, 2503.15223, 2603.00520). The numbers in mini-agi's research doc (e.g. "~28.5% of a SWE-bench sample passes a Docker-verified incorrect patch"; "7.8% of counted-correct patches fail the developer suite") are reproduced there with the caveat "single-paper preprints (replicate before treating as calibration targets)." Source: `docs/VERIFIABLE-REWARD-RESEARCH.md` lines 30-37, 46-49; `docs/RESEARCH-2026-08.md` lines 42-44.
   - **estimate** (numbers unverified by me; flagged preprint) / **fact** (that mini-agi's research doc reports them).

4. **Anthropic — disagreement handled by routing to a different judge tier.**
   - Multi-agent research: single LLM judge scored most consistent "when the eval test cases *did* have a clear answer"; for edge cases, "human evaluation catches what automation misses," and the fix was prompt heuristics (source-quality) rather than a new scorer. Source: "How we built our multi-agent research system".
   - Claude Code best-practices: the Stop-hook deterministic gate has a documented escape (8 consecutive blocks ends the turn) — a bounded disagreement between gate and worker. Source: "Best practices for Claude Code".
   - **fact**.

### D. Comparison

| Framework | Deterministic verifier | Self-report role | Disagreement handling |
|---|---|---|---|
| mini-agi | `verify_command` in `verify_target`, exit-code gate, 120s timeout, verify-audit | `outcome.achieved` = run's own claim; `unverified` if no gate | `disagrees` → exit 1, gap stays open, claim held; judge-drift + calibration corpus |
| SWE-bench | `FAIL_TO_PASS`/`PASS_TO_PASS` tests in Docker | model patch is the artifact, no self-report | only the oracle decides `resolved`; wrong-but-green patches are the open failure mode |
| Terminal-Bench/Harbor | `tests/test.sh` → reward file; separate verifier env | agent run vs verifier are separate phases | verifier timeouts; tamper-sensitive evidence via sidecars, not agent artifacts |
| Claude Code | user-supplied check (tests/build/screenshot), Stop hook, `/goal` | agent "stops when the work looks done" — that is the failure mode being gated | hook blocks turn; 8-consecutive-blocks override; fresh subagent refutes |
| OpenAI Evals | `match/includes/fuzzy/json_match` | LLM grades its own output (modelgraded) | disagreement is a label (`D`) in fact.yaml; `__invalid__` parse |
| Voyager / Reflexion / LATS | LATS: environment feedback + value functions | Voyager self-verification; Reflexion internally simulated feedback | self-critique is the loop's own feedback signal (no separate judge) |

## Sources

Fetched this session (primary):
- mini-agi repo (local): `AGENTS.md`; `docs/adr/ADR-0011-verifiable-reward-layer.md`; `docs/PLAN.md`; `docs/VERIFIABLE-REWARD-RESEARCH.md`; `docs/RESEARCH-2026-08.md`; `docs/harness/HARNESS-2026-08-04-cb09335.md`; `crates/mini-agi-core/src/verifier.rs`; `crates/mini-agi-core/src/audit.rs`; `crates/mini-agi-core/src/loopcmd.rs`.
- SWE-bench: https://raw.githubusercontent.com/SWE-bench/SWE-bench/main/README.md ; https://raw.githubusercontent.com/SWE-bench/SWE-bench/main/docs/guides/evaluation.md ; https://arxiv.org/abs/2310.06770 ; https://huggingface.co/datasets/SWE-bench/SWE-bench
- Terminal-Bench / Harbor: https://raw.githubusercontent.com/harbor-framework/terminal-bench/main/README.md ; https://harborframework.com/docs/task-format
- Anthropic: https://www.anthropic.com/engineering/claude-code-best-practices ; https://www.anthropic.com/engineering/built-multi-agent-research-system
- OpenAI Evals: https://raw.githubusercontent.com/openai/evals/main/README.md ; https://raw.githubusercontent.com/openai/evals/main/docs/eval-templates.md
- arXiv abstracts: https://arxiv.org/abs/2305.16291 (Voyager) ; https://arxiv.org/abs/2303.11366 (Reflexion) ; https://arxiv.org/abs/2310.04406 (LATS) ; https://arxiv.org/abs/2305.20050 (Let's Verify Step by Step) ; https://arxiv.org/abs/2408.02666 (Self-Taught Evaluators)
- https://karpathy.bearblog.dev/verifiability/

## Verdict

**Established:** Three verifier patterns are real and directly observable in primary sources. (1) Deterministic verifier commands — mini-agi's declared `verify_command`/`verify_target` with a 120s timeout and exit-code semantics (verifier.rs), SWE-bench's `FAIL_TO_PASS`/`PASS_TO_PASS` Docker test oracle, Terminal-Bench/Harbor's `test.sh`→reward-file contract with separate verifier environments, and OpenAI Evals' deterministic comparators. (2) Self-report auditing — mini-agi treats `outcome.achieved` as the run's own claim and downgrades to `unverified` when no gate exists; Voyager's and Reflexion's self-verification/self-reflection; OpenAI's and Anthropic's LLM-as-judge grading of the model's own output. (3) Disagreement handling — mini-agi's `disagrees` state (gate/claim mismatch blocks gap-close, exit 1, claim held) plus judge-drift/calibration-corpus monitoring; OpenAI's `fact.yaml` explicit `D`/`E` disagreement labels; Anthropic's Stop-hook with a bounded override and routing to fresh-subagent or human judges.

**Uncertain / not verifiable from sources I reached:**
- The quantitative "verifier-weakness" numbers (~28.5% SWE-bench false-pass, 7.8% dev-suite failures, one-in-five "solved" patches semantically wrong) come only from mini-agi's research doc citing preprints I did not fetch (2606.16062, 2503.15223, 2603.00520). Treat as unconfirmed; the source doc itself flags them as preprints.
- Voyager's internal self-verification prompt mechanics were not read in full (abstract only); the fact that self-verification is a designed component is established.
- Whether "verifier of the verifier" audits (mini-agi `verify-audit`) are common outside this repo — I found no primary source from another framework doing the same; unverified, likely a differentiator.
- "Let's Verify Step by Step" (2305.20050) establishes that step-level supervision beats outcome-only supervision for math reward models, but I did not find a production agent framework that runs step-level process supervision as its run verifier; the mini-agi plan lists it as roadmap, not shipped.

**Evidence that would settle the open items:** (a) fetching the four preprints to confirm the false-pass/dev-suite/`SWE-bench Illusion` statistics; (b) reading Voyager's `minecraft` prompts in the MineDojo/Voyager repo for the exact self-verification wording; (c) a survey of Claude-Code Stop-hook usage telemetry or Anthropic's eval write-ups for how often the 8-block override fires — no primary source I reached reports that rate.
