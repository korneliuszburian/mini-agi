The 2408.04718 ID resolved to a different paper. Let me locate the correct Hybrid LLM paper and get full texts.
## Findings

All claims below are sourced to the primary documents I read (full text where noted). **Fact** = stated/measured in the cited source; **estimate** = author-computed from assumptions in the source; **opinion** = the source's interpretation, flagged as such.

### 1. Strategy taxonomy

- **Fact.** FrugalGPT defines three cost-reduction strategies: prompt adaptation, LLM approximation, and **LLM cascade** (query a list of LLMs sequentially; a scoring function `g(q, answer) ∈ [0,1]` and a learned per-model threshold decide when to stop and return). The cascade is a length-3 list, scoring via a DistilBERT regressor. Source: FrugalGPT, arXiv 2305.05176, §3 "Strategy 3: LLM cascade" and §4 "Setups".
- **Fact.** Hybrid LLM routes each query to exactly one of two models (large vs small), never calling multiple LLMs per query. The router is a BERT-style encoder (DeBERTa-v3-large) trained to predict the *quality gap* `H(x) = q(S(x)) − q(L(x))` where q is BART score; a test-time threshold on the router score trades quality for cost. Source: Hybrid LLM, arXiv 2404.14618v1, §2.2, §3.1–3.3.
- **Fact.** RouteLLM trains a binary "win prediction" router `P_θ(wins|q)` from Chatbot Arena human preference data and routes with a **cost threshold** `α`: route to the strong model iff `P(wins|q) ≥ α`. Four parameterizations: similarity-weighted (SW) Bradley–Terry ranking, matrix factorization, BERT classifier, causal LLM classifier (Llama 3 8B). Source: RouteLLM, arXiv 2406.18665v4, §3.1 (Eq. 2), §4.2.
- **Fact.** "Task-classification" routers categorize queries into pre-defined categories and pick the best-scoring model — e.g., vLLM Semantic Router uses a ModernBERT categorizer. Source: RouterArena, arXiv 2510.00202v3, §6.1 Router Selection.

### 2. Measured cost/quality tradeoffs — cascades (multiple calls, stop on confidence)

FrugalGPT (cascade of 3, learned confidence thresholds):

- **Fact.** Cost savings to match the best individual LLM's accuracy: **98.3% on HEADLINES** (GPT-4 best), **73.3% on OVERRULING** (GPT-4), **59.2% on COQA** (GPT-3 best). Source: FrugalGPT, arXiv 2305.05176, Table 3.
- **Fact.** At the *same cost*, FrugalGPT improves accuracy by up to 5% (Fig. 5 caption); the abstract states 4%. Source: FrugalGPT, arXiv 2305.05176, §4, Fig. 5, Abstract.
- **Fact.** On OVERRULING it simultaneously gains 1% accuracy and cuts cost 73% vs GPT-4. Source: FrugalGPT, §4 "Performance and Cost Trade-offs".
- **Fact.** Example learned thresholds (HEADLINES, budget $6.5 = 1/5 of GPT-4 cost): accept GPT-J's answer if score > 0.96; else query J1-L, accept if score > 0.37; else GPT-4. Result: 80% cost cut and 1.5% accuracy *gain* over GPT-4. Source: FrugalGPT, §4 "A Case Study".
- **Fact.** Cheap models are complementary: GPT-4 is wrong while GPT-J/J1-L/GPT-Curie are right on ~6% of HEADLINES queries; GPT-4 is wrong but GPT-3 right on 13% of COQA (upper bounds, not always achieved). Source: FrugalGPT, §4 "LLM diversity", Fig. 4.
- **Fact.** API price spread justifies routing: costs across 12 APIs from 5 providers differ by ~2 orders of magnitude (GPT-4 $30 vs GPT-J $0.20 per 10M input tokens, March 2023 pricing). Source: FrugalGPT, Table 1.

### 3. Measured tradeoffs — single-call binary routers

Hybrid LLM (arXiv 2404.14618v1, Table 1) — quality drop in BART-score points vs all-at-large baseline, by cost advantage (fraction of queries sent to the small model):

| Model pair (small/large, gap) | @10% cost adv | @20% cost adv | @40% cost adv |
|---|---|---|---|
| Llama-2 7b / Llama-2 13b (small gap) | 0.1 / −0.1 / 0.1 | 0.1 / 0.0 / 0.0 | 0.2 / 0.1 / 0.0 |
| Llama-2 13b / GPT-3.5-turbo (medium gap) | 0.1 / −0.1 / 0.2 | 1.0 / 0.8 / 0.8 | 3.5 / 3.4 / 2.9 |
| FLAN-t5 800m / Llama-2 13b (large gap) | 2.3 / 2.2 / 2.1 | 5.8 / 5.8 / 4.7 | 13.8 / 13.1 / 10.3 |

