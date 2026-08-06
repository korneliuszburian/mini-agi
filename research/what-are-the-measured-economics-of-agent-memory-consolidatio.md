## Findings

Scope note: "measured economics" here means published benchmark numbers (token counts, latencies, accuracy) measured by the papers themselves. All numbers below are **fact** as reported in the cited primary sources under each source's own protocol; cross-paper generalization is labeled **estimate**/**opinion**. The dominant framing in the literature is "eager/incremental consolidation" (LLM invoked on every interaction) vs "selective/batch/offline consolidation" (LLM invoked on clusters, on recurrence, or during offline "sleep" passes).

### 1. Token cost: per-run incremental extraction is the expensive regime

- **RecMem** (ACL 2026 Findings) frames the exact tradeoff: existing memory systems "invoke LLMs to process every incoming interaction for memory extraction, and such an eager memory consolidation scheme leads to substantial token consumption." Their fix triggers LLM consolidation only when semantically similar interactions recur (thresholds θ_sim=0.6–0.7, θ_count=4–5), buffering everything else in an embedding-only "subconscious" store. *Source: RecMem §1, §3.2, arXiv:2605.16045.*
- Measured construction-token costs on **LoCoMo** (GPT-4.1-mini), per conversation: Mem0 1,520.8K / A-Mem 1,459.9K (eager, per-turn) vs RecMem 193.2K (recurrence-based batch) → **−87.3% vs Mem0, −86.8% vs A-Mem**. Same setup, GPT-4o-mini: Mem0 1,233.5K / A-Mem 1,143.3K vs RecMem 202.4K (**up to ~7.8× fewer**). *Source: RecMem Table 1, §4.2, §1.*
- On **LongMemEval-S** (500 conversations, ~115K tokens avg), GPT-4.1-mini: Mem0 1,626.5K / A-Mem 1,264.3K vs RecMem 365.5K (**−77.5% / −71.1%**). *Source: RecMem Table 2, §4.2.*
- RecMem's query-time token cost stays comparable to eager baselines (e.g., LoCoMo GPT-4o-mini: 2.73K vs Mem0 1.99K), so the saving concentrates in construction — which the authors argue dominates total LLM usage in streaming deployments. **opinion → their claim**: "these construction-time differences accumulate over time and can dominate total LLM usage." *Source: RecMem §4.2 (Construction vs. query cost).*
- **LightMem** (ICLR 2026) reports total (online+offline) token usage on LongMemEval-S, GPT-4o-mini: A-MEM 1,605.8K / MemoryOS 2,991.8K / Mem0 1,152.6K vs LightMem(r=0.7,th=512) 28.25K → **10×–38× fewer total tokens** (GPT), 6.9×–21.8× (Qwen) vs baselines. *Source: LightMem Table 2, §5.2.*
- LightMem's **online-only** cost (soft updates; no offline pass yet) is far lower still: **up to 105.9× (GPT) / 117.1× (Qwen) token reduction and 159.4× / 309.9× fewer API calls** vs baselines. *Source: LightMem Abstract, §5.2.*
- **Measured cost of an explicit batch consolidation pass**: LightMem's offline "sleep-time" OP-update rows show the offline consolidation pass raises construction cost vs online-soft-update alone — e.g., LongMemEval-S GPT-4o-mini, LightMem(r=0.7,th=512): online 28.25K tokens / 18.4 calls / 283.8s runtime → with OP-update 83.44K / 125.5 calls / 496.0s. The batch pass roughly triples construction cost but stays ~14× cheaper than Mem0's eager construction (1,152.6K). *Source: LightMem Table 2.*
- **Mem0 vs full-context** (a different axis — memory vs no memory): Mem0 reports **>90% token-cost saving** and **91% lower p95 latency** vs processing the whole 26,031-token conversation each query (Mem0 retrieves ~1,764 tokens avg). *Source: Mem0 Abstract, §1, Table 2; arXiv:2504.19413.*

### 2. Latency

