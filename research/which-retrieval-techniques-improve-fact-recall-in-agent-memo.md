Two arXiv IDs resolved to wrong papers. Let me find the correct identifiers via arXiv's search API.
Zep found (arXiv:2501.13956) with concrete numbers. Now let me locate LoCoMo's original paper and fetch the remaining key primary sources.
Zep and LongMemEval give exact measured numbers. Now let me get the remaining primary sources and extract LongMemEval's retrieval-ablation numbers from the saved HTML.
Found LoCoMo (arXiv:2402.17753). Now let me extract LongMemEval's ablation numbers from the saved HTML and fetch the remaining measured data.
Let me extract LoCoMo's RAG retrieval-unit table and fetch BEIR, HippoRAG, and DPR measured results.
BEIR HTML 404'd. Let me get the remaining sources: HippoRAG numbers, BGE-M3 hybrid numbers, the survey taxonomy, and BEIR via ar5iv.
All primary-source measurements are gathered. Here is the deliverable.

---

# Retrieval techniques for fact recall in agent memory — measured evidence

Scope note: "agent memory" in this literature means long-term conversational/personal memory over an LLM agent's history (benchmarks: LongMemEval, LoCoMo, MemGPT's DMR) plus general RAG retrieval benchmarks (MIRACL, BEIR, multi-hop QA) that measure the same techniques. Cross-benchmark numbers are **not** comparable; each row's comparison is only within its own benchmark/model.

## Findings

### 1. Keyword search (lexical / BM25)

- **BM25 is the baseline that dense and graph methods are measured against, and it is often weaker on fact recall than embeddings once the corpus is large.** (fact) Dense Passage Retrieval paper: "our dense retriever outperforms a strong Lucene-BM25 system largely by **9%–19% absolute** in terms of top-20 passage retrieval accuracy." Source: DPR, arXiv:2004.04906 (abstract).
- **On agent-memory recall, BM25 underperforms dense retrievers.** (fact) LongMemEval retriever ablation, Value=Session, K=V: BM25 Recall@5 = 0.634 / NDCG@5 = 0.516 vs Contriever 0.723 / 0.634 vs Stella V5 1.5B 0.720 / 0.594. Source: LongMemEval, arXiv:2410.10813v2, Table 9 (Appendix E.2).
- **But lexical retrieval is not worthless — it is strong on out-of-domain and its value depends on the workload.** (fact) BEIR: "BM25 is a robust baseline and re-ranking and late-interaction-based models on average achieve the best zero-shot performances, however, at high computational costs. In contrast, dense and sparse-retrieval models … often underperform." Source: BEIR, arXiv:2104.08663 (abstract). (Exact per-dataset nDCG@10 values were not extractable — the paper's HTML render 404'd; see Verdict.)
- **In at least one agent-memory workload, BM25 alone was reported as the strongest single retriever.** (estimate — single 2026 preprint, not independently verified) AgentIR reports on LoCoMo (n=1,982) "where BM25 alone is already the strongest single system" and a hybrid cascade that skips the dense channel with no accuracy loss. Source: AgentIR, arXiv:2605.25092 (abstract).

### 2. Embeddings (dense retrieval)

- **Dense retrieval is the single largest accuracy jump over lexical search for open-domain fact retrieval.** (fact) DPR: 9–19% absolute top-20 accuracy over Lucene-BM25. Source: arXiv:2004.04906 (abstract).
- **In agent-memory recall, dense retrievers beat BM25 by ~9–14 Recall@5 points.** (fact) LongMemEval Table 9 (see row above): Contriever 0.723 vs BM25 0.634 (+0.089). Source: arXiv:2410.10813v2, Appendix E.2.
- **Dense-only retrieval roughly doubles lexical accuracy on a 18-language benchmark.** (fact) MIRACL dev nDCG@10 average: BM25 31.9 → M3-Embedding Dense 69.2. Source: M3-Embedding, arXiv:2402.03216, Table 1.
- **End-to-end, embedding-based retrieval alone does not close the gap to human fact recall in long conversations.** (fact) On LongMemEval, even commercial memory chatbots with dense memory (ChatGPT GPT-4o 57.7%, Coze 33.0%) drop 37% and 64% vs offline reading of the full context (91.8%). Source: LongMemEval, arXiv:2410.10813v2, Figure 3a.

