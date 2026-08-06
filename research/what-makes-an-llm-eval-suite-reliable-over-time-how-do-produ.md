## Findings

Scope note: for the research papers cited below I read the arXiv abstract pages (HTML) — full PDFs were not read. Claims from papers are limited to what their abstracts state.

### A. What makes an eval suite reliable over time

**A1. Evaluations must start from measurable, well-specified success criteria.**
- **fact** — Anthropic's eval guidance defines good success criteria as *Specific, Measurable, Achievable, Relevant*, and recommends measuring on a "held-out test set" (e.g., "F1 ≥ 0.85 on a held-out test set of 10,000 diverse Twitter posts"). Source: Anthropic developer docs, "Define success criteria and build evaluations" — https://platform.claude.com/docs/en/test-and-evaluate/develop-tests.md
- **fact** — The same doc's eval design principles: be task-specific, "Automate when possible" (multiple-choice, string match, code-graded, LLM-graded), and "Prioritize volume over quality" (many automated-graded questions over fewer human-graded ones). Source: same URL.

**A2. Reliability is a function of task quality, not just grader quality.**
- **fact** — Anthropic's agent-eval practice: a good task is one "where two domain experts would independently reach the same pass/fail verdict," each task should be passable given a reference solution, and "a 0% pass rate across many trials (0% pass@100) is most often a signal of a broken task, not an incapable agent." Source: Anthropic, "Demystifying evals for AI agents" (2026-01-09) — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
- **fact** — SWE-bench Verified, a human-validated subset of SWE-bench built by OpenAI with 93 professional annotators (each sample labeled 3×, ensembled by max severity), filtered out 68.3% of the original SWE-bench test set for underspecified problem statements (38.3%), unit tests that unfairly reject valid solutions (61.1%), or other issues. Source: OpenAI, "Introducing SWE-bench Verified" (2024-08-13) — https://openai.com/index/introducing-swe-bench-verified/
- **fact** — Anthropic reports that Opus 4.5 initially scored 42% on CORE-Bench and jumped to 95% after fixing rigid grading (e.g., penalizing "96.12" for expected "96.124991…"), ambiguous specs, and unreproducible stochastic tasks; METR likewise found misconfigured time-horizon tasks whose graders required exceeding the score threshold stated in the task. **fact-as-reported** (Anthropic citing external sources). Source: "Demystifying evals for AI agents" — same URL.

**A3. Grader choice and structure determine reproducibility.**
- **fact** — Anthropic classifies agent graders into code-based (fast, objective, reproducible, but "brittle to valid variations"), model-based (flexible but "non-deterministic" and "requires calibration with human graders"), and human (gold standard, calibrates LLM graders); they recommend deterministic graders where possible, and grading outcomes rather than specific tool-call sequences ("grade what the agent produced, not the path it took"). Source: "Demystifying evals for AI agents" — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
- **fact** — LiveBench was designed to avoid both test-set contamination and the biases of LLM judges/crowdsourcing by scoring "answers automatically according to objective ground-truth values." Source: White et al., "LiveBench: A Challenging, Contamination-Limited LLM Benchmark," arXiv:2406.19314 — https://arxiv.org/abs/2406.19314

**A4. Reproducibility requires versioning, integrity checks, and pinned environments.**
- **fact** — EleutherAI's lm-evaluation-harness gives every task a `VERSION` field and enforces stability with unit tests, so "if the task definition changes... we can know exactly which metrics were computed using the old buggy implementation"; it also offers `--check_integrity` for data verification. Source: EleutherAI lm-evaluation-harness README — https://github.com/EleutherAI/lm-evaluation-harness (README.md, "Task Versioning" section)
- **fact** — OpenAI collaborated with the SWE-bench authors to build a Docker-containerized harness because SWE-bench environments were "difficult to reliably set up... inadvertently causing unit tests to fail regardless of the solution." Source: OpenAI, "Introducing SWE-bench Verified" — https://openai.com/index/introducing-swe-bench-verified/
- **fact** — Anthropic's harness guidance: each trial must start from a clean, isolated environment, because "unnecessary shared state between runs... can cause correlated failures due to infrastructure flakiness rather than agent performance" (they observed Claude gaining an unfair advantage by reading git history from prior trials). Source: "Demystifying evals for AI agents" — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents

**A5. Reliability over time is maintenance work, not a one-time build.**
- **fact** — Anthropic: "An eval suite is a living artifact that needs ongoing attention and clear ownership"; effective at Anthropic was dedicated evals teams owning infrastructure while domain/product teams contribute tasks; automated evals "require ongoing maintenance as product and model evolves to avoid drift." Source: "Demystifying evals for AI agents" — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
- **fact** — Anthropic separates **capability evals** (low initial pass rate, a "hill to climb") from **regression evals** (≈100% pass rate, protect against backsliding); once capabilities plateau, high-pass-rate capability evals "graduate" into a continuously run regression suite. Source: same URL.

### B. Benchmark leakage / contamination

**B1. Contamination is a documented, first-class failure mode that can invalidate benchmarks.**
- **fact** — "Test set contamination, wherein test data from a benchmark ends up in a newer model's training set, is a well-documented obstacle for fair LLM evaluation and can quickly render benchmarks obsolete." Source: LiveBench abstract, arXiv:2406.19314 — https://arxiv.org/abs/2406.19314
- **fact** — Pretrained models are trained on web corpora "often 'contaminated' with downstream test sets"; controlled experiments (BERT pretrained on Wikipedia + labeled downstream data) show exploitation "exists in some cases, but in others the models memorize the contaminated data, but do not exploit it," and that memorization vs. exploitation is affected by duplication count and model size. Source: Magar & Schwartz, "Data Contamination: From Memorization to Exploitation," ACL 2022, arXiv:2203.08242 — https://arxiv.org/abs/2203.08242

**B2. Leakage handling in practice: private/held-out data, freshness, and decontamination filters.**
- **fact** — LiveBench mitigates contamination by (1) frequently updated questions drawn from recent math competitions, arXiv papers, news, and datasets, and (2) harder "contamination-limited" versions of prior benchmarks; "Questions are added and updated on a monthly basis." Source: arXiv:2406.19314 — https://arxiv.org/abs/2406.19314
- **fact** — The EleutherAI harness ships a "Test Set Decontamination" utility that scores only "data points not found in the model training set," via precomputed 13-gram exact-match indices against The Pile. Source: lm-evaluation-harness README — https://github.com/EleutherAI/lm-evaluation-harness
- **fact** — MLE-bench (agent benchmark built from 75 Kaggle competitions) explicitly "investigate[s]... the impact of contamination from pre-training" alongside its main results. Source: Chan et al., "MLE-bench," arXiv:2410.07095 — https://arxiv.org/abs/2410.07095
- **fact** — OpenAI states that SWE-bench, being scraped public GitHub repos, is "likely to be contaminated" for models pre-trained on internet text, and that static datasets are "inherently limited" — so it must be supplemented by other evals. Source: "Introducing SWE-bench Verified" — https://openai.com/index/introducing-swe-bench-verified/
- **fact** — OpenAI Evals offers "private evals" on a company's own data so teams "represent the common LLM patterns in your workflow without exposing any of that data publicly." Source: OpenAI Evals GitHub README — https://github.com/openai/evals

### C. Flakiness

**C1. Non-determinism is expected in agent evals and is measured, not assumed away.**
- **fact** — Anthropic: "Because model outputs vary between runs, we run multiple trials to produce more consistent results," and a task "that passed on one eval run might fail on the next." Two complementary metrics: **pass@k** (at least one of k trials succeeds) and **pass^k** (all k trials succeed); for a 75% per-trial success rate, pass^3 ≈ 42%. Source: "Demystifying evals for AI agents" — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
- **fact** — OpenAI ran SWE-bench Verified scaffolds with "a single seed," explicitly noting "results may differ from what is reported in the official leaderboards" — an acknowledgment that single-seed agent runs are noisy. Source: footnote 4, "Introducing SWE-bench Verified" — https://openai.com/index/introducing-swe-bench-verified/

**C2. Flakiness sources are identified and eliminated: infrastructure, spec ambiguity, and grader errors.**
- **fact** — SWE-bench Verified identified three reliability problems: overly-specific/unrelated unit tests (false rejections), underspecified issue descriptions, and unreliable dev-environment setup — and addressed them via human annotation plus a containerized (Docker) harness. Source: https://openai.com/index/introducing-swe-bench-verified/
- **fact** — Anthropic's guidance on failure triage: read transcripts to distinguish "a genuine mistake" from "graders rejected a valid solution"; design graders "resistant to bypasses or hacks"; build partial credit for multi-component tasks. Source: "Demystifying evals for AI agents" — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
- **fact** — METR (2026) measured the noise floor of human grading itself: repo maintainers merged only ~68% of original human-written ("golden") patches, so METR normalizes all agent scores against this golden baseline rather than treating any single grader as exact. Source: METR research note, "Many SWE-bench-Passing PRs Would Not Be Merged into Main" (2026-03-10) — https://metr.org/notes/2026-03-10-many-swe-bench-passing-prs-would-not-be-merged-into-main/

