## Findings

Scope: "empirically derived" is taken as — the taxonomy's categories were induced from a body of observed failures (annotated traces, incident logs, benchmark rollouts), not merely asserted a priori. Baseline context (excluded by the question): **MAST** — "Why Do Multi-Agent LLM Systems Fail?" (Cemri et al., UC Berkeley; arXiv 2503.13657). Fact (from paper full text, arXiv HTML): MAST has 14 failure modes in 3 categories — FC1 System Design Issues (disobey task spec 11.8%, disobey role spec 1.5%, step repetition 15.7%, loss of conversation history 2.8%, unaware of stopping conditions 12.4%), FC2 Inter-Agent Misalignment (conversation reset 2.2%, fail to ask for clarification 6.8%, task derailment 7.4%, information withholding 0.85%, ignored other agent's input 1.9%, reasoning–action mismatch 13.2%), FC3 Task Verification (premature termination 6.2%, no/incomplete verification 8.2%, incorrect verification 9.1%). Measured: Grounded Theory analysis of 150 traces across 5 frameworks, refined via 3 inter-annotator rounds (κ=0.88), then LLM-as-judge annotation of 1,642 traces (κ=0.77 vs. humans).

### Cross-domain, architecture-general taxonomies

**1. Interaction-Centric Taxonomy ("Model or Harness?")** — Raj, Gupta, Mahmoud et al. (Scale AI; arXiv 2607.28802, July 2026).
Fact (paper full text): **41 failure modes**, each assigned to an interaction *edge* between two components and a *fault side* (model vs. other). Components: Model, Owner, Grader, Third party, Context, Memory, Tool, Local env., External env., grouped into three families (User, Harness, Environment); model–model edges carry peer/subagent roles. Of 41 modes, 36 are model-side, 5 are component-side. Named modes include: Over-initiative, Under-initiative, Satisficing, Instruction-Following Failure, Reasoning Failure, Unauthorized Irreversible Action, Sycophancy, Domain Knowledge Deficit, Value Misalignment, Instruction–Grader Mismatch (owner edge); Specification Gaming, Evaluation Awareness (grader edge); Indirect Prompt Injection, Contextual Sycophancy (third-party); State Tracking Failure, Goal Drift, Context Rationale Erosion (context); Missed Write, State Staleness, Overgeneralization, Memory Rationale Erosion, Pollution, Redundancy, Missed Read, Memory Following Failure (memory); Incorrect Tool Selection, Tool Hallucination, Tool Feedback Neglect, Tool Recovery Failure, Malformed Arguments, Suboptimal Arguments, Mistranslation (tool); Delegation Failure, Communication Failure (model–model); Recovery Failure, Service Failure, Stale State Delivery (external env.); Observation Failure, Recovery Failure (local env.).
Measured (fact, paper full text): taxonomy grounded in 40 worked examples (E1–E40) drawn from public benchmarks, model system cards, published reports, and logged agent trajectories; labels assigned by tracing the earliest unrecoverable failure ("root-cause principle"); reproducibility validated by using independent reasoning agents as judges — strongest judge reaches Cohen's κ=0.76 vs. human category labels, highest pairwise judge agreement κ=0.84.

**2. AgentRx failure taxonomy ("AgentRx: Diagnosing AI Agent Failures from Execution Trajectories")** — Barke et al. (Microsoft; arXiv 2602.02475, Feb 2026).
Fact (abstract; full text not reachable — arXiv HTML returned 404): a "grounded-theory derived, cross-domain failure taxonomy" applied to **115 manually annotated failed trajectories** spanning structured API workflows, incident management, and open-ended web/file tasks; each trajectory is labeled with a critical failure step and a taxonomy category. Category names are not enumerated in the abstract — **unknown, not verifiable from the sources I reached** (PDF not readable).

**3. Longitudinal silent-failure taxonomy ("When Errors Become Narratives")** — Wu (arXiv 2606.14589, June 2026).
Fact (abstract): **five mechanism-oriented classes** — (A) environment and platform quirks, (B) design-assumption mismatches, (C) error swallowing and dilution, (D) chained hallucination and fabrication, (E) operational omission and forensic blind spots — with class D described as the unique-to-LLM "fail-plausible" pattern.
Measured (fact, abstract): 8-week longitudinal study of one production personal-assistant runtime (~40 scheduled jobs, 8 LLM providers), 22 documented root-caused incidents; reported ~70% of silent failures caught by human user-view observation, 0% ex-ante prevention vs. 87% regression-blocking in retrospective audit, incident latency 13h–60 days.

### Agent–environment interaction scoped