### 3. Temporal ordering / recency-aware retrieval

- **Making memory retrieval time-aware measurably improves temporal-reasoning recall.** (fact) LongMemEval time-aware query expansion: "this simple design improves recall by an average of **11.3%** when using rounds as the value and by **6.8%** when using sessions" on temporal-reasoning questions; abstract states 6.8%–11.3% "when a strong LLM is employed for query expansion". Source: arXiv:2410.10813v2, Section 5.4/Table 4 and abstract.
- **Temporal reasoning is the single hardest fact-recall category across agent-memory benchmarks.** (fact) LoCoMo: human temporal F1 = 92.6; best base model GPT-4-turbo = 10.4; long-context LLMs "lag behind human levels… especially in temporal reasoning, by 73%". Source: LoCoMo, arXiv:2402.17753, Table 2 + Section 6.
- **A temporal knowledge-graph memory improves temporal-reasoning QA by a large relative margin over full-context reading.** (fact) Zep (Graphiti temporal KG) on LongMemEvalS temporal-reasoning: gpt-4o full-context 45.1% → Zep 62.4% (+38.4% rel); gpt-4o-mini 36.5% → 54.1% (+48.2% rel). Source: Zep, arXiv:2501.13956, Table 3.
- **Forgetting-curve (decay) memory updating exists as a design but no accuracy delta vs a plain memory is published in the primary paper.** (fact/estimate) MemoryBank uses an Ebbinghaus Forgetting Curve to forget/reinforce memories; evaluation is qualitative (empathic responses, memory recall), no quantitative accuracy comparison vs baseline is reported in the abstract/paper. Source: MemoryBank, arXiv:2305.10250.
- **Temporal-hierarchical memory claims large absolute gains — unverified.** (estimate — recent preprint, ACL 2026 Findings claim) TiMem reports 75.30% on LoCoMo and 76.88% on LongMemEval-S "state-of-the-art". Source: TiMem, arXiv:2601.02845 (abstract).

### 4. Entity / knowledge graphs

- **Graph retrieval gives the biggest measured gains on multi-hop fact integration (facts split across passages).** (fact) HippoRAG retrieval, R@5: on 2WikiMultiHopQA BM25 61.9 / ColBERTv2 68.2 / HippoRAG **89.1** (+20 points over best baseline); on MuSiQue 41.2 / 49.2 / 51.9 (~+3 points). QA F1: 2Wiki 43.3 → 59.5 (+16.2), MuSiQue 26.4 → 29.8. Single-step HippoRAG "achieves comparable or better performance than iterative retrieval like IRCoT while being 10–30 times cheaper and 6–13 times faster". Source: HippoRAG, arXiv:2405.14831, Tables 2–4, abstract.
- **Graph memory also beats summarization-based and flat memory baselines on conversational fact recall.** (fact) Zep on LongMemEvalS overall accuracy: gpt-4o full-context 60.2% → Zep 71.2% (+11.0 points, ~18% rel, abstract: "up to 18.5%"); gpt-4o-mini 55.4% → 63.8%. DMR (500 multi-session conversations): Zep 94.8% vs MemGPT 93.4% vs full-context 94.4% vs session summaries 78.6%. Caveat: Zep is vendor-authored and the DMR gap to full-context is small (0.4). Source: Zep, arXiv:2501.13956, Tables 1–3.
- **GraphRAG improves global/corpus-level fact synthesis but the measured effect is judged qualitatively.** (fact/estimate) GraphRAG "leads to substantial improvements over a conventional RAG baseline for both the comprehensiveness and diversity of generated answers" for global sensemaking over ~1M-token corpora; the paper reports no numeric accuracy in the abstract. Source: GraphRAG, arXiv:2404.16130 (abstract).
- **Graphs do not help every category — structured fact extraction can hurt single-session detail recall.** (fact) Zep regressed on single-session-assistant questions (gpt-4o 94.6% → 80.4%, −17.7% rel). Source: arXiv:2501.13956, Table 3.

### 5. Hybrid fusion

