## Findings

### 1. The dominant pattern: layered per-scope memory, not one shared brain

All three coding-agent kernels (opencode, Codex CLI, Claude Code) document memory/instruction loading as a **stack of scope layers keyed to filesystem location**, resolved from a project root — never a single cross-repo brain.

**Claude Code** (`CLAUDE.md` docs, https://code.claude.com/docs/en/memory):
- Five scope tiers with explicit load order: managed policy → user (`~/.claude/CLAUDE.md`) → project (`./CLAUDE.md` or `./.claude/CLAUDE.md`) → local (`CLAUDE.local.md`), plus per-directory files loaded on demand when Claude reads files there. All discovered files are *concatenated*, not merged/overriding. *fact*
- Auto memory is **keyed to git repository identity**: "Each project gets its own memory directory at `~/.claude/projects/<project>/memory/`. The `<project>` path is derived from the git repository, so all worktrees and subdirectories within the same repo share one auto memory directory." It is machine-local, not shared across machines. Only the first 200 lines / 25KB of `MEMORY.md` load at session start. *fact*
- Subagents get their own context and, optionally, their own separate memory directory; the main session's auto memory is not loaded into subagents. *fact*

**Codex** (`AGENTS.md` doc, https://learn.chatgpt.com/codex/agent-configuration/agents-md; and the codex-1 system message in "Introducing Codex", https://openai.com/index/introducing-codex/):
- Instruction chain built once per run: global `~/.codex/AGENTS.md` (or `AGENTS.override.md`), then project files walking from project root down to cwd (one file per directory), concatenated root-down, deeper files override earlier; combined size capped at `project_doc_max_bytes` (32 KiB default). *fact*
- AGENTS.md scoping rule from the shipped system message: "The scope of an AGENTS.md file is the entire directory tree rooted at the folder that contains it. For every file you touch in the final patch, you must obey instructions in any AGENTS.md file whose scope includes that file… More-deeply-nested AGENTS.md files take precedence." *fact*
- Codex also has a *global* cross-chat memory ("Memories", https://learn.chatgpt.com/codex/customization/memories) stored at `~/.codex/memories/`, injected into future sessions — but it is **off by default**, per-chat controllable, skips chats that used external context when configured, and the docs explicitly say to keep required rules in AGENTS.md rather than memories. *fact*

**opencode** (`Rules` doc, https://opencode.ai/docs/rules; `Config` doc, https://opencode.ai/docs/config):
- Rule discovery order: local `AGENTS.md` traversing up from cwd, then global `~/.config/opencode/AGENTS.md`, then `~/.claude/CLAUDE.md` (Claude Code compat fallback). Project config resolution: "it first looks for a config file in the current directory, then traverses up to the nearest Git directory." Config files are *merged*, later layers override conflicting keys only. *fact*
- No memory subsystem is documented: `https://opencode.ai/docs/memory/` returns 404 (verified). opencode's persistent knowledge mechanism is AGENTS.md + `instructions` globs in config, not a memory store. *fact*

### 2. Identity: the kernel is per-repo; identity = filesystem location + trust gate

- **opencode**: identity is the resolved config stack — remote (`.well-known/opencode`) → global → custom (`OPENCODE_CONFIG`) → project → `.opencode/` → inline → managed (MDM, non-overridable). Project config overrides global; managed overrides all. *fact* (Config doc)
- **Codex**: project root detection via `project_root_markers` (default `.git`; can add `.hg`, `.sl`). Crucially, project identity is gated by **trust**: "Codex loads project-scoped config files only when the project is trusted. If the project is untrusted, Codex ignores project `.codex/` layers, including `.codex/config.toml`, project-local hooks, and project-local rules." Project-scoped config cannot override credential/provider/telemetry keys (identity/host-owned surface stays user-level). *fact* (Advanced Config doc, https://learn.chatgpt.com/codex/config-file/config-advanced)
- **Claude Code**: four settings scopes (managed/user/project/local) with fixed precedence; managed settings cannot be overridden by other scopes; project `.claude/settings.json` loads only from the starting directory; workspace-trust dialog gates project allow rules. MCP servers are scoped per scope: `.mcp.json` (project), `~/.claude.json` (user/local). *fact* (Settings doc)
- **MCP protocol**: identity is carried per-request in `_meta` (`io.modelcontextprotocol/clientInfo`); the protocol is stateless and "does not dictate how AI applications use LLMs or manage the provided context." Local (stdio) servers serve a single client; remote (Streamable HTTP) servers serve many. Scoping of *which* servers a project sees is entirely a client-side concern. *fact* (MCP Architecture, https://modelcontextprotocol.io/docs/concepts/architecture)

### 3. Enforcement: advisory memory vs deterministic client-side gates, layered per scope

- **Claude Code**: CLAUDE.md is explicit *context, not enforcement* — "Claude treats them as context, not enforced configuration. To block an action regardless of what Claude decides, use a PreToolUse hook." Enforcement lives in settings (permissions, sandbox, hooks), which are enforced by the client and can be pushed to the managed scope. *fact* (Memory + Settings docs)
- **Codex**: enforcement via sandbox modes, approval policies, and `allow_managed_hooks_only` (ignores user/project/session hooks, keeps managed ones) — the last is only honored in `requirements.toml`, i.e., the org-owned layer. *fact* (docs/config.md in github.com/openai/codex; Config Reference)
- **opencode**: per-agent `permission` model (`allow|ask|deny`) including `external_directory` (any read/write outside the project worktree), and managed settings that users cannot override. *fact* (Agents doc, Config doc)
- Pattern across all: **memory/instructions are advisory; enforcement is a separate deterministic layer with its own stricter scope stack**. *estimate* (synthesis of the above primary claims)

### 4. Eval baselines: per-repo, verifier-bound; no documented shared cross-repo baseline

- **Claude Code** documents verification as a per-session contract bound to the repo: give Claude a check it can run (tests, build, diff-vs-fixture), and gate stops on it — as a Stop hook (deterministic), a `/goal` condition re-checked each turn, or a verification subagent. CLAUDE.md is where the verification commands for the repo are stored. *fact* (Best practices, https://code.claude.com/docs/en/best-practices)
- **Codex (cloud)**: "Each task is processed independently in a separate, isolated environment preloaded with your codebase"; the codex-1 system message says "Only committed code will be evaluated" and "If the AGENTS.md includes programmatic checks to verify your work, you MUST run all of them … AFTER all code changes have been made." Eval is therefore scoped to one repo checkout per task instance. *fact* (Introducing Codex, appendix system message)
- **SWE-bench** (paper, arXiv:2310.06770) is the canonical *per-repo eval-instance* design these benchmarks descend from: 2,294 problems drawn from 12 repos, each = one codebase + one issue + gold test verification. *fact*
- None of opencode, Claude Code, Codex, or the MCP spec documents an eval-baseline registry shared across multiple repos from one kernel; MCP explicitly excludes evaluation from its scope. *fact* (absence in fetched primary docs; MCP scope statement)

### 5. Measured tradeoffs of one shared brain vs per-project instances

- **The context window is the stated binding constraint.** Claude Code's best-practices: "performance degrades as context fills. When the context window is getting full, Claude may start 'forgetting' earlier instructions or making more mistakes. The context window is the most important resource to manage." *fact (documented guidance)*
- Startup overhead of a per-repo session is quantified only as **illustrative** token figures (context-window doc, labeled "illustrative"): system prompt ~4,200; user CLAUDE.md ~320; project CLAUDE.md ~1,800; auto memory ~680; skills index ~450; deferred MCP tools ~120; env ~280 — ≈7.85k tokens consumed before the first prompt. Every per-project-instance pays this fixed cost; wider global memory raises it. *estimate* (from illustrative numbers)
- Documented recommendation is *more, smaller instances*: "If you've corrected Claude more than twice on the same issue in one session… run `/clear` and start fresh with a more specific prompt. A clean session with a better prompt almost always outperforms a long session with accumulated corrections." Subagents exist specifically so file reads stay in a separate context; the worked example shows a subagent reading ~6,100 tokens of files and returning a ~420-token summary. *fact (Anthropic's documented guidance)*
- The large-codebases analysis frames it as a two-sided trade: "Too much context loaded into every session degrades performance, while too little context leaves Claude to navigate blind." It also documents why a shared index ("one brain" over the codebase) is avoided: RAG-style embeddings "can fail because embedding pipelines can't keep up with active engineering teams… Retrieval then returns a function the team renamed two weeks ago," whereas each developer's instance reads the live checkout. *fact (claude.com blog, May 14 2026)*
- Codex's shared-memory feature is deliberately conservative: off by default, per-chat opt-in, redacts secrets, waits for idle, can be disabled when MCP/web-search context was used. *fact* (Memories doc)
- **No controlled, head-to-head measurement** of "one shared brain vs per-project instances" is published in these primary sources. The only numeric outcomes found are anecdotal team reports (e.g., 3× faster incident triage, ~80% reduction in research time) that do not compare memory architectures. *opinion → verified absence*

## Sources

1. opencode docs — Intro: https://opencode.ai/docs/
2. opencode docs — Config: https://opencode.ai/docs/config/
3. opencode docs — Rules: https://opencode.ai/docs/rules/
4. opencode docs — Agents: https://opencode.ai/docs/agents/
5. opencode docs — MCP servers: https://opencode.ai/docs/mcp-servers/
6. opencode docs — Memory: https://opencode.ai/docs/memory/ (404, noted)
7. Claude Code docs — Memory: https://code.claude.com/docs/en/memory
8. Claude Code docs — Settings: https://code.claude.com/docs/en/settings
9. Claude Code docs — Best practices: https://code.claude.com/docs/en/best-practices
10. Claude Code docs — Monorepos/large codebases: https://code.claude.com/docs/en/large-codebases
11. Claude Code docs — Context window: https://code.claude.com/docs/en/context-window
12. Claude Code blog — How Claude Code works in large codebases (May 14, 2026): https://claude.com/blog/how-claude-code-works-in-large-codebases-best-practices-and-where-to-start
13. Claude Code blog — How Anthropic teams use Claude Code (Jul 24, 2025): https://claude.com/blog/how-anthropic-teams-use-claude-code
14. Codex docs — Advanced Config: https://learn.chatgpt.com/codex/config-file/config-advanced
15. Codex docs — Config Reference: https://learn.chatgpt.com/codex/config-file/config-reference
16. Codex docs — AGENTS.md: https://learn.chatgpt.com/codex/agent-configuration/agents-md
17. Codex docs — Memories: https://learn.chatgpt.com/codex/customization/memories
18. Codex docs — Chronicle: https://learn.chatgpt.com/codex/customization/chronicle
19. OpenAI — Introducing Codex (May 16, 2025, incl. codex-1 system message appendix): https://openai.com/index/introducing-codex/
20. Codex repo — README and docs/config.md: https://github.com/openai/codex (raw: https://raw.githubusercontent.com/openai/codex/main/README.md, .../docs/config.md)
21. MCP — Architecture overview: https://modelcontextprotocol.io/docs/concepts/architecture
22. SWE-bench paper: https://arxiv.org/abs/2310.06770

## Verdict

**Established:** The four systems converge on one pattern — a single kernel binary whose identity, memory, and enforcement are all resolved per-repo from a scope stack (global/user → project → nested subdirectory), with git root as the identity anchor and a trust gate on project-loaded content. Memory/instructions are advisory context; enforcement is a separate deterministic layer (hooks, permissions, sandbox, managed settings) that is strictly scope-ordered. MCP is the only cross-repo mechanism, and even there the protocol is stateless per-request context exchange with no memory or eval semantics. Eval is uniformly per-repo and verifier-bound (test/gold-patch), never a documented shared cross-repo baseline.

**Uncertain:** The "one shared brain vs per-project instances" tradeoff has **no controlled measurement** in any primary source I reached. The documented rationale is qualitative (context degradation, illustrative startup token costs, RAG-index staleness) and the operational guidance uniformly favors many small instances. A concrete per-task or per-repo memory architecture (e.g., this repo's canonical-memory kernel) is not a documented pattern in any of the four systems' official docs.

**What would settle it:** (a) the OpenAI Codex benchmark paper (per-repo task evaluation) — I could not verify its arXiv ID from title search and did not fetch it, so per-task eval claims rely on the SWE-bench and Introducing-Codex sources; (b) a controlled study holding the model fixed while varying memory breadth (single shared store vs per-repo stores) and reporting pass-rate/context-cost — none exists in the sources above; (c) vendor-published telemetry (Claude Code's `skill_activated`, Codex OTel metrics) correlating loaded-memory size with failure rate would quantify the degradation the docs assert.