**4. Aegis taxonomy ("Aegis: Taxonomy and Optimizations for Overcoming Agent-Environment Failures in LLM Agents")** — Song et al. (U. Toronto/Vector; arXiv 2508.19504, Aug 2025).
Fact (paper full text): **6 failure modes in 3 categories** — Exploration Failures: State-space Navigation Failure, State Awareness Failure; Exploitation Failures: Tool Output Processing Failure (further split into comparison/calculation/retrieval/sorting), Domain Rule Violation (invalid action vs. lack of correct action), User Instruction Following Failure; plus Resource Exhaustion (turn/token limit).
Measured (fact, paper full text): analyzed **142 failed traces / 3,656 turns** across 5 benchmarks (TauBench airline & retail, BFCL file system, CRM-Arena, MedAgentBench) and 3 models (GPT-4.1, GPT-4.1 mini, o3); failures localized to the first unsuccessful subtask using an HTN-inspired subtask abstraction; per-workload and per-model failure distributions reported (e.g., resource exhaustion = 45%/83% of retail/CRM failures under GPT-4.1).

### Domain-scoped (deep research, tool-use, autonomous task agents)

**5. DEFT — Deep rEsearch Failure Taxonomy ("How Far Are We from Genuinely Useful Deep Research Agents?")** — Zhang et al. (OPPO AI Agent Team; arXiv 2512.01948, Dec 2025).
Fact (paper full text): **14 failure modes under 3 core categories** — Reasoning: Failure to Understand Requirements (10.55%), Lack of Analytical Depth (11.09%), Limited Analytical Scope (0.90%), Rigid Planning Strategy (5.60%); Retrieval: Insufficient External Information Acquisition (16.30%), Information Handling Deficiency (2.26%), Information Integration Failure (2.91%), Information Representation Misalignment (2.91%), Verification Mechanism Failure (8.72%); Generation: Redundant Content Piling (2.51%), Structural Organization Dysfunction (2.26%), Content Specification Deviation (10.73%), Deficient Analytical Rigor (4.31%), Strategic Content Fabrication (18.95%).
Measured (fact, paper full text): grounded theory on ~1,000 reports from 10+ DRAs; open coding by 5 LLM coders (Claude Opus 4.1, Gemini 2.5 Pro, Grok 4, DeepSeek-V3.1, Qwen3-Max-Preview) → 51 concepts; axial coding in 3 ICR rounds with 3 domain experts sampling 24–54 records, Krippendorff's α; selective coding → 3 core categories; theoretical-saturation check on unseen agents (WebThinker, OpenManus); human–LLM inter-coder reliability α≈0.80–0.90.

**6. DeepVerifier DRA Failure Taxonomy ("Inference-Time Scaling of Verification…")** — Wan et al. (CUHK/Tencent; arXiv 2601.15808, ACL 2026 Findings).
Fact (paper full text): **5 major classes and 13 sub-classes**, built to drive rubric-based verification. Analysis text names the major classes: Problem Understanding, Finding Sources (dominant — e.g., consulting wrong evidence, generic searches), Reasoning (premature conclusions, misinterpretation, hallucinated/overconfident claims), Action Errors, Max Step Reached; 13 sub-classes shown in Fig. 3 (not all names reproduced in fetched text).
Measured (fact, paper full text): 2,997 agent actions across 90 tasks on WebAggregatorQA (CK-Pro agent, Claude-3.7-Sonnet backbone); **555 error points** annotated by two staff annotators against human reference solutions (63% cross-annotator overlap); taxonomy built by iterative clustering/labeling of error points.

**7. Three-tier task-phase taxonomy ("Exploring Autonomous Agents: A Closer Look at Why They Fail…")** — Lu, Li, Huo (arXiv 2508.13143, ASE 2025 NIER).
Fact (abstract): **three-tier taxonomy aligned to task phases — planning errors, task execution issues, incorrect response generation**.
Measured (fact, abstract): benchmark of 34 programmable tasks, 3 open-source agent frameworks × 2 LLM backbones, ~50% observed task completion; failure causes induced from in-depth failure analysis of runs.

**8. Tool-agent parameter-filling taxonomy ("Butterfly Effects in Toolchains…")** — Xiong et al. (arXiv 2507.15296, July 2025).
Fact (abstract): a parameter-failure taxonomy with **five failure categories derived from the invocation chain of a mainstream tool agent**. Individual category names not enumerated in the abstract — **unknown, not verifiable from sources I reached** (arXiv HTML 404, PDF not readable). Reported result (fact, abstract): parameter-name-hallucination failures stem chiefly from LLM limits; other categories trace to input sources. Measurement: 15 input perturbation methods correlated against failure categories.

**9. AgentEval failure taxonomy** — Guo, Wu, Yiu (arXiv 2604.23581, ACL 2026 Industry).
Fact (abstract): a **hierarchical failure taxonomy (3 levels, 21 subcategories)** used for DAG-structured step-level evaluation with root-cause attribution. Category names not enumerated in the abstract.
Measured (fact, abstract): 450 test cases across three production workflows and two agent model families; Cohen's κ=0.84 vs. human experts; 72% root-cause accuracy; failure-detection recall 0.89 vs. 0.41 for end-to-end; transfer shown on tau-bench and SWE-bench traces.