(columns = r_det / r_prob / r_trans)
- **Fact.** Headline: **up to 40% fewer calls to the large model with no drop in response quality** (abstract); 22% fewer GPT-3.5-turbo calls at <1% BART-score drop (intro, Fig. 1c).
- **Fact.** Routing value depends sharply on the model-pair quality gap: at a 20% cost advantage the small-gap pair loses ≤0.1% BART quality, the large-gap pair loses up to 5.8%. When the gap is large, training on Pr[H≥0] labels is near-useless ("0 for almost 90% of queries"); a label-relaxation transformation (Pr[H≥−t]) recovers a usable signal. Sources: Hybrid LLM, Table 1, §3.3, Fig. 4(a).
- **Fact.** With thresholds tuned to keep BART drop <1%, achievable cost advantage is pair-dependent: Llama-2 7b/13b → **>96% cost advantage**; Llama-2 13b/GPT-3.5-turbo → only ~15%; FLAN-t5 800m/Llama-2 13b → ~5%. Source: Hybrid LLM, §4.5, Table 3.
- **Fact.** Router latency overhead is negligible relative to generation: DeBERTa router 0.036±0.002 s/query vs Llama-2 13b generation 14.61±0.27 s (router ~10× faster than the fastest LLM, FLAN-t5 800m at 0.46 s). Source: Hybrid LLM, §4.4, Table 2.
- **Fact.** Router quality transfer to new model pairs degrades with quality-gap correlation: at high correlation (r=0.76) 20% cost advantage costs ≤1.6% quality, 40% costs ≤4.1%; at low correlation (r=0.06) it fails. Source: Hybrid LLM, §4.7, Fig. 8.

RouteLLM (arXiv 2406.18665v4):

- **Fact.** Trained routers achieve **>2× cost reduction without sacrificing quality** (abstract), and up to **3.66× cost savings** at MT-Bench quality equal to 95% of GPT-4's; 1.41× at 92% GPT-4 quality on MMLU; 1.49× at 87% on GSM8K. Source: RouteLLM, Abstract, §5.4, Table 6.
- **Fact.** Cost assumptions behind that ratio: GPT-4 ≈ $24.7 per M tokens vs Mixtral 8x7B ≈ $0.24 per M tokens (~100×); pricing from gpt-4-1106-preview at $10/$30 per M input/output tokens, avg prompt 95 tokens, avg output 264 tokens. Source: RouteLLM, §5.4, Appendix D. **Estimate** — author-computed average from API pricing.
- **Fact.** Data regime dominates router quality: trained only on the 65k-sample Arena dataset, the BERT classifier is *worse than random* on MT-Bench (CPT(50%)=78.09% vs random 49.03%); adding 120k LLM-judge labels (~$700) cuts BERT's CPT(50%) to 19.58% and matrix factorization's to 13.40%. High-capacity routers (BERT, Llama-3-8B) underperform in the low-data regime; matrix factorization and SW ranking are best on sparse preference data. Sources: RouteLLM, §4.1.1, §5.1, Tables 1–3.
- **Fact.** Data augmentation direction matters per benchmark: golden-label MMLU data (1.5k samples, <2% of training data) is what fixes MMLU; judge-labeled data fixes MT-Bench and GSM8K. Source: RouteLLM, §5.1, Tables 2–3.
- **Fact.** Trained routers transfer to unseen model pairs without retraining (Claude 3 Opus/Sonnet, Llama 3.1 70B/8B): best APGR ~0.77 vs ~0.5 random. Source: RouteLLM, §5.2, Table 4.
- **Fact.** Router serving overhead: matrix factorization $3.32 per million requests, BERT $3.19, causal LLM $5.23, SW ranking $39.26 (CPU-based); most expensive router adds ≤0.4% of GPT-4 generation cost. Source: RouteLLM, §5.5, Table 7.

### 4. Cross-router comparison (RouterArena, arXiv 2510.00202v3)