### D. Regression drift

**D1. Model behavior drifts even without your code changing — evals are the detection mechanism.**
- **fact** — GPT-4 (March 2023) identified prime vs. composite numbers with 84% accuracy; GPT-4 (June 2023) scored 51% on the same questions; instruction-following ability decreased over that window; the authors conclude behavior of the "same" LLM service can change substantially in a short time, "highlighting the need for continuous monitoring of LLMs." Source: Chen, Zaharia, Zou, "How is ChatGPT's behavior changing over time?" arXiv:2307.09009 — https://arxiv.org/abs/2307.09009

**D2. Production teams build regression detection into the eval workflow.**
- **fact** — OpenAI's regression-eval recipe: define one eval (data source + grader), run a **baseline run** against the current prompt, then run a candidate ("regression-run") after a prompt change; the lower score on the same dataset flags the regression before shipping. Source: OpenAI Cookbook, "Evaluations Example: Push Notifications Summarizer Prompt Regression" — https://cookbook.openai.com/examples/evaluation/use-cases/regression
- **fact** — OpenAI's eval run API returns per-criteria pass/fail counts and supports webhook events (`eval.run.succeeded`/`failed`/`canceled`), enabling continuous/automated regression runs. Source: OpenAI, "Working with evals" — https://platform.openai.com/docs/guides/evals
- **fact** — Anthropic: regression evals "should have a nearly 100% pass rate... a decline in score signals that something is broken," and capability evals that saturate "graduate" into continuously-run regression suites. Source: "Demystifying evals for AI agents" — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents

**D3. Eval-suite drift: saturation and ecosystem progress make static suites deceptive.**
- **fact** — Anthropic: "Eval saturation occurs when an agent passes all of the solvable tasks, leaving no room for improvement"; SWE-bench Verified went from ~30% to >80% for frontier models in about a year, and "large capability improvements [then] appear as small increases in scores"; Anthropic's rule is to not trust scores until transcripts are read. Source: "Demystifying evals for AI agents" — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
- **fact** — METR notes that as its time-horizon task suite saturates, "the results are becoming more sensitive to analysis choices." Source: METR Notes index, "Impact of modelling assumptions on time horizon results" (2026-03-20) — https://metr.org/notes/2026-03-20-impact-of-modelling-assumptions-on-time-horizon-results/
- **fact** — Progress isn't only the model: OpenAI reports GPT-4 on SWE-bench Lite scoring between 2.7% (early RAG-based scaffold) and 28.3% (CodeR) depending on scaffolding, and therefore runs evaluations "continually and as often as needed... before, during, and even after training." Source: "Introducing SWE-bench Verified" — https://openai.com/index/introducing-swe-bench-verified/
- **fact** — LiveBench's monthly refresh of questions is explicitly a response to benchmarks becoming obsolete via contamination. Source: arXiv:2406.19314 — https://arxiv.org/abs/2406.19314

**D4. Calibration and grounding keep graders honest over time.**
- **fact** — Anthropic: LLM-as-judge graders "should be closely calibrated with human experts," graded per-dimension with isolated judges, given an "Unknown" escape to reduce hallucinated verdicts; "once the system is robust, it's sufficient to use human review only occasionally." Source: "Demystifying evals for AI agents" — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
- **fact** — For subjective output, Anthropic recommends constraining output format and using structured outputs for guaranteed schema compliance (mechanisms that reduce grader-side parsing flakiness). Source: Anthropic, "Increase output consistency" — https://platform.claude.com/docs/en/test-and-evaluate/strengthen-guardrails/increase-consistency.md

## Sources

