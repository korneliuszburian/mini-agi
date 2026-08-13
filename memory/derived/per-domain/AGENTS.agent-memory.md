# PROVENANCE
# canonical_sha256: b37f64ea8d024bdf
# canonical_entries: 138
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# Domain: agent-memory (derived from canonical memory)

Applies when working on this domain. Canonical memory wins on conflict.
- `37c5e79bfe877fc0` Two consolidation regimes exist in published agent-memory systems: batch/scheduled (Generative Agents, MemGPT, Letta) and continuous background (Zep/Graphiti, Mem0, A-MEM) [S1]-[S7].
- `bd7a60fc349c2dce` No system in the surveyed sources uses a literal 'nightly cron'; 'nightly' is approximated by daily-frequency batch reflection (Generative Agents) and idle-time scheduled consolidation (Letta sleep-time compute); continuous background consolidation is approximated by per-event incremental ingestion (Zep/Graphiti, Mem0, A-MEM).
- `0ba4573ee5aac6e7` Generative Agents (Stanford 2023) batch-reflection triggers when the sum of importance scores for latest events exceeds a threshold of 150; agents reflected roughly 2-3 times per day [S4].
- `fce995c5d47473d7` Generative Agents ablation (TrueSkill): no-reflection scored mu=26.88 (sigma=0.69), no-reflection-and-no-planning mu=25.64 (sigma=0.68), crowdworker baseline mu=22.95, no-memory/no-reflection/no-planning mu=21.21; full architecture vs no-memory baseline gives Cohen's d=8.16; authors concluded 'each of these components is critical to strong performance' [S4].
- `d1f15e378891e773` MemGPT runs timed events on a regular schedule, allowing it to run 'unprompted' without user intervention; on memory pressure (warning at ~70% of context) the agent self-edits working/archival memory; at flush (~100%) it evicts ~50% of the window and generates a new recursive summary from the existing recursive summary and evicted messages [S1].
- `99693f68781d1d3c` MemGPT DMR accuracy was 93.4% (GPT-4 Turbo + MemGPT) vs 35.3% (recursive summarization alone); MemGPT reports no separate consolidation cost figures [S1].
- `990864ff4101caf9` Letta sleep-time compute (2025) is the closest published analogue to 'nightly' consolidation: an idle-time batch process where a sleep-time agent owns the memory-edit tools and rewrites the primary agent's in-context memory blocks; frequency is configurable, with higher frequency using more tokens [S3].
- `203a56e1520ca788` Letta paper reports sleep-time compute can reduce test-time compute needed for the same accuracy by ~5x on Stateful GSM-Symbolic and Stateful AIME, increase accuracy by up to 13% on Stateful GSM-Symbolic and 18% on Stateful AIME, and decrease average cost per query by 2.5x on Multi-Query GSM-Symbolic [S2].
- `ae673088f770db1e` Zep/Graphiti design rationale: continuous incremental (dynamic community) updates significantly reduce latency and LLM inference costs but their community quality gradually diverges from a full label-propagation run, so 'periodic community refreshes remain necessary' [S5].
- `d93bda457e548fbf` Zep/Graphiti (2025) performs per-message incremental ingestion: each episode triggers entity extraction (with prior n=4 messages for context), entity resolution, fact extraction, deduplication, temporal extraction, and LLM-based edge invalidation on contradictions; 'Graphiti consistently prioritizes new information' [S5].
- `cf53d2dc3217b1f6` Zep vs full-context baseline on LongMemEval (~115k token conversations): accuracy +15.2% (gpt-4o-mini) and +18.5% (gpt-4o); latency 31.3s -> 3.20s and 28.9s -> 2.58s (~90% reduction); context tokens 115k -> 1.6k [S5].
- `791ea28bb09313db` Zep caveats reported in its own paper: on DMR Zep scored 94.8% vs MemGPT 93.4%, but the full-conversation baseline scored 94.4% (gpt-4-turbo) and 98.0% (gpt-4o-mini), so the DMR gap is within noise; Zep declined on single-session-assistant questions (-17.7% gpt-4o, -9.06% gpt-4o-mini) [S5].
- `9ee7b7e18c52b434` Mem0 (2025) performs incremental consolidation at write time (per message pair) plus an async background summarization module that periodically refreshes the conversation summary independently of the main pipeline; per candidate fact the LLM chooses ADD/UPDATE/DELETE/NOOP [S6].
- `2c723f6a508796e9` Mem0 cost vs full-context on LOCOMO: full-context p95 latency ~17.1s (p50 9.87s); Mem0 p50 0.708s/p95 1.440s; graph variant p50 1.091s/p95 2.590s; paper claims '91% lower p95 latency and saves more than 90% token cost' vs full-context [S6].
- `4b7b2e26d80f0475` Mem0^g reached the highest LLM-judge score among memory systems (J~68.4 overall) with '26% relative improvements in the LLM-as-a-Judge metric over OpenAI' [S6].