- **Mem0** (LOCOMO): total p95 = 1.44s (Mem0) / 2.59s (Mem0^g) vs full-context 17.12s; search p95 0.20s vs LangMem 59.82s. *Source: Mem0 Table 2, §4.3–4.4.*
- **Zep/Graphiti** (LongMemEval-S, ~115K-token conversations): response latency 3.20s (gpt-4o-mini) / 2.58s (gpt-4o) vs full-context 31.3s / 28.9s → **~90% latency reduction** while avg context drops 115K → 1.6K tokens. *Source: Zep Table 2, §4.3.2; arXiv:2501.13956.*
- **LightMem** runtime (memory-bank construction) speedups over eager baselines: **2.9×–12.4× (GPT), 1.6×–6.3× (Qwen)** on LongMemEval-S. *Source: LightMem §5.2.*
- **Sleep-time Compute** (Letta) targets latency differently: doing inference offline before queries lets you "respond at the accuracy of standard test-time compute but with far lower latencies"; it reduces **test-time tokens needed to reach the same accuracy by ~5×** (Stateful GSM-Symbolic, Stateful AIME). *Source: Sleep-time Compute Abstract, §5.1; arXiv:2504.13171.*

### 3. Quality tradeoffs

- **RecMem**: selective batch consolidation matches or beats eager systems on accuracy — best overall among memory-based methods on both benchmarks (LoCoMo GPT-4.1-mini: 81.10 vs Mem0 62.92 / A-Mem 68.83; LongMemEval-S: 76.80 vs 71.20 / 71.60, and beats FullContext 66.20). **But** on short LoCoMo conversations FullContext still slightly outperforms RecMem (76.43 vs 72.47, gpt-4o-mini), i.e., the consolidation saving is not free on small contexts. *Source: RecMem Tables 1–2, §4.2.*
- **LightMem**: higher ACC *and* lower cost than all memory baselines (LongMemEval-S GPT-4o-mini: 68.64 vs Mem0 53.61 / A-Mem 62.60 / FullText 56.80), i.e., no measured quality–cost tradeoff in their eval. Ablation: removing topic segmentation costs −6.3% ACC (GPT). *Source: LightMem Table 2, §5.4.*
- **Mem0**: full-context still scores highest on LLM-as-Judge (J≈72.90%) but at 17s p95; Mem0 J=66.88 at 1.44s — the measured accuracy/latency frontier. *Source: Mem0 Table 2, §4.3.*
- **Zep**: DMR 94.8% vs MemGPT 93.4% (gpt-4-turbo); LongMemEval-S +15.2% (gpt-4o-mini) / +18.5% (gpt-4o) over full-context **while** cutting latency ~90%; largest gains on temporal-reasoning, multi-session, single-session-preference. Caveat: single-session-assistant questions regress (−9.1% / −17.7%). *Source: Zep Abstract, Table 1, Table 3, §4.2–4.3.*
- **MemSIF** (dual-track: write-time "CoreFact" + query-driven "ActiveFact" consolidation): on LoCoMo (Qwen3-4B), deferred/query-time consolidation costs more tokens per query (Full 3,052 vs CoreFact-only 1,935 tokens/q) but buys quality, especially on low-salience evidence (LSHU subset 85.69 vs 74.45; total ACC 75.62 vs 67.43). Under Qwen3-32B it beats GAM by 5.78 ACC at −40.6% tokens/query (2.41K vs 4.06K). *Source: MemSIF Table 4, §4.4; arXiv:2608.01742.*
- **Sleep-time Compute**: amortizing one offline pass across ~10 related queries cuts **average cost per query 2.5×** (modeling test-time tokens at 10× sleep-time token cost); scaling sleep-time compute adds up to +13% (GSM) / +18% (AIME) accuracy; at *high* test-time budgets pure test-time compute is slightly better (extra pre-computed context distracts). SWE case study: ~1.5× fewer test-time tokens at low budgets. *Source: Sleep-time Compute Abstract, §5.2–5.3, §6.*
- **Auto-Dreamer** (learned offline consolidator, RL-trained): consolidating offline (batch) into a 12× smaller active memory bank improves ScienceWorld by ~7 points over fixed/RL/prompted baselines and generalizes to ALFWorld/WebArena at 6× less memory. Quality-only evidence (no token economics reported in abstract). *Source: Auto-Dreamer Abstract; arXiv:2605.20616.*

### 4. Structural / cost-driver notes (estimate/opinion)

- Complexity model (LightMem §4): eager systems are O(N) summarization+update calls per N-turn dialogue; buffered/compressed systems are O(N·r^x·T/th) where r is compression rate, T avg tokens/turn, th buffer capacity — the mechanism behind the 10×–38× token gaps. **estimate** — this is the paper's own accounting, not an independent measurement.
- Numbers are **not directly comparable across papers**: different eager baselines, backbones (GPT-4o-mini/4.1-mini/Qwen3), datasets (LoCoMo ~16K tokens vs LongMemEval-S ~115K tokens), and evaluation protocols (LLM-as-Judge vs F1) vary. **opinion** — no single published "per-run extraction vs batch consolidation" cost ratio exists; the closest measured pair is RecMem's 193K vs 1,520K (7.8×) and LightMem's 28K vs 1,153K (41×), which are different implementations of the same dichotomy.