1. Anthropic, "Demystifying evals for AI agents" (2026-01-09) — https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
2. Anthropic developer docs, "Define success criteria and build evaluations" — https://platform.claude.com/docs/en/test-and-evaluate/develop-tests.md
3. Anthropic developer docs, "Increase output consistency" — https://platform.claude.com/docs/en/test-and-evaluate/strengthen-guardrails/increase-consistency.md
4. OpenAI, "Working with evals" — https://platform.openai.com/docs/guides/evals
5. OpenAI Cookbook, "Push Notifications Summarizer Prompt Regression" — https://cookbook.openai.com/examples/evaluation/use-cases/regression
6. OpenAI Evals repository README — https://github.com/openai/evals
7. OpenAI, "Introducing SWE-bench Verified" (2024-08-13) — https://openai.com/index/introducing-swe-bench-verified/
8. Jimenez et al., "SWE-bench: Can Language Models Resolve Real-World GitHub Issues?" arXiv:2310.06770 — https://arxiv.org/abs/2310.06770
9. Chen, Zaharia, Zou, "How is ChatGPT's behavior changing over time?" arXiv:2307.09009 — https://arxiv.org/abs/2307.09009
10. Magar & Schwartz, "Data Contamination: From Memorization to Exploitation" (ACL 2022), arXiv:2203.08242 — https://arxiv.org/abs/2203.08242
11. Chan et al., "MLE-bench: Evaluating Machine Learning Agents on Machine Learning Engineering," arXiv:2410.07095 — https://arxiv.org/abs/2410.07095
12. White et al., "LiveBench: A Challenging, Contamination-Limited LLM Benchmark" (ICLR 2025), arXiv:2406.19314 — https://arxiv.org/abs/2406.19314
13. EleutherAI lm-evaluation-harness README — https://github.com/EleutherAI/lm-evaluation-harness
14. METR, "Many SWE-bench-Passing PRs Would Not Be Merged into Main" (2026-03-10) — https://metr.org/notes/2026-03-10-many-swe-bench-passing-prs-would-not-be-merged-into-main/
15. METR Notes index, "Impact of modelling assumptions on time horizon results" (2026-03-20) — https://metr.org/notes/2026-03-20-impact-of-modelling-assumptions-on-time-horizon-results/

## Verdict

**Established (multiple independent primary sources):**
- Reliability comes from design and maintenance jointly: specific success criteria (Anthropic), task quality with reference solutions and transcript review (Anthropic), human-validated task filtering (OpenAI/SWE-bench Verified: 68.3% of samples rejected), grader choice favoring deterministic/objective scoring (Anthropic, LiveBench), reproducibility tooling — task versioning, integrity checks, containerized environments (EleutherAI, OpenAI, Anthropic) — and explicit suite ownership (Anthropic).
- Leakage is a real, documented failure mode with named mitigations: continuously refreshed questions (LiveBench, monthly), pre-training contamination analysis and filtering (MLE-bench, EleutherAI decontamination utility), acknowledging that public-repo-derived benchmarks are likely contaminated (OpenAI on SWE-bench), and private held-out data (OpenAI).
- Flakiness is treated as a measurable, structural property: multiple trials with pass@k/pass^k metrics, per-trial environment isolation, false-negative auditing, and normalization against human-grading noise (Anthropic, METR, OpenAI).
- Regression drift is addressed by baseline-vs-regression run workflows, continuous/repeated evaluation, capability→regression suite graduation, and saturation awareness (OpenAI cookbook/API, Anthropic, METR, Chen et al. 2023).

**Uncertain / not fully verified:**
- I could not locate, from the sources I reached, a dedicated METR analysis of the sensitivity of agent-eval *scoring functions* (a frequently-cited METR finding); do not treat its specific numbers as verified here. The OpenAI-reported CORE-Bench 42%→95% and METR misconfigured-threshold anecdotes are secondary reports inside a primary source, not independently verified.
- For the research papers (Chen et al., Magar & Schwartz, SWE-bench, MLE-bench, LiveBench) I verified claims from their arXiv abstracts only, not full PDFs. Specific internal methodology (exact contamination-measurement procedures, n-gram thresholds, annotation rubrics) would need the PDFs or repo code.

**What would settle it:** reading the full PDFs/repos of the five papers; and for the missing METR scoring-functions work, locating the original post or its repository and verifying its exact conclusions before citing its numbers.

One notable caveat about primary-source integrity surfaced during research: a same-titled arXiv entry (2305.10348) returned by search was an unrelated laser-physics paper, and the correctly identified contamination paper (Magar & Schwartz, arXiv:2203.08242) was found via its exact title through the arXiv API — evidence that citation-by-recollection alone is unreliable, which is itself an argument for versioned, URL-anchored eval evidence.