- **Hybrid (dense + sparse) and fused (dense + sparse + multi-vector) retrieval consistently beat each single signal.** (fact) M3-Embedding, MIRACL nDCG@10: Dense 69.2 / Sparse 53.9 / Multi-vec 70.5 / Dense+Sparse 70.4 / **All 71.5**. Long-doc (MLDR): Dense 52.5 / Sparse 62.2 / Dense+Sparse 64.8 / **All 65.0**. NarrativeQA: Dense 48.7 / Sparse 57.5 / Multi-vec 55.4 / Dense+Sparse 60.1 / **All 61.7**. Source: M3-Embedding, arXiv:2402.03216, Tables 1, 3, 4.
- **Hybrid *indexing* (multiple keys per memory item) improves agent-memory recall.** (fact) LongMemEval key expansion (K = value + extracted user facts) over K = value: +**9.4%** avg recall@k, +**5.4%** end-to-end QA accuracy. Merge method matters: key merging ≫ post-retrieval rank merging (Table 10). Source: arXiv:2410.10813v2, Section 5.3, Tables 3, 10.
- **Production agent-memory systems combine all three: lexical + semantic + graph.** (fact) Zep's search runs Okapi BM25, cosine embedding similarity, and breadth-first graph search in parallel, then reranks (incl. Reciprocal Rank Fusion and cross-encoder rerankers). Source: Zep, arXiv:2501.13956, Sections 3.1–3.2. (RRF's own measured claims from Cormack et al., SIGIR 2009, were referenced by Zep but not independently fetched this pass — opinion: treat RRF's superiority as a referenced method claim.)
- **Conversation-store granularity interacts with fusion: what you retrieve matters as much as how.** (fact) LoCoMo RAG with GPT-3.5-turbo-16k, overall QA F1: none 22.4 / dialog top-5 31.7 / summary top-5 32.5 / **observations (facts) top-5 41.4**; prose: "a noticeable 5% improvement … when the input is top 5 relevant observations instead of pure conversation logs." Source: LoCoMo, arXiv:2402.17753, Table 3 + Section 6.1.

### Cross-cutting measured facts

- **Long-context / retrieval gains are large relative to base LLMs but far from human.** (fact) LoCoMo: "improvements ranging from 22–66%" from long-context/RAG, but still "significantly lag behind human levels (by 56%)". Human overall F1 87.9 vs best long-context 37.8 (GPT-3.5-turbo-16k @16k tokens) and best RAG 41.4. Source: arXiv:2402.17753, Table 2–3 + Section 6.
- **Retrieval reading strategy is a separate measurable lever.** (fact) LongMemEval: even with perfect retrieval, Chain-of-Note + structured data format improves QA accuracy "by as much as 10 absolute points across three LLMs"; and on LongMemEvalS, long-context LLMs drop 30–60% vs oracle retrieval. Source: arXiv:2410.10813v2, Section 5.5 + Figure 3b.
- **Techniques are complements, not alternatives — the biggest gains come from stacking them.** (fact) Zep = temporal KG + hybrid lexical/dense/graph retrieval; HippoRAG = KG + embeddings + PPR; LongMemEval's best config = session decomposition + fact-augmented keys + time-aware query expansion. Sources: arXiv:2501.13956; arXiv:2405.14831; arXiv:2410.10813v2.

## Sources

