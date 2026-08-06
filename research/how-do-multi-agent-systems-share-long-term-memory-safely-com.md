## Findings

All claims below are from primary sources fetched directly (vendor docs, arXiv paper full text). Labels: **[fact]** = stated by the cited source about that system; **[analysis]** = my inference from the cited source's documented API; **[opinion]** = judgment, not sourced.

### 0. Terminology: what "long-term memory" means in the sources

- **[fact]** CoALA (Sumers/Yao/Narasimhan/Griffiths, arXiv:2309.02427v3, §4.1) defines memory as working memory plus long-term episodic, semantic, and procedural memory; "learning" = writing to long-term memory; retrieval = reading it back. https://arxiv.org/html/2309.02427v3
- **[fact]** CoALA explicitly frames writing that can corrupt later behavior: learning "by writing to procedural memory... is significantly riskier than writing to episodic or semantic memory, as it can easily introduce bugs or allow an agent to subvert its designers' intentions" (§4.1). https://arxiv.org/html/2309.02427v3
- **[fact]** CoALA contains **no** treatment of shared memory between agents, memory privacy, or isolation (the words "privacy"/"isolation" do not occur in the body) — verified against full text. Multi-agent work is only mentioned as grounding/debate/collaboration, and Generative Agents are described as each holding their *own* episodic memory. https://arxiv.org/html/2309.02427v3
- **[fact]** Framework docs define long-term memory operationally: LangGraph — "long-term, cross-thread memory" via Stores vs "short-term, thread-scoped memory" via Checkpointers. https://docs.langchain.com/oss/python/langgraph/persistence

### 1. Shared stores and write-permission models

- **[fact]** LangGraph's Store is "arbitrary key-value data accessible from any thread"; items are isolated only by a user-chosen `namespace` tuple (e.g. `(user_id, "memories")`), and reads match by namespace *prefix*. The Store API (`put`/`get`/`delete`/`search`/`list_namespaces`) has **no permission or ownership parameter** — any caller that has the namespace can read/write it. https://docs.langchain.com/oss/python/langgraph/stores
- **[analysis]** Therefore LangGraph's contamination boundary is *convention, not enforcement*: safety relies on every writer scoping to the correct namespace and every reader querying within its namespace. The docs give the per-agent pattern: procedural-memory example stores instructions under namespace `("agent_instructions",)` with key `"agent_a"`. https://docs.langchain.com/oss/python/langgraph/memory
- **[fact]** CrewAI's unified `Memory` implements an actual write-permission model, unlike LangGraph/OpenAI:
  - `MemoryScope` "restricts all operations to a branch of the tree. The agent or code using it can only see and write within that subtree." https://docs.crewai.com/concepts/memory
  - `MemorySlice` with `read_only=True` allows recall from multiple scopes (e.g. own scope + shared `/company/knowledge`) but `remember()` "Raises PermissionError (read-only)"; read-write slices require an explicit scope on every write. https://docs.crewai.com/concepts/memory
  - `private=True` memories are visible on recall only when the `source` matches; `include_private=True` is an admin escape hatch. https://docs.crewai.com/concepts/memory
- **[fact]** CrewAI serializes concurrent writers at the backend: "LanceDB operations are serialized with a shared lock and retried automatically on conflict. This handles multiple `Memory` instances pointing at the same database (e.g. agent memory + crew memory)." https://docs.crewai.com/concepts/memory
- **[fact]** OpenAI Agents SDK sessions are conversation-history stores keyed by `session_id`; "Different sessions maintain separate conversation histories" and sharing is explicit ("Different agents can share the same session" by passing the same object). `RedisSession` exists for "shared memory across workers/services". No per-agent ACL exists. https://openai.github.io/openai-agents-python/sessions/
- **[fact]** Mem0 scopes memory by `user_id` / `agent_id` / `app_id` / `run_id` on both writes (`add`) and reads (`search`/`get_all` filters), explicitly "to prevent data from mixing between them." **Subtlety documented by Mem0:** unmentioned entities are *not* constrained — searching `{"user_id":"alice"}` does not require `agent_id` to be null, so cross-tagged records still surface. https://docs.mem0.ai/platform/features/entity-scoped-memory
- **[fact]** Mem0's default extraction attributes each fact to the *speaker*: "facts from `user` messages are stored with `user_id` set and `agent_id` null, facts from `assistant` messages with `agent_id` set and `user_id` null." Consequently an AND-filter on user+agent returns nothing for normally-created records — a documented misattribution trap. https://docs.mem0.ai/platform/features/entity-scoped-memory
- **[fact]** Mem0 writes are additive-only by default: "New memories are added without overwriting or deleting existing memories"; update/delete are separate operations. https://docs.mem0.ai/core-concepts/memory-operations/add
- **[fact]** AutoGen (AgentChat): a `Memory` (e.g. `ListMemory`, `ChromaDBVectorMemory`, `RedisMemory`) is constructed and passed into a specific agent's constructor; the only way two agents share memory is passing the same instance. No framework-level permission scoping — filtering is by `metadata` you supply. https://microsoft.github.io/autogen/stable/user-guide/agentchat-user-guide/memory.html

