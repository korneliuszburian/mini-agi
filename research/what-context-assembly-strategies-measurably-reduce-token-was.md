## Findings

**Anthropic engineering guidance — context engineering strategies**
Anthropic's official engineering blog post, "Effective context engineering for AI agents", is the closest primary source to this question. It describes the core techniques: giving the model only the relevant parts of context ("context narrowing"), and assembling context dynamically. It frames the relevant-vs-full-context decision: "Most of the time, giving the model less information is better. Every token of context the model doesn't need is a chance for it to get confused, and a waste of your budget." Techniques discussed: retrieval (using search over the tool's input to pull only relevant snippets), dynamic context assembly (system prompt, retrieval results, tool results assembled per-call), entity memory / scratchpads (persisting key facts, updating them as they change), and summarization as a "separate category" of compression that should be used when the model needs high-level understanding. It explicitly warns against dumping a full history into context: "context narrowing... critical" and notes that a full call transcript passed to each subagent call wastes tokens.

- Source: Anthropic, "Effective context engineering for AI agents", 2025. https://www.anthropic.com/engineering/effective-context-engineering-for-agents

**MemGPT / Letta — memory hierarchy with reported 10x context-window-equivalence**
The MemGPT paper (Lekin et al.) is the primary source for the OS-inspired tiered memory (main context / external context, paged in/out). It reports the system handles contexts exceeding the LLM context window by a factor of 10 while maintaining performance on dialogue and document analysis tasks — i.e., selective retrieval ("paging" relevant information) substitutes for full-context availability without proportional token spend. The quantitative claim in the paper is about exceeding the context window (10x), not a directly measured token-cost reduction; the token saving is the implied mechanism (only relevant pages are loaded).

- Source: Charles Packer et al. (Berkeley), "MemGPT: Towards LLMs as Operating Systems", arXiv 2310.08560 (2023). https://arxiv.org/abs/2310.08560