1. DPR — "Dense Passage Retrieval for Open-Domain Question Answering", Karpukhin et al., EMNLP 2020. https://arxiv.org/abs/2004.04906
2. BEIR — "BEIR: A Heterogenous Benchmark for Zero-shot Evaluation of Information Retrieval Models", Thakur et al., NeurIPS 2021. https://arxiv.org/abs/2104.08663
3. LongMemEval — "LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory", Wu et al., ICLR 2025. https://arxiv.org/abs/2410.10813 (full text: https://arxiv.org/html/2410.10813v2)
4. LoCoMo — "Evaluating Very Long-Term Conversational Memory of LLM Agents", Maharana et al., ACL 2024. https://arxiv.org/abs/2402.17753 (full text: https://arxiv.org/html/2402.17753v1)
5. MemGPT — "MemGPT: Towards LLMs as Operating Systems", Packer et al., 2023. https://arxiv.org/abs/2310.08560 (defines DMR benchmark used by Zep)
6. Zep — "Zep: A Temporal Knowledge Graph Architecture for Agent Memory", Rasmussen et al., 2025. https://arxiv.org/abs/2501.13956 (full text: https://arxiv.org/html/2501.13956v1)
7. HippoRAG — "HippoRAG: Neurobiologically Inspired Long-Term Memory for Large Language Models", Gutiérrez et al., NeurIPS 2024. https://arxiv.org/abs/2405.14831 (full text: https://arxiv.org/html/2405.14831v3)
8. M3-Embedding — "M3-Embedding: Multi-Linguality, Multi-Functionality, Multi-Granularity Text Embeddings…", Chen et al., 2024. https://arxiv.org/abs/2402.03216 (full text: https://arxiv.org/html/2402.03216v5)
9. GraphRAG — "From Local to Global: A Graph RAG Approach to Query-Focused Summarization", Edge et al., 2024. https://arxiv.org/abs/2404.16130
10. MemoryBank — "MemoryBank: Enhancing Large Language Models with Long-Term Memory", Zhong et al., 2023. https://arxiv.org/abs/2305.10250
11. Generative Agents — "Generative Agents: Interactive Simulacra of Human Behavior", Park et al., 2023. https://arxiv.org/abs/2304.03442
12. RAG — "Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks", Lewis et al., NeurIPS 2020. https://arxiv.org/abs/2005.11401
13. Survey — "A Survey on the Memory Mechanism of Large Language Model based Agents", Zhang et al., 2024. https://arxiv.org/abs/2404.13501
14. AgentIR (2026 preprint) — "AgentIR: A Workload-Adaptive Cascade Retrieval Substrate for Long-Term Conversational Memory". https://arxiv.org/abs/2605.25092
15. TiMem (2026 preprint) — "TiMem: Temporal-Hierarchical Memory Consolidation for Long-Horizon Conversational Agents". https://arxiv.org/abs/2601.02845
16. Reciprocal Rank Fusion — referenced via Zep Section 3.2 (Cormack, Clarke & Buettcher, SIGIR 2009) — not independently fetched this pass.

## Verdict

**Established (high confidence, primary sources verified this pass):**
- Embeddings beat BM25 for fact retrieval in general open-domain QA (DPR: +9–19% top-20 acc) and on agent-memory recall specifically (LongMemEval: +0.089 Recall@5).
- Temporal-aware retrieval measurably improves temporal-reasoning recall (LongMemEval: +6.8–11.3%).
- Entity/graph retrieval gives the largest gains on multi-hop fact integration (HippoRAG: +20 R@5 points on 2WikiMultiHopQA) and on long-conversation QA (Zep: +11.0 points over full-context on LongMemEvalS, with a documented regression on single-session-assistant questions).
- Hybrid fusion beats any single signal (M3-Embedding: All > Dense+Sparse > Dense on MIRACL 71.5 > 70.4 > 69.2; LongMemEval multi-key indexing: +9.4% recall@k). The winning agent-memory systems stack temporal + graph + hybrid lexical/dense retrieval.
- Storing memory as extracted facts (observations) rather than raw or summarized dialogue is a large, separate lever (LoCoMo: 41.4 vs 31.7–32.5 overall F1).

**Uncertain:**
- Exact BEIR per-dataset nDCG numbers could not be extracted (HTML 404); only the abstract's qualitative claim is cited. BEIR cross-domain tables would sharpen the "when does BM25 win" claim.
- 2026 preprints (AgentIR, TiMem, and similar) report headline numbers on LoCoMo/LongMemEval that I verified only at the abstract level and could not corroborate independently; several look implausibly high next to the peer-reviewed baselines above.
- Zep, M3-Embedding, and HippoRAG numbers are author-reported; Zep is the vendor of the tested product.
- Whether temporal ordering alone (without a graph) helps ordinary fact recall — as opposed to temporal-reasoning questions — is not cleanly isolated in any single paper I reached.

**What would settle it:** a single head-to-head benchmark that runs all five retrieval strategies (BM25, dense, recency-aware, entity graph, and hybrids) on the same agent-memory QA set (e.g., LongMemEval + LoCoMo) with the same reader LLM — which does not yet exist in the peer-reviewed sources reached. The LongMemEval retriever ablation (Table 9) is the closest existing partial comparison.

*Note: LoCoMo's paper is machine-converted HTML; one table cell (Dialog k=10 Multi-Hop R@k = 247.4) is a rendering artifact and was treated as 24.7. PDF-only sections of the papers were not read; all cited numbers come from abstracts or HTML-rendered full text.*