### 2. Per-agent views

- **[fact]** LangGraph: subgraphs "manage their own checkpoint namespace", so parent does not see subgraph state changes; the docs' prescribed escape hatch for cross-boundary sharing is "shared state via Store". This is an isolation boundary plus an explicit, separate shared channel. https://docs.langchain.com/oss/python/langgraph/persistence
- **[fact]** CrewAI per-agent view = a scope path, e.g. `memory.scope("/agent/researcher")` gives private findings, while a writer agent reads shared crew memory; slices compose several scopes into one view. https://docs.crewai.com/concepts/memory
- **[fact]** Mem0 per-agent view = `agent_id` filter ("Different agents (like a planner and a critic) need separate context for the same user"); even the extraction step only pulls prior context that shares the same identifiers (`user_id`, `run_id`). https://docs.mem0.ai/platform/features/entity-scoped-memory
- **[fact]** OpenAI: the docs' session-ID naming guidance is the per-view mechanism ("User-based: `user_12345`... Thread-based... Context-based: `support_ticket_456`"). https://openai.github.io/openai-agents-python/sessions/

### 3. Consolidation ownership (who may merge/update/supersede)

Honesty note: **CoALA never uses the term "consolidation"** — verified in full text. The question's "consolidation" maps to three distinct documented mechanisms:

- **[fact]** Agent-owned reflection: CoALA §4.5 describes LLM reflection over an agent's own episodic experience written back to *semantic* memory (Reflexion: "there is no dishwasher in kitchen"; Generative Agents: reflections like "I like to ski now."). Here consolidation is a per-agent, deliberate learning action over *its own* memory. https://arxiv.org/html/2309.02427v3
- **[fact]** Save-time dedup owned by the storage pipeline: CrewAI's encoding pipeline, on save, compares new content to existing records; above `consolidation_threshold` (default 0.85) an LLM chooses keep / update / delete / insert_new; near-duplicates within one batch are dropped by pure vector math. Agents never decide this themselves. https://docs.crewai.com/concepts/memory
- **[fact]** Background system-owned consolidation: Mem0's "Dream" runs in the background — "synthesizing recurring patterns, superseding outdated facts, and merging duplicates"; synthesis is opt-in, "Supersede and Merge are always on." (Descriptions from Mem0's first-party docs index.) https://docs.mem0.ai/llms.txt
- **[fact]** CoALA notes the undeveloped end of the spectrum: "modifying and deleting (a case of 'unlearning') are understudied in recent language agents" (§4.5). https://arxiv.org/html/2309.02427v3

### 4. Patterns that prevent cross-agent contamination

- **[pattern, fact-sourced]** **No shared store; coordination via compressed summaries.** Anthropic's Research system: orchestrator-worker; each subagent has "distinct tools, prompts, and exploration trajectories — separation of concerns"; the lead agent compiles condensed results. Subagents write large outputs to a **filesystem and pass only references** to the coordinator ("minimize the 'game of telephone'"). The shared context is the coordinator's summary, not the raw memories. https://www.anthropic.com/engineering/multi-agent-research-system
- **[pattern, fact-sourced]** Same point in Anthropic's context-engineering guidance: subagents "return only a condensed, distilled summary of its work (often 1,000-2,000 tokens)"; "the detailed search context remains isolated within sub-agents." https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- **[pattern, fact-sourced]** **Scope-partitioned shared store.** LangGraph namespaces, CrewAI scopes/slices, Mem0 entity tags, OpenAI session IDs (see §1–2). https://docs.langchain.com/oss/python/langgraph/stores, https://docs.crewai.com/concepts/memory, https://docs.mem0.ai/platform/features/entity-scoped-memory
- **[pattern, fact-sourced]** **Read-scope ≠ write-scope.** CrewAI read-only slices and private/source-matched visibility are the only sources where read and write privilege are separated at the API level. https://docs.crewai.com/concepts/memory
- **[pattern, fact-sourced]** **Append-only writes + explicit update/delete.** Mem0's additive storage prevents silent overwrite of another agent's memory. https://docs.mem0.ai/core-concepts/memory-operations/add
- **[pattern, fact-sourced]** **System-owned consolidation** (CrewAI save-time dedup; Mem0 background Dream) keeps merge/supersede decisions out of per-agent hot paths — the merged view is produced by the platform, not mutated arbitrarily by agents. https://docs.crewai.com/concepts/memory, https://docs.mem0.ai/llms.txt
- **[opinion]** None of these docs provide an empirical comparison of contamination rates across patterns; "safety" is architectural, and per the sources it is strongest where read and write scopes are enforced objects (CrewAI scopes/slices) and weakest where isolation is purely a calling convention (LangGraph namespaces, OpenAI session IDs, Mem0 filters — the latter even documents a leakage-by-default in its "unmentioned entities are not constrained" rule).