**10. Tool-invocation reliability taxonomy ("When Agents Fail to Act…")** — Huang, Malwe, Wang (arXiv 2601.16280, ICAIBD 2026).
Fact (abstract): **12-category error taxonomy** spanning tool initialization, parameter handling, execution, and result interpretation. Category names not enumerated in the abstract.
Measured (fact, abstract): 1,980 deterministic test instances across open-weight (Qwen2.5 series, Functionary) and proprietary models (GPT-4, Claude 3.5/3.7); tool-initialization failures identified as primary bottleneck for smaller models.

### Additional narrower, empirically grounded taxonomies (fact from abstracts unless noted)
- XAI for Coding Agent Failures (Joshi, arXiv 2603.05941): domain-specific failure taxonomy "derived from analyzing real agent failures"; user study n=20. Fact (abstract).
- SpreadsheetBench 2 (Zhu et al., arXiv 2606.29955): trajectory-analysis failure taxonomy; dominant bottlenecks: insufficient spreadsheet inspection, incorrect target-cell selection. Fact (abstract).
- NOMAD UML error taxonomy (Giannouris & Ananiadou, arXiv 2511.22409): errors in LLM-generated UML diagrams — structural, relationship, semantic/logical. Fact (abstract).
- Harness-sensitivity six-label taxonomy (Cho et al., arXiv 2605.26731): 432-run controlled experiment; format_violation dominates capable models, wrong_file dominates low-capability models. Fact (abstract).
- Benchmark-scoped error characterizations exist (e.g., SWE-bench diff/test failures; ToolScan malformed-call/recovery errors; MCP-atlas) — claim sourced to the related-work section of the Interaction-Centric Taxonomy paper (arXiv 2607.28802), not independently verified here.

## Sources
Primary sources fetched during this research:
1. MAST — arXiv abs + full HTML, https://arxiv.org/abs/2503.13657 and https://arxiv.org/html/2503.13657v3
2. Interaction-Centric Taxonomy — arXiv abs + full HTML, https://arxiv.org/abs/2607.28802 and https://arxiv.org/html/2607.28802v1
3. AgentRx — arXiv abs, https://arxiv.org/abs/2602.02475
4. Silent failures — arXiv abs, https://arxiv.org/abs/2606.14589
5. Aegis — arXiv abs + full HTML, https://arxiv.org/abs/2508.19504 and https://arxiv.org/html/2508.19504v1
6. DEFT/FINDER — arXiv abs + full HTML, https://arxiv.org/abs/2512.01948 and https://arxiv.org/html/2512.01948v2
7. DeepVerifier — arXiv abs + full HTML, https://arxiv.org/abs/2601.15808 and https://arxiv.org/html/2601.15808v2
8. Lu et al. — arXiv abs, https://arxiv.org/abs/2508.13143
9. Butterfly/toolchain — arXiv abs, https://arxiv.org/abs/2507.15296
10. AgentEval — arXiv abs, https://arxiv.org/abs/2604.23581
11. When Agents Fail to Act — arXiv abs, https://arxiv.org/abs/2601.16280
12. Additional abstracts: https://arxiv.org/abs/2603.05941, https://arxiv.org/abs/2606.29955, https://arxiv.org/abs/2511.22409, https://arxiv.org/abs/2605.26731
13. arXiv search API queries (discovery): http://export.arxiv.org/api/query (multiple queries)

## Verdict
**Established (fact):** Besides MAST there is a substantial, actively growing body of empirically derived LLM-agent failure taxonomies. The strongest-documented are: the Interaction-Centric Taxonomy (41 modes; induced from 40 grounded worked examples; judge reproducibility κ=0.76), DEFT (14 modes; grounded theory over ~1,000 DRA reports with Krippendorff's-α validation), the Aegis agent–environment taxonomy (6 modes; 142 annotated failed traces), the DeepVerifier DRA taxonomy (5 classes/13 sub-classes; 555 annotated error points), the longitudinal silent-failure taxonomy (5 classes; 22 production incidents), and smaller three-tier/12-category/5-category/21-subcategory taxonomies (Lu et al.; Huang et al.; Xiong et al.; Guo et al.). All report explicit measurement corpora (trace counts, annotator counts, agreement statistics, task/turn counts), which is what makes them "empirically derived."
**Uncertain:** (i) There is no single canonical taxonomy — the space is fragmented by scope (multi-agent, agent–environment, deep research, tool-use, production runtime) and by whether categories name the responsible component or the observable symptom; no survey synthesizing them was located in this search. (ii) For AgentRx (2602.02475) and the toolchain parameter taxonomy (2507.15296) the category names could not be verified — their arXiv HTML pages returned 404 and the PDFs were not readable via my tools; only the abstracts were cited.
**What would settle it:** (a) Reading the full PDFs of AgentRx and the Butterfly toolchain paper to enumerate their category names; (b) a comprehensive literature survey of agent-failure taxonomies (none was found in my arXiv queries); (c) direct comparison of category overlap across MAST, Interaction-Centric, and Aegis on a shared annotated corpus.
