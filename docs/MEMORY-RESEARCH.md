# Memory & skills research (deep pass, 2026-08-05)

Synthesis of the best current patterns for AGENT MEMORY and SKILLS
from official sources: Anthropic (context engineering, building
agents, Claude Code best practices, writing tools, agent skills),
OpenAI (new tools for agents, compaction API), Codex docs (build
skills, AGENTS.md), the CoALA and MemGPT papers, and Karpathy's
append-and-review note. Each claim carries its source URL.

Sources: anthropic.com/engineering/effective-context-engineering-for-
ai-agents · /building-effective-agents · /claude-code-best-practices ·
/writing-tools-for-agents · /equipping-agents-for-the-real-world-with-
agent-skills · agentskills.io/specification · openai.com/index/new-
tools-for-building-agents · developers.openai.com/api/docs/guides/
compaction · developers.openai.com/codex/build-skills · codex/agent-
configuration/agents-md · arxiv.org/abs/2309.02427 (CoALA) ·
2310.08560 (MemGPT) · karpathy.bearblog.dev/the-append-and-review-note

NOT REACHABLE (verified): llm.wiki is parked (domain for sale);
Karpathy's "A Deep Dive into LLM Memory" (Apr 2025) is a YouTube
video, not a blog post — his RAM/ROM framing is covered from public
coverage, marked as such.

## A. Memory

### Hierarchy (how we map)
- MemGPT: context = main memory (RAM), external storage = disk, OS-
  style paging; INTERRUPTS trigger reflection/consolidation at event
  boundaries (session end, task completion), not only on demand.
- CoALA: three modules — episodic (traces), semantic (facts),
  procedural (skills). OUR layout maps directly: memory/episodic/ =
  episodic, memory/canonical/ = semantic, .agents/skills/ = procedural.
  We implement CoALA's taxonomy better than most products.
- Karpathy framing: system prompt = most-trusted always-loaded tier;
  conversation = working memory; long-term store = retrieval tier
  (most fallible). Our AGENTS.md/brief = the always-loaded tier (must
  stay small), canonical facts = the retrieval tier.

### Retrieval / curation
- Progressive disclosure (Claude skills + codex): metadata always
  loaded, full doc on activation, resources on demand. Codex caps the
  skill index at 2% of the context window (~8k chars), shortening
  descriptions first.
- Context editing > summarization (Anthropic): REMOVE/REWRITE stale
  context rather than summarize; summaries lose detail. Compaction is
  Anthropic's primary tool; directed compaction (/compact <what to
  preserve>) is the refinement.
- OpenAI compaction is a first-class API primitive with a
  compact_threshold and an opaque compaction item carrying "key prior
  state and reasoning".
- Selective retention: preserve the full list of modified files and
  test commands in compaction instructions.
- Lazy loading (Claude Code): child CLAUDE.md on demand; long
  reference material costs almost nothing until needed.

### Consolidation
- When: at context-pressure thresholds AND event boundaries.
- What to preserve: facts > narrative; decisions, code patterns,
  file states, commands.
- Karpathy append-and-review: append-only, gravity (unreferenced
  items sink, nothing is deleted), periodic review PROMOTES (copy to
  top) or merges. Validates our append-only canonical + never-edit
  journal; suggests our review pass should promote/merge, not rewrite.

### Memory QUALITY vs what we do
- Typed memory with explicit read/write tools: we have it.
- IDs + provenance: our sha256[:16] fact ids + provenance on every
  entry is AHEAD of the literature (papers assume flat text stores).
- DEDUP: MemGPT reflection says "update, don't append" — the ONE
  quality mechanic we under-specify: our append-only canonical has no
  merge/supersede operation.
- Forgetting: Karpathy's gravity is the only source-endorsed model
  (demote, never delete) — our journal/episodic buffer already do
  this.
- Retrieval quality (Anthropic writing-tools): resolve opaque ids to
  natural language, return high-signal fields, token-budget responses
  — our memory_query returns whole topics (the "address book brute-
  force" failure mode).

## B. Skills in codex vs claude vs ours

Codex: scans .agents/skills up the tree; frontmatter name+description;
index budget 2%/~8k chars; agents/openai.yaml policy
(allow_implicit_invocation) ≈ our disable-model-invocation;
[[skills.config]] to disable.
Claude: 3-level progressive disclosure; disable-model-invocation,
user-invocable, allowed-tools, paths globs, context: fork (subagent),
hooks-in-skills, dynamic context injection (backtick-command),
skill content survives compaction (5k/skill, 25k combined).

OUR GAPS:
1. No `name:` in several SKILL.md files (codex requires it).
2. No agents/openai.yaml — the kernel's HITL gates are not expressed
   as machine policy (the model decides when to run write skills).
3. No context-budget discipline on the 15-skill index (2% cap).
4. No dynamic context injection (skills say "check memory" instead of
   inlining the state).
5. No context: fork isolation for heavy skills (review/code-review).
6. Versioning not in a standard place (agentskills metadata field).
7. No skills-ref validate in verify.sh (the open spec has a validator).

## C. Top-tier patterns (source | our status)

- Verification closes the loop (cc-best-practices) | EXCEEDED.
- Deterministic hooks over advisory instructions | HAVE (extend to
  skill lifecycle).
- Fresh-context adversarial review | HAVE.
- Compaction preserves decisions/files/tests; directed compaction |
  PARTIAL (consolidation lacks an explicit preservation list).
- Threshold-triggered compaction as a primitive | NO (adopt as
  trigger semantics).
- Memory tools: resolve ids->names, token budgets, concise/detailed |
  PARTIAL (memory_query whole-topic).
- Tool consolidation + namespacing (writing-tools) | NO (flat MCP
  names, granular tools).
- Error responses that teach (writing-tools) | NO.
- Poka-yoke / disable-model-invocation for side-effecting workflows |
  PARTIAL (HITL prompts, not skill policy).
- Evals with tool-use metrics (writing-tools) | PARTIAL (harness +
  judge-drift; no tool-use metrics).
- Memory reflection: extract facts, update-don't-append (MemGPT) |
  PARTIAL (no merge/supersede).
- Typed memory (CoALA) | HAVE.
- Append + gravity + promote review (Karpathy) | PARTIAL (review
  should promote/merge).

## Top 10 highest-impact changes (source | effort)

1. Frontmatter normalization + skills-ref validate in verify.sh
   (agentskills.io, codex-skills | S)
2. Invocation policy in machine form — agents/openai.yaml with
   allow_implicit_invocation:false on write/HITL skills
   (codex-skills | S)
3. Skills index context budget — measure, cap, truncate-tail
   descriptions, split references/ (codex-skills | M)
4. Directed consolidation: explicit preservation list (files, tests,
   open decisions) + event-boundary triggers
   (cc-best-practices, oai-compaction, MemGPT | M)
5. Fact merge/supersede + dedup gate (MemGPT | M)
6. Selective token-budgeted memory retrieval with concise/detailed
   response format (writing-tools | M)
7. Composite tools + MCP namespacing (writing-tools | S)
8. Skill-scoped hooks + subagent fork for heavy skills (cc-skills | M)
9. Poka-yoke & teaching error responses + tool descriptions as
   junior-dev docstrings (building-agents, writing-tools | S)
10. Tool-use eval metrics in the harness (writing-tools | M)

S = hours, M = days. Nothing conflicts with the v1-frozen behavioral
contract; all changes sit in kernel-adjacent tooling/memory ops, not
core semantics. The roadmap: memory-quality goal (items 4-6, 10) and
skills-in-codex goal (items 1-3, 7-9) — the two next slices.