## Sources

Primary sources fetched directly during this research:

1. **CrewAI — Memory** (docs): https://docs.crewai.com/concepts/memory — scopes, `MemoryScope`, `MemorySlice` read-only/read-write, `private` + `source`, consolidation threshold, LanceDB lock.
2. **LangGraph — Memory overview** (docs): https://docs.langchain.com/oss/python/langgraph/memory — short/long-term memory, namespaces, procedural-memory per-agent example.
3. **LangGraph — Persistence** (docs): https://docs.langchain.com/oss/python/langgraph/persistence — checkpointer vs store, subgraph checkpoint isolation.
4. **LangGraph — Stores** (docs): https://docs.langchain.com/oss/python/langgraph/stores — namespace semantics, prefix matching, BaseStore API, no permission layer.
5. **OpenAI Agents SDK — Sessions** (docs): https://openai.github.io/openai-agents-python/sessions/ — session_id isolation, multiple/shared sessions, RedisSession.
6. **AutoGen (AgentChat) — Memory and RAG** (docs): https://microsoft.github.io/autogen/stable/user-guide/agentchat-user-guide/memory.html — Memory protocol, ListMemory/ChromaDBVectorMemory/RedisMemory, Mem0Memory.
7. **Mem0 — Entity-Scoped Memory** (docs): https://docs.mem0.ai/platform/features/entity-scoped-memory — user/agent/app/run scoping, attribution splitting, unmentioned-entities caveat.
8. **Mem0 — Add Memory** (docs): https://docs.mem0.ai/core-concepts/memory-operations/add — additive-only storage, identifier-scoped extraction context.
9. **Mem0 — Documentation index** (first-party llms.txt): https://docs.mem0.ai/llms.txt — Dream feature description (synthesis/supersede/merge).
10. **CoALA — Cognitive Architectures for Language Agents** (paper, arXiv:2309.02427 v3): https://arxiv.org/html/2309.02427v3 — memory taxonomy, learning/reflection, absence of multi-agent memory/privacy treatment. (Also https://arxiv.org/abs/2309.02427 for the abstract.) Read via the arXiv HTML rendering; the LaTeX PDF was not parsed.
11. **Anthropic — How we built our multi-agent research system** (engineering post): https://www.anthropic.com/engineering/multi-agent-research-system — orchestrator-worker, subagent compression, filesystem artifacts.
12. **Anthropic — Effective context engineering for AI agents** (engineering post): https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents — compaction, note-taking, sub-agent context isolation.

## Verdict

**Established** (from the sources): The field's shared long-term memory is built from four mechanisms — (1) a shared store partitioned by namespaces/scopes/entity-tags/session-IDs, (2) per-agent views implemented as scope objects or filter scopes, (3) write permissions that are enforced only in CrewAI (read-only slices, private/source-matched visibility) and otherwise are calling conventions, and (4) consolidation owned by the storage pipeline (CrewAI save-time dedup, Mem0 background Dream) or by an agent reflecting on its own episodic memory (CoALA/Reflexion) — never as a free cross-agent edit. Anthropic's production system avoids a shared store entirely and passes compressed summaries/references instead. CoALA, the one theoretical source, explicitly does not cover multi-agent memory sharing, privacy, or isolation.

**Uncertain**: There is no empirical benchmark in any source comparing contamination rates across these patterns; "safe" here is architectural convention, and at least one documented default (Mem0's unconstrained unmentioned entities) leaks across scopes if the caller is careless. Whether namespace/scoping conventions or enforced permission objects meaningfully reduce real contamination is unmeasured.

**What would settle it**: a contamination eval harness — e.g., seed agent A's private facts, run retrieval for agent B across each pattern, and measure false retrievals/overwrites (plus a concurrent-write stress test). None of the primary sources publish one. (No primary source here was a PDF; the only paper was read via arXiv's HTML rendering and is fully cited above.)