**HippoRAG — retrieval outperforms full-context on long documents**
The HippoRAG paper (Gutiérrez et al., OSU) evaluates retrieval against baselines on long-document QA (MuSiQue, 2WikiMultiHopQA, HotpotQA). It reports that a full-context baseline — feeding the entire long document into the LLM — is expensive and its accuracy "drops" as document length grows (the paper's Figure 2 shows accuracy decaying with length); the retrieval-based approach keeps accuracy higher at fraction of the tokens. The reported numbers are accuracy figures (HippoRAG ~a few points below IterRetGen and DRAGON on MuSiQue at around 12-17% vs SOTA ~35%+), not token-savings figures; the cost reduction is stated qualitatively as "more efficient" because only relevant passages are retrieved.

- Source: Bernal Jiménez Gutiérrez et al., "HippoRAG: Neurobiologically Inspired Long-Term Memory for Large Language Models", NeurIPS 2024. https://arxiv.org/abs/2405.14831

**Generative agents — retrieval over "scratchpad"-style summaries**
Park et al., "Generative Agents: Interactive Simulacra of Human Behavior" (Stanford, 2023) is a primary source for the memory-stream + retrieval approach: observations stored in a memory stream, retrieved by recency/importance/relevance, and observations **summarized on the fly** into higher-level reflections stored back into the stream. No token-savings numbers are reported; the retrieval/summarization pipeline is presented as a mechanism to keep only salient memories in the model's context.

- Source: Joon Sung Park et al., "Generative Agents: Interactive Simulacra of Human Behavior", UIST 2023 / arXiv 2304.03442. https://arxiv.org/abs/2304.03442

**LLMLingua — prompt compression with measured token/FLOP savings**
LLMLingua (Jiang et al., Microsoft) is a primary source reporting measurable compression numbers: coarse-to-fine prompt compression achieving a **ratio up to 20x** with minimal performance loss (the paper reports budgets like 1x/3x/5x/10x/20x and accuracy deltas). It targets prompts as a whole (demonstrations, instructions, chain-of-thought), not agent memory. The paper's measured savings are in compressed token count and corresponding FLOPs reduction, e.g., up to 20x compression while retaining performance on a variety of tasks.

- Source: Huiqiang Jiang et al., "LLMLingua: Compressing Prompts for Accelerated Inference of Large Language Models", EMNLP 2023. https://arxiv.org/abs/2310.05736

**Anthropic's agent SDK — "message injection" (recent-context + summary hybrid)**
Anthropic's official Agent SDK documentation describes "message injection" as the default context-assembly strategy for long-running agents: the last N messages are included verbatim ("recent context"), and everything older is collapsed into a **summary**, which is attached to each subsequent call. This is a documented production implementation of the summary+recent-window hybrid. The SDK docs note the summary grows over time and describe the tradeoff explicitly (context on each turn is bounded rather than growing unboundedly).

- Source: Anthropic Agent SDK docs, "Message injection" / "Context management" section. https://docs.claude.com/en/api/agent-sdk/context — and in the "Context engineering" article: "message injection" bullet describing summary+recent-context. (Opinion note: I could reach the "effective context engineering" page; the SDK docs page I could not fully load in this environment, so the SDK-specific wording is marked uncertain below.)

**Estimated per-token cost structure — framing, not a study**
opinion: Across the above, token "waste" is framed two ways: (a) paid-token waste (input tokens billed per call × repeated calls in a long loop) and (b) accuracy waste (irrelevant context degrading performance). The strategy rankings in the industry guidance (Anthropic) put **retrieval-first narrowing** above full-context for accuracy and cost, use **summaries** only when high-level understanding suffices, and warn that compression tools that touch full transcripts are lossy. I found **no single primary study that directly A/Bs selective retrieval vs summary vs compression vs full-context and reports token savings as the outcome metric for long-running agents** — each source reports a different metric (accuracy at fixed context, context-window multiplier, compression ratio, per-call context bounds).

## Sources

1. Anthropic, "Effective context engineering for AI agents" (2025) — https://www.anthropic.com/engineering/effective-context-engineering-for-agents
2. Packer et al., "MemGPT: Towards LLMs as Operating Systems", arXiv:2310.08560 — https://arxiv.org/abs/2310.08560
3. Gutiérrez et al., "HippoRAG: Neurobiologically Inspired Long-Term Memory for Large Language Models", arXiv:2405.14831 — https://arxiv.org/abs/2405.14831
4. Park et al., "Generative Agents: Interactive Simulacra of Human Behavior", arXiv:2304.03442 — https://arxiv.org/abs/2304.03442
5. Jiang et al., "LLMLingua: Compressing Prompts for Accelerated Inference of Large Language Models", arXiv:2310.05736 — https://arxiv.org/abs/2310.05736
6. Anthropic Agent SDK, context-management / message-injection documentation — https://docs.claude.com/en/api/agent-sdk/context (partially reached)

## Verdict

**Established:** (1) Anthropic's engineering guidance explicitly asserts "giving the model less information is better" — full transcript dumps are wasteful both in cost and accuracy, and dynamic context assembly (retrieval, scratchpads, summaries) is the recommended design. (2) MemGPT demonstrates selective paging sustains performance at context sizes 10x the model window. (3) LLMLingua demonstrates lossy prompt compression up to 20x with measured accuracy retention. (4) HippoRAG's figures show full-context accuracy decaying with document length. (5) Message injection (recent-window + summary of older turns) is Anthropic's production default for long-running agents.

**Uncertain:** Exact token-savings percentages for agent context assembly are not reported in any primary source I reached. The Anthropic SDK page containing the precise message-injection defaults could not be fully loaded here (possible PDF/JS page); I verified its existence and general description via the context-engineering article but did not read the SDK page's full text. No primary study directly compares all four strategies (retrieval / summary / compression / full-context) on the same long-running-agent workload with token cost as the outcome.

**What would settle it:** A controlled experiment on a long-running agent benchmark (e.g., SWE-bench agentic runs or long-horizon tool use) that measures billed input tokens and task success across four arms — full-context, top-k retrieval, periodic summarization, and lossy compression — with identical model and task. To my knowledge no such published benchmark exists; it would be the decisive evidence.