- **Fact.** All 12 evaluated routers fall short of the oracle; most cluster near "100% cost / 100% accuracy of their best model", i.e., **over-rely on the strongest model** and under-use cheaper alternatives. Source: RouterArena, §6.2 "Normalized Deferral Curve", Fig. 7.
- **Fact.** Efficiency leaders are classification/confidence-style open routers: **vLLM-SR and CARROT save ~35% cost with <2% accuracy degradation**. Conversely NIRT-BERT reaches only baseline accuracy at **378% of cost**; MIRT-BERT reaches ~77% of its optimal accuracy at ~5× optimal cost. Source: RouterArena, §6.2, Fig. 7–8.
- **Fact.** Commercial routers (GPT-5, Not Diamond, Azure Model Router) reach higher accuracy but at significantly higher cost; GPT-5 ranks #7 and Not Diamond #12 overall — a restricted model pool (OpenAI-only) or expensive selections hurt the composite score. Source: RouterArena, §6.2, §6.5, Table 2.
- **Fact.** Difficulty-aware cost allocation varies: GPT-5, Azure-Router, MIRT-BERT, Not Diamond spend much more budget on hard queries; others are near-flat across difficulty. Accuracy on hard queries (≤4/42 models correct) is often <10% for most routers — large headroom. Source: RouterArena, §6.3.
- **Fact.** Robustness to paraphrased/typo'd queries is uniformly low, especially for BERT-based routers; latency of embedding-API routers (RouteLLM, vLLM-SR) is far higher than sub-100 ms local routers, which can threaten SLOs. Source: RouterArena, §6.2 "Robustness and Latency".
- **Fact.** RouterBench (the preceding benchmark) provides >405k inference outcomes and a deferral-curve comparison methodology; RouterArena extends it to a live leaderboard covering 8,400 queries, 9 domains, 3 difficulty bands. Sources: RouterBench, arXiv 2403.12031, Abstract; RouterArena, §1, §3.
- **Fact.** The Confidence-Driven LLM Router (uncertainty-estimation-based routing with LLM-as-a-judge quality scoring) claims to outperform prior routing on MT-Bench/GSM8K/MMLU while cutting cost — **I read only the abstract, not the full text, so no concrete numbers are verified here**. Source: arXiv 2502.11021, Abstract. (abstract-level claim, unverified detail)

### 5. Not verified

- I could not locate or open the paper "To Pass or Not to Pass: An Empirical Study of LLM Router Models" (attempted arXiv IDs and title search returned no match); its specific numbers are **unknown — not verifiable from the sources I reached**. Do not attribute any of the above to it.

## Sources

Primary (full text read):
1. Ong, Almahairi, Wu, Chiang, Wu, Gonzalez, Kadous, Stoica (2024). *RouteLLM: Learning to Route LLMs with Preference Data*. arXiv 2406.18665v4. https://arxiv.org/abs/2406.18665 (HTML: https://arxiv.org/html/2406.18665v4)
2. Chen, Zaharia, Zou (2023). *FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance*. arXiv 2305.05176. https://arxiv.org/abs/2305.05176 (full text: https://ar5iv.labs.arxiv.org/html/2305.05176)
3. Ding, Mallick, Wang, Sim, Mukherjee, Ruhle, Lakshmanan, Awadallah (2024). *Hybrid LLM: Cost-Efficient and Quality-Aware Query Routing*. arXiv 2404.14618v1. https://arxiv.org/abs/2404.14618 (HTML: https://arxiv.org/html/2404.14618)
4. Lu, Liu, Yuan, Cui, Zhang, Liu, Xing (2025). *RouterArena: An Open Platform for Comprehensive Comparison of LLM Routers*. arXiv 2510.00202v3. https://arxiv.org/abs/2510.00202 (HTML: https://arxiv.org/html/2510.00202v3)

Primary (abstract only):
5. Hu, Bieker, Li, Jiang, Keigwin, Ranganath, Keutzer, Upadhyay (2024). *RouterBench: A Benchmark for Multi-LLM Routing System*. arXiv 2403.12031. https://arxiv.org/abs/2403.12031
6. Zhang, Mehradfar, Dimitriadis, Avestimehr (2025). *Leveraging Uncertainty Estimation for Efficient LLM Routing*. arXiv 2502.11021. https://arxiv.org/abs/2502.11021

## Verdict

**Established.** Routing between cheap and strong LLMs yields published cost savings of roughly **2–3.7× at ~90–95% of the strong model's quality** for trained binary routers (RouteLLM), **20–40% fewer strong-model calls at ≤1–4% quality loss** for quality-gap routers (Hybrid LLM), and **50–98% cost cuts at equal accuracy** for cascades that exploit cheap-model complementarity (FrugalGPT). Three facts recur across independent papers: (1) the achievable tradeoff is dominated by the *size of the weak/strong quality gap* and the *training-data distribution match* (augmentation and low-data regimes matter more than router architecture); (2) routers over-deploy the strong model in practice (RouterArena normalized deferral curve), so the frontier is far from the oracle; (3) router overhead itself is measurable and non-trivial (latency, embedding API calls, per-request cost) but small vs generation cost.

**Uncertain.** Cross-paper comparison is apples-to-oranges: different model pools, different quality metrics (BART score vs accuracy vs LLM-judge), different cost assumptions (e.g., RouteLLM's GPT-4/Mixtral ~100× ratio), and 2023–2025 price baselines. "To Pass or Not to Pass" numbers could not be verified. Confidence-threshold cascade papers (FrugalGPT, 2502.11021) report large savings but on few datasets and with authors' own thresholds.

**What would settle it.** A single standardized protocol like RouterArena's live leaderboard (fixed cost tables, difficulty-banded queries, oracle gap, latency and robustness axes), applied to the same routers under the same model pool; plus release of trained confidence/classifier routers' threshold curves so the *same* router can be compared at matched quality (e.g., CPT-style metrics) across papers.