## Sources

1. RecMem: Dai, Deng, Guan, Tian, Yao, Yan, Cheng — "RecMem: Recurrence-based Memory Consolidation for Efficient and Effective Long-Running LLM Agents," arXiv:2605.16045 (ACL 2026 Findings). https://arxiv.org/abs/2605.16045 (full text read)
2. LightMem: Fang et al. — "LightMem: Lightweight and Efficient Memory-Augmented Generation," arXiv:2510.18866 (ICLR 2026). https://arxiv.org/abs/2510.18866 (full text read)
3. Mem0: Chhikara et al. — "Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory," arXiv:2504.19413. https://arxiv.org/abs/2504.19413 (full text read)
4. Zep: Rasmussen et al. — "Zep: A Temporal Knowledge Graph Architecture for Agent Memory," arXiv:2501.13956. https://arxiv.org/abs/2501.13956 (full text read)
5. Sleep-time Compute: Lin, Snell, Wang, Packer, Wooders, Stoica, Gonzalez — arXiv:2504.13171. https://arxiv.org/abs/2504.13171 (full text read)
6. MemSIF: Luo, Xu, Yang — arXiv:2608.01742. https://arxiv.org/abs/2608.01742 (full text read)
7. MemGPT: Packer et al. — arXiv:2310.08560. https://arxiv.org/abs/2310.08560 (abstract verified; DMR baseline numbers cited via Zep Table 1)
8. Auto-Dreamer: Ye et al. — "Auto-Dreamer: Learning Offline Memory Consolidation for Language Agents," arXiv:2605.20616. https://arxiv.org/abs/2605.20616 (abstract)
9. MemInsight: Salama et al. — arXiv:2503.21760. https://arxiv.org/abs/2503.21760 (abstract; found via search, not used in claims)

Note: A-Mem / MemoryOS / LangMem numbers cited above are as *re-measured inside* RecMem and LightMem, not from A-Mem/MemoryOS/LangMem's own papers.

## Verdict

**Established (facts with published numbers):**
- Eager per-run incremental extraction is measurably the expensive regime: RecMem's recurrence-based batch consolidation cut construction tokens 87% (LoCoMo) and 78% (LongMemEval-S) vs Mem0/A-Mem at equal-or-better accuracy; LightMem's buffered + offline sleep-time design cut total tokens 10×–38× and online-only tokens up to ~106×–117× vs eager baselines.
- A batch/offline consolidation pass has its own measured price: LightMem's OP-update pass roughly triples construction cost over online-only soft updates (28.25K → 83.44K tokens) but is executed offline/parallel, so test-time latency stays ~3s-class and the pass remains ~14× cheaper than eager construction.
- Latency is where consolidation wins outright: Mem0 1.44s vs 17.1s p95 (full-context); Zep ~90% latency cut at higher accuracy; sleep-time compute cuts test-time tokens ~5× to equal accuracy.
- Quality tradeoff is context-dependent, not a universal free lunch: on short conversations full-context still beats selective systems (RecMem 72.47 vs FullContext 76.43 on LoCoMo), and LightMem's own ablation shows −6.3% ACC without topic-aware batching; on long (~100K+) contexts selective/batch systems win or tie while costing 1–2 orders of magnitude less.

**Uncertain:**
- No single paper measures both regimes head-to-head under one protocol; the two key datasets (RecMem: 7.8×; LightMem: up to ~41× vs Mem0) are different implementations, backbones, and codebases. Dollar costs are never reported — token counts only, so "economics" in currency remains an inference.
- Whether consolidation-trigger heuristics (recurrence thresholds, buffer capacity, compression rate) generalize across domains is untested beyond the two benchmarks; RecMem's own limitations section flags this.

**What would settle it:** a single-paper, same-backbone, same-budget ablation of per-run extraction vs batch consolidation on both LoCoMo and LongMemEval-S reporting (a) construction + query tokens, (b) wall-clock including the offline pass, (c) accuracy by question type, and (d) USD cost at list API prices. No such head-to-head currently exists in the sources reached.

Source-check note: all papers were read as arXiv full-text HTML; no PDF-only claims were made. I could not verify A-Mem's original paper directly (not fetched), so all A-Mem numbers here are attributed to RecMem/LightMem's re-measurements.
