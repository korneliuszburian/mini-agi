## Findings

Scope note: all mechanisms below are grounded in the cited primary sources; the papers (MemGPT, Zep/Graphiti, Generative Agents, A-MEM, LongMemEval) and first-party docs/source (Mem0, Letta, LangGraph). Where a mechanism's effectiveness number is not published by its owner, that is stated explicitly rather than invented.

### 1. The dominant pattern: the fact store is append-only and unbounded; "retention" is applied to the *in-context* tier and at retrieval time, not to storage

- **Fact (MemGPT paper, §2.2):** MemGPT explicitly separates *main context* (the LLM prompt tokens: system instructions, working context, FIFO queue) from *external context* (archival storage — "a read/write database storing arbitrary length text objects" — and recall storage — the message database). Messages evicted from the FIFO queue "are stored indefinitely in recall storage and readable via MemGPT function calls." Archival/recall storage is by design unbounded; the bounded tier is the context window. — Packer et al., "MemGPT: Towards LLMs as Operating Systems," arXiv:2310.08560 (full text via https://ar5iv.labs.arxiv.org/html/2310.08560).
- **Fact (Zep paper, §2.1):** Graphiti's episode subgraph is a "non-lossy data store" of raw messages; episodic edges "reinforce the non-lossy nature of Graphiti's episodic subgraph." Nothing in Zep deletes data. — Rasmussen et al., "Zep: A Temporal Knowledge Graph Architecture for Agent Memory," arXiv:2501.13956 (https://arxiv.org/html/2501.13956v1).
- **Fact (Mem0 docs):** Mem0's default `add` path is "ADD-only" extraction: "If a user says, 'I moved from Austin to Seattle,' Mem0 can store the new fact without silently rewriting the old one. Use explicit `update` or `delete` operations when your application needs to correct or remove a memory." — https://docs.mem0.ai/core-concepts/how-it-works.
- **Fact (LangGraph source):** LangGraph's `InMemoryStore` is a plain dict-backed store with no TTL, eviction, or expiry logic; the docstring states "This store keeps all data in memory. Data is lost when the process exits. For persistence, use a database-backed store like PostgresStore." Retention policy is delegated entirely to the backing database or the application. — https://github.com/langchain-ai/langgraph/blob/main/libs/checkpoint/langgraph/store/memory/__init__.py.
- **Opinion:** none of the reviewed systems actually delete facts as a default policy; the exceptions are explicit application-level `delete` calls (Mem0) and TTL in backing stores (not built into the memory layer itself).

### 2. Archival tier + queue eviction (MemGPT / Letta)

- **Fact (MemGPT paper, §2.2):** The queue manager enforces two thresholds: a "warning token count" (e.g. 70% of the context window) at which it inserts a system message warning the LLM of an "impending queue eviction (a 'memory pressure' warning)" so the LLM can move important data to working context or archival storage, and a "flush token count" (e.g. 100%) at which it "evicts a specific count of messages (e.g. 50% of the context window), generates a new recursive summary using the existing recursive summary and evicted messages." The FIFO queue's first slot holds that recursive summary. — arXiv:2310.08560.
- **Fact (MemGPT paper, Table 2):** On Deep Memory Retrieval (DMR, 500 MSC conversations), MemGPT with GPT-4 Turbo reaches 93.4% LLM-judge accuracy vs 35.3% for the recursive-summarization baseline, 32.1% for raw GPT-4 (→92.5% with MemGPT), 38.7% for GPT-3.5 Turbo (→66.9% with MemGPT). Eviction-to-summary is the mechanism that loses information; unbounded searchable archival storage is what recovers it.
- **Fact (Letta docs, "Memory & dreaming"):** The current product (Letta/MemGPT successor) continues the consolidation idea as *dreaming*: "background subagents to review recent conversations, consolidate useful lessons, and update memory without interrupting your active work," runnable "after a configured number of user messages or when the context window is compacted," plus a `/doctor` audit for "placement, duplication, and system-prompt token usage." — https://docs.letta.com/configuration/memory.
- **Estimate:** the 70%/100%/50% numbers are framed as examples in the paper ("e.g."), not as a tuned optimum; no reported ablation of eviction fraction was found.

### 3. Decay (retrieval-time, never deletion)

- **Fact (Generative Agents paper, §4.1):** Retrieval score = `α_recency·recency + α_importance·importance + α_relevance·relevance`, all α set to 1. Recency is "an exponential decay function over the number of sandbox game hours since the memory was last retrieved. Our decay factor is 0.995." Importance is an LLM-assigned 1–10 integer at creation ("cleaning up the room" → 2, "asking your crush out on a date" → 8). Relevance is embedding cosine similarity. Every memory object carries a creation timestamp and a most-recent-access timestamp. The memory stream itself is unbounded; decay only re-ranks at retrieval. — Park et al., "Generative Agents: Interactive Simulacra of Human Behavior," arXiv:2304.03442 (https://ar5iv.labs.arxiv.org/html/2304.03442).
- **Fact (Mem0 docs, "Memory Decay"):** Decay is "a soft ranking bias, never a filter… at worst it scales [a candidate's] score by 0.3×." Scaling factor range 0.3×–1.5× (≈1.5× just-accessed, 0.3× floor for months-idle). It is "opt-in per project and off by default"; widens the candidate pool to `top_k × 3` (floor 50) so reordering has room; reinforcement per memory is "capped at the most recent 20 touches." — https://docs.mem0.ai/platform/features/memory-decay.
- **Estimate:** Mem0 publishes the scaling band and lifecycle stages but **no measured retrieval-quality number** (e.g., recall/accuracy delta with decay on/off) on that page; effectiveness is asserted, not benchmarked there.
- **Opinion:** decay-as-ranking-bias (Mem0) and decay-as-recency-weight (Generative Agents) are the only "decay" mechanisms found; neither removes data.

### 4. Deduplication (write-time)

- **Fact (Mem0 docs):** Extraction pipeline step 3 is "Deduplication and embedding. Redundant facts are removed, then each memory is embedded"; step 1 is a "Context lookup. Mem0 checks related existing memories so it can avoid storing the same fact again." — https://docs.mem0.ai/core-concepts/how-it-works.
- **Fact (Mem0 docs, "Dream — Merge"):** Duplicates are folded into one canonical memory; the folded record "is hidden from reads by default (you get the one canonical memory), retained rather than deleted, and surfaced with `include_merged=true`." Runs "as memories are added." — https://docs.mem0.ai/platform/features/dream.
- **Fact (Zep paper, §2.2):** Two-stage dedup during ingestion: entity resolution (embedding cosine + full-text candidates → LLM duplicate check) and edge/fact dedup, where "the hybrid search for relevant edges is constrained to edges existing between the same entity pairs as the proposed new edge," which "significantly reduces the computational complexity of the deduplication process." — arXiv:2501.13956.
- **Fact (A-MEM paper, §3.3):** Rather than deduplicating by deletion, new memories *merge into* semantically near existing notes: for each near neighbor the LLM decides whether to "update its context, keywords, and tags," and the evolved note replaces the original. — Xu et al., "A-MEM: Agentic Memory for LLM Agents," arXiv:2502.12110 (https://arxiv.org/html/2502.12110v11).
- **Opinion:** across systems, dedup happens at write time as a way to keep the store from growing with near-identical facts; the dedup *rate* (how much growth is prevented) is not reported by any source reviewed.

### 5. Invalidation / supersede (marking, not deleting)

- **Fact (Zep paper, §2.2.3):** Graphiti uses a bi-temporal model: timeline T (when a fact holds) and T' (transactional ingestion). Each edge stores `t_valid`, `t_invalid`, `t'_created`, `t'_expired`. "The system employs an LLM to compare new edges against semantically related existing edges to identify potential contradictions. When the system identifies temporally overlapping contradictions, it invalidates the affected edges by setting their `t_invalid` to the `t_valid` of the invalidating edge. … Graphiti consistently prioritizes new information." The retrieval constructor returns the fact with its `t_valid, t_invalid` range so stale-but-historical facts remain representable. — arXiv:2501.13956.
- **Fact (Mem0 docs, "Dream — Supersede"):** "Dream marks the older memory as **superseded** and links it to the newer fact… Superseded memories are not deleted and not hidden by default. A normal `search` or `get` still returns them… badged as superseded"; `latest_only=true` filters them out. Runs "as memories are added, on every plan." — https://docs.mem0.ai/platform/features/dream.
- **Fact (Zep paper, Table 3):** The value of time-aware invalidation appears in LongMemEval's knowledge-update and temporal-reasoning categories: e.g. with gpt-4o, knowledge-update 78.2%→83.3% and temporal-reasoning 45.1%→62.4% vs full-context reading (see §8 for numbers and setup).
- **Opinion:** invalidation/supersede is the near-universal answer to "outdated fact": keep the history, hide or timestamp the old truth, never destroy it.

### 6. Explicit expiry (per-item TTL)

- **Fact (Mem0 docs, "Memory Expiration"):** A per-memory `expiration_date` (`YYYY-MM-DD`, evaluated in UTC, date-inclusive) makes the memory "stop surfacing in search once that date passes. Nothing is deleted… `search()` and `get_all()` skip it, fetching it by ID still returns it, and clearing the date brings it straight back." No date ⇒ never expires (the default); malformed dates "fail open." — https://docs.mem0.ai/platform/features/memory-expiration.
- **Fact (Mem0 docs):** The same page contrasts the three policies: Expiration (hides, data kept, reversible), Decay (re-ranks, data kept, never filters), Delete (permanent, irreversible). — https://docs.mem0.ai/platform/features/memory-expiration.
- **Fact (LangGraph source):** no expiry/TTL in the core `InMemoryStore` (see §1); TTL would have to come from the backing store. — github.com/langchain-ai/langgraph.

### 7. Compression/synthesis as anti-growth (the lossy tier)

- **Fact (MemGPT paper, §2.2):** the recursive summary (first FIFO slot, regenerated at each flush) is the lossy compacted representation of evicted messages.
- **Fact (Generative Agents paper, §4.2):** *reflection* recursively synthesizes observations into higher-level memories ("reflections"), which are themselves appended to the memory stream — the stream is compacted in meaning, not in size.
- **Fact (Mem0 docs, "Dream — Synthesis"):** background synthesis "distills a user's memories into higher-order **pattern memories**," written "alongside your existing memories, never in place of them," gated on ≥20 memories per user and a cadence (7 days on Pro, daily on Enterprise). — https://docs.mem0.ai/platform/features/dream.
- **Fact (Zep paper, §2.3):** community subgraphs are periodic summaries of entity clusters (label propagation, dynamically extended); the paper concedes "the resulting communities gradually diverge from those that would be generated by a complete label propagation run. Therefore, periodic community refreshes remain necessary" — i.e., even the summarization tier requires scheduled re-runs to stay accurate as facts accumulate. — arXiv:2501.13956.
- **Fact (Letta docs):** "Dreaming uses background subagents to review recent conversations, consolidate useful lessons, and update memory"; "the memory workflow backs up the current repository before splitting large files, merging duplicates, or restructuring the hierarchy." — https://docs.letta.com/configuration/memory.
- **Estimate:** no source reports a measured compression ratio (bytes or fact-count reduction) for these consolidation tiers.

### 8. Reported numbers on retrieval quality under growth (with sources)

- **Zep vs MemGPT vs full-context** (Zep paper, Tables 1–3; LongMemEvalS = ~115k-token histories, avg; Zep retrieved 10–20 facts/entities per query):
  - DMR (500 MSC conversations): MemGPT 93.4%, Zep 94.8%, full-conversation baseline 94.4%, session-summary baseline 78.6%, recursive summarization 35.3% (all gpt-4-turbo). — arXiv:2501.13956 Table 1.
  - LongMemEvalS: Zep 63.8% (gpt-4o-mini) / 71.2% (gpt-4o) vs full-context 55.4% / 60.2%; latency 3.20 s / 2.58 s vs 31.3 s / 28.9 s (~90% lower); average context tokens 1.6k vs 115k. "accuracy improvements of up to 18.5%" (gpt-4o; 15.2% for gpt-4o-mini, per paper text §4.3.2). — arXiv:2501.13956 Tables 2–3.
  - Caveat (Zep paper itself, §4.2): DMR "each conversation contains only 60 messages, easily fitting within current LLM context windows," so DMR barely exercises retention. — arXiv:2501.13956.
- **LongMemEval benchmark findings** (Wu et al., "LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory," arXiv:2410.10813, ICLR 2025):
  - Long-context LLMs lose 30–60% accuracy reading the full LongMemEvalS history vs oracle retrieval; GPT-4o: 0.870 oracle → 0.606 full-history (30.3% drop). — https://arxiv.org/abs/2410.10813, Fig. 3(b).
  - Commercial memory systems on histories ~10× shorter: ChatGPT (GPT-4o) 0.5773 vs offline reading 0.9184 (37% drop); Coze 0.3299 (64% drop). — Fig. 3(a).
  - Design deltas under growth: fact-augmented key expansion ↑ memory recall@k by 9.4% and QA accuracy by 5.4%; time-aware query expansion ↑ temporal-reasoning recall by 6.8–11.3%; Chain-of-Note + structured format ↑ QA accuracy by up to 10 absolute points. — Abstract §1 and §5.
  - Storage-granularity finding relevant to summarization-as-retention: compressing sessions into individual user facts "harms overall performance due to information loss, but it improves the multi-session reasoning accuracy." — §1, §5.2.
- **A-MEM scaling under unbounded growth** (A-MEM paper, Table 4): retrieval time grows from 0.31 μs (1,000 memories) to 3.70 μs (1,000,000 memories) with linear O(N) memory (1.46 MB → 1464.84 MB); per-operation cost ~1,200 tokens vs ~16,900 for MemGPT/LoCoMo baselines ("85–93% reduction"). These are the authors' measured figures on their own system. — arXiv:2502.12110.
- **No numbers published:** Mem0's decay/dream/expiration pages state mechanics but report no before/after retrieval-quality or storage-growth figures on those pages. The Zep and Mem0 retention-quality comparisons above are the only head-to-head numbers found; direct "decay vs no-decay" controlled numbers were not found in the sources reached.

## Sources

1. Packer et al. (2023/2024), *MemGPT: Towards LLMs as Operating Systems*, arXiv:2310.08560 — https://arxiv.org/abs/2310.08560; full text https://ar5iv.labs.arxiv.org/html/2310.08560
2. Rasmussen et al. (2025), *Zep: A Temporal Knowledge Graph Architecture for Agent Memory*, arXiv:2501.13956 — https://arxiv.org/abs/2501.13956; full text https://arxiv.org/html/2501.13956v1
3. Park et al. (2023), *Generative Agents: Interactive Simulacra of Human Behavior*, arXiv:2304.03442 (UIST '23) — https://arxiv.org/abs/2304.03442; full text https://ar5iv.labs.arxiv.org/html/2304.03442
4. Xu et al. (2025), *A-MEM: Agentic Memory for LLM Agents*, arXiv:2502.12110 (NeurIPS 2025) — https://arxiv.org/abs/2502.12110; full text https://arxiv.org/html/2502.12110v11
5. Wu et al. (2024/2025), *LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory*, arXiv:2410.10813 (ICLR 2025) — https://arxiv.org/abs/2410.10813; full text https://arxiv.org/html/2410.10813v2
6. Mem0 docs, "Memory Decay" — https://docs.mem0.ai/platform/features/memory-decay
7. Mem0 docs, "Dream" — https://docs.mem0.ai/platform/features/dream
8. Mem0 docs, "Memory Expiration" — https://docs.mem0.ai/platform/features/memory-expiration
9. Mem0 docs, "How Mem0 Works" — https://docs.mem0.ai/core-concepts/how-it-works
10. Letta docs, "Memory & dreaming" — https://docs.letta.com/configuration/memory
11. LangGraph source, `langgraph.store.memory.InMemoryStore` — https://github.com/langchain-ai/langgraph/blob/main/libs/checkpoint/langgraph/store/memory/__init__.py

## Verdict

**Established (fact, primary sources):** Agent memory systems almost universally keep the long-term fact store append-only and unbounded, and solve "growth" by bounding only the *in-context* tier and by re-ranking/superseding at retrieval: (a) MemGPT/Letta evicts the context queue to a recursive summary while archival/recall storage grows indefinitely; (b) decay exists as a retrieval-time ranking bias (exponential recency factor 0.995 in Generative Agents; a 0.3×–1.5× soft scaling band in Mem0) and never deletes; (c) dedup is write-time (Mem0 context-lookup/merge, Zep entity+edge resolution, A-MEM merge-into-note); (d) invalidation/supersede is timestamp-based and non-destructive (Graphiti bi-temporal `t_valid/t_invalid`; Mem0 Dream supersede); (e) per-item expiry exists as soft-hide (Mem0 `expiration_date`), and is absent from LangGraph's core store. Head-to-head numbers exist: Zep vs MemGPT vs full-context on DMR (94.8/93.4/94.4%) and LongMemEvalS (71.2% vs 60.2% full-context, 1.6k vs 115k context tokens, ~90% lower latency); LongMemEval's 30–60% degradation for long-context LLMs and 37–64% for commercial assistants over growing histories; A-MEM's flat retrieval-time scaling to 1M memories (3.70 μs).

**Uncertain:** No controlled experiment was found isolating any single retention policy (e.g., decay on/off, eviction threshold, supersede vs delete) with retrieval-quality numbers — such numbers are published only for whole systems (Zep, MemGPT) on fixed benchmarks, not for the retention mechanism in isolation. Mem0 publishes decay/dream mechanics but no effectiveness figures. No source reports dedup rates, compression ratios, or fact-count growth over time, so "does it actually bound storage" is unanswered in every source reviewed.

**What would settle it:** (1) a controlled ablation of one policy at a time on a fixed corpus with a fixed retriever (e.g., recall@k and QA accuracy with vs without decay, and at 3 eviction thresholds); (2) longitudinal storage-growth logs (fact count / byte count over N sessions) for each system with and without dedup/consolidation enabled; (3) a published benchmark like LongMemEval scored *per question category* (knowledge-update, temporal-reasoning) for each retention policy, which is exactly the axis the Zep and LongMemEval papers already expose.

No PDFs were partially unreadable: all cited full texts were read as HTML (arXiv/ar5iv) or native docs; no content was guessed.
