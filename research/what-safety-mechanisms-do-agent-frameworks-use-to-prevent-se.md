## Findings

Scope: mechanisms that stop an agent from corrupting its own **configuration**, **skills**, or **memory** during self-modification. Primary sources only (vendor docs, specs, first-party repo code); secondary write-ups excluded. `fact` = stated verbatim in the cited source; `estimate` = my inference from stated facts; `opinion` = comparative judgment of mine.

### 1. Config protection — read-only self-paths, trust-gated loading, managed layers

- **Codex protects its own config/skills dirs inside the writable sandbox.** In the default `workspace-write` sandbox, `<writable_root>/.git`, `<writable_root>/.agents`, and `<writable_root>/.codex` are "protected as read-only … Protection is recursive, so everything under those paths is read-only." This is the direct answer to "self-modification of configuration/skills": the agent's own `.codex` and `.agents` directories cannot be written even when the workspace is writable. `fact` — Codex, "Agent approvals & security", https://learn.chatgpt.com/codex/agent-approvals-security
- **Codex only loads project-scoped config after you trust the project**, and project config "can't override machine-local provider, auth, host-owned app request metadata, notification, configuration profile selection, or telemetry routing keys" (e.g. `model_provider`, `profiles`, `otel` are ignored in project-local `.codex/config.toml`). `fact` — Codex "Config Reference", https://learn.chatgpt.com/codex/config-file/config-reference
- **Codex admins can force managed-only hooks**: `allow_managed_hooks_only = true` in `requirements.toml` "ignores user, project, and session hook configs while still allowing managed hooks." Only honored in `requirements.toml`. `fact` — Codex repo `docs/config.md`, https://github.com/openai/codex/blob/main/docs/config.md
- **Claude Code splits "hard enforcement" from "behavioral guidance".** Settings rules (`permissions.deny`, `sandbox.enabled`, `forceLoginMethod`) "are enforced by the client regardless of what Claude decides to do"; managed CLAUDE.md "cannot be excluded by individual settings"; CLAUDE.md content is "context, not enforced configuration." `fact` — Claude Code "How Claude remembers your project", https://code.claude.com/docs/en/memory; Claude Code "Security", https://code.claude.com/docs/en/security
- **Claude Code warns that memory/instructions are not a security boundary**: "To block an action regardless of what Claude decides, use a PreToolUse hook instead." `fact` — https://code.claude.com/docs/en/memory

### 2. Gate checks / deterministic enforcement at the tool-call boundary

- **Claude Agent SDK evaluates every tool request in a fixed order: hooks → deny rules → ask rules → permission mode → allow rules → `canUseTool` callback.** Deny rules "blocked, even in `bypassPermissions` mode"; bare-name deny rules remove the tool from the model's context entirely. Hooks run first and a hook deny applies even in `bypassPermissions`. `fact` — Claude Agent SDK "Configure permissions", https://code.claude.com/docs/en/agent-sdk/permissions
- **Permission modes are the coarse gate**: `default`, `dontAsk` (deny instead of prompt), `acceptEdits` (auto-approve file ops inside working dir only; out-of-scope and protected paths still prompt), `bypassPermissions`, `plan` (writes never auto-approved), `auto` (model-classified approvals). `fact` — https://code.claude.com/docs/en/agent-sdk/permissions
- **Fail-closed matching**: Claude Code "unmatched commands default to requiring manual approval," and "suspicious bash commands require manual approval even if previously allowlisted." `fact` — https://code.claude.com/docs/en/security
- **Codex is two layers: sandbox (what is technically possible) + approval policy (when to ask).** OS-enforced sandbox by default (macOS Seatbelt, Linux bwrap+seccomp, Windows native sandbox). `fact` — https://learn.chatgpt.com/codex/agent-approvals-security
- **Mini AGI runs deterministic gates before any LLM judge** ("Deterministic gates run FIRST; LLM judge is calibrated on top"), and a run's outcome is its own claim until `run verify` confirms it ("verified before trusted", ADR-0011). `fact` — docs/adr/ADR-0003-hitl-reviewer-memory-anchored.md; AGENTS.md
- **OpenCode gates tools per call with `allow`/`ask`/`deny`**, plus two structural guards: `external_directory` (default `ask` for paths outside the workspace) and `doom_loop` (prompts when "the same tool call repeats 3 times with identical input"). `fact` — OpenCode "Permissions", https://opencode.ai/docs/permissions

### 3. Human signoff / approvals

- **Interactive approval is a first-class SDK surface, not an add-on.** Claude Agent SDK's `canUseTool` callback decides anything not resolved by rules/modes; `AskUserQuestion` and MCP tools marked `anthropic/requiresUserInteraction` "always fall through to the callback, even when an allow rule matches." `fact` — https://code.claude.com/docs/en/agent-sdk/permissions
- **OpenAI Agents SDK pauses the run for human review**: tools can declare `needsApproval: true`; the run returns `interruptions` plus a resumable `state`, and "your application approves or rejects the pending items" before resuming the same run. `fact` — OpenAI Agents SDK "Guardrails and human review", https://learn.chatgpt.com/api/docs/guides/agents/guardrails-approvals
- **Codex can substitute an automatic reviewer agent for the human.** `approvals_reviewer = "auto_review"` routes eligible approvals through a reviewer subagent whose policy (in `codex-rs/core/src/guardian/policy.md`) "checks for data exfiltration, credential probing, persistent security weakening, and destructive actions"; "low-risk and medium-risk actions can proceed," "critical-risk actions" are denied, and "prompt-build, review-session, and parse failures fail closed." `fact` — https://learn.chatgpt.com/codex/agent-approvals-security; policy file https://github.com/openai/codex/blob/main/codex-rs/core/src/guardian/policy.md
- **Codex approval is granular per prompt category**: `approval_policy = { granular = { sandbox_approval, rules, mcp_elicitations, request_permissions, skill_approval } }` — note `skill_approval` is its own gate. `fact` — https://learn.chatgpt.com/codex/config-file/config-reference
- **Workspace-trust dialogs gate loaded skills/config.** Claude Code: skills in a project's `.claude/skills/` take effect for `allowed-tools` "after you accept the workspace trust dialog," and "review project skills before trusting a repository, since a skill can grant itself broad tool access." `fact` — Claude Code "Extend Claude with skills", https://code.claude.com/docs/en/skills. Codex similarly loads project config "only when you trust the project." `fact` — https://learn.chatgpt.com/codex/config-file/config-reference
- **Mini AGI makes memory writes human-signed**: contested facts and dream-loop results route through `memory signoff` / a "human queue" (ADR-0010 signoff); the reviewer gate is "human by design." `fact` — AGENTS.md; ADR-0014 lists memory poisoning as "MITIGATED … `memory signoff` for contested facts."
- **Skills can restrict their own invocation and tools.** Agent Skills standard + Claude Code extensions: `disable-model-invocation: true` (only the user can trigger — recommended for `/deploy`-style skills), `allowed-tools` / `disallowed-tools` frontmatter (grants clear after the invoking turn). `fact` — https://code.claude.com/docs/en/skills; https://agentskills.io

### 4. Memory self-modification protections

- **Vendors separate human-written memory from agent-written memory.** Claude Code: CLAUDE.md is written by the user; *auto memory* is "notes Claude writes itself." The two are "complementary memory systems"; both "loaded at the start of every conversation" and treated as context, not enforcement. `fact` — https://code.claude.com/docs/en/memory
- **Bounds and metadata on self-written memory.** Claude Code auto memory: per-repo directory `~/.claude/projects/<project>/memory/`, `MEMORY.md` index capped at "the first 200 lines … or the first 25KB"; writes that push it over limit return an error to rewrite the index; a `modified` ISO-8601 timestamp is stamped on frontmatter so "the timestamp shows how current the fact is." `fact` — https://code.claude.com/docs/en/memory
- **Codex memories are opt-in generated state.** Local memories "are off by default"; stored under `~/.codex/memories/`; "Treat these files as generated state… don't rely on editing them by hand"; "Codex redacts secrets from generated memory fields"; docs recommend keeping required rules in `AGENTS.md`, treating "memories as a helpful recall layer, not as the only source for rules that must always apply." `fact` — Codex "Memories", https://learn.chatgpt.com/codex/customization/memories
- **Audit hooks can watch/block config+memory changes mid-session.** Claude Code ships `ConfigChange` hooks ("Audit or block settings changes during sessions"). `fact` — https://code.claude.com/docs/en/security
- **Mini AGI's memory is append-only with provenance on every entry.** Canonical memory entries are dated, carry a `source`, and every fact id is `sha256[:16]` (e.g. `F-000 38e05948dad83b29`, source `2026-08-05-buffer.md`, domain, kind). Derived views are generated, never hand-edited; "on conflict canonical wins." `fact` — AGENTS.md; memory/canonical/entries/2026-08-06/2026-08-06-001.md
- **Rollback journal as a recovery gate.** Mini AGI records `BEGIN`/`VERIFY-PASS`/`VERIFY-FAIL`/`CHECKPOINT-ABORT` in `memory/episodic/checkpoints.log`; a red gate hard-resets to the last BEGIN commit (rollback always lands on the last checkpoint, ADR-0004). The journal must be repaired via `checkpoint.sh`, never through git. `fact` — docs/adr/ADR-0004-checkpoint-rollback.md; memory/episodic/checkpoints.log; AGENTS.md

### 5. Guardrails and provenance

- **Guardrails = model-as-judge pre/post checks.** OpenAI Agents SDK: input guardrails run before the main model, output guardrails validate final output, tool guardrails check arguments/results; a "tripwire" blocks the run (e.g. `InputGuardrailTripwireTriggered`). Boundary caveat: "Input guardrails run only for the first agent in the chain," output guardrails only at the final-output agent, so checks belong next to the side-effecting tool in manager-style workflows. `fact` — https://learn.chatgpt.com/api/docs/guides/agents/guardrails-approvals
- **Guards against guardrail gap in chains.** The Claude Agent SDK warns "auto-approved tools never reach `canUseTool`," so for checks that must run on every call the docs prescribe a `PreToolUse` hook. `fact` — https://code.claude.com/docs/en/agent-sdk/permissions
- **Provenance as a safety requirement.** Mini AGI reviewers must "cite canonical fact ids it relies on; a verdict without memory anchors is flagged and gated" (ADR-0003); the OWASP mapping attributes memory poisoning mitigation to "Append-only canonical with content-hash fact ids" and a "provenance fingerprint in the audit" (ADR-0014). `fact` — docs/adr/ADR-0003; docs/adr/ADR-0014
- **Provenance in telemetry.** Codex OTel records `codex.tool_decision` with "approved/denied, source: configuration vs. user," enabling post-hoc audit of who authorized each action. `fact` — https://learn.chatgpt.com/codex/agent-approvals-security
- **Protocol level defers to the host.** MCP spec: "MCP itself cannot enforce these security principles at the protocol level"; hosts "must obtain explicit user consent before invoking any tool," tool annotations "should be considered untrusted, unless obtained from a trusted server," and implementors "SHOULD build robust consent and authorization flows." `fact` — MCP Specification 2025-11-25, "Security and Trust & Safety", https://modelcontextprotocol.io/specification/2025-11-25
- **Sandbox as provenance evidence.** Mini AGI's CI gate fails unless the runner identifies itself as isolated (non-root + `RUNNER_NAME`), so "gate requires sandbox evidence" is literal (ADR-0009); the local worker runs under Landlock write-containment confined to workdir + its own state dir (ADR-0012). `fact` — docs/adr/ADR-0009-sandbox-first.md; docs/adr/ADR-0012-worker-sandbox-landlock.md

### 6. Known limits (stated by vendors)

- **Skills are not a sandbox.** Claude Agent SDK: "The `skills` option is a context filter, not a sandbox. Unlisted Skills are hidden from the model and rejected by the Skill tool, but their files remain on disk and are reachable through Read and Bash." `fact` — https://code.claude.com/docs/en/agent-sdk/skills
- **Bypass modes explicitly drop protections.** Claude Agent SDK: in `bypassPermissions`, "Claude has full system access… Use with extreme caution"; `allowed_tools` does not constrain it (deny rules and hooks still bind). `fact` — https://code.claude.com/docs/en/agent-sdk/permissions
- **Custom system prompts drop built-in safety.** Replacing the `claude_code` preset with a custom prompt loses the preset's "security and safety instructions"; "Built-in safety … must be added." `fact` — https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts
- **Codex yolo mode is explicit** (`--dangerously-bypass-approvals-and-sandbox`): "No sandbox; no approvals (not recommended)." `fact` — https://learn.chatgpt.com/codex/agent-approvals-security

## Sources

1. Claude Agent SDK — *Configure permissions*. https://code.claude.com/docs/en/agent-sdk/permissions
2. Claude Code — *How Claude remembers your project* (CLAUDE.md + auto memory). https://code.claude.com/docs/en/memory
3. Claude Code — *Security*. https://code.claude.com/docs/en/security
4. Claude Code — *Extend Claude with skills*. https://code.claude.com/docs/en/skills
5. Claude Agent SDK — *Agent Skills in the SDK*. https://code.claude.com/docs/en/agent-sdk/skills
6. Claude Agent SDK — *Modifying system prompts*. https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts
7. OpenAI Codex — *Agent approvals & security*. https://learn.chatgpt.com/codex/agent-approvals-security
8. OpenAI Codex — *Configuration Reference* (config.toml / requirements.toml). https://learn.chatgpt.com/codex/config-file/config-reference
9. OpenAI Codex — *Memories*. https://learn.chatgpt.com/codex/customization/memories
10. OpenAI Codex repo — *docs/config.md* (`allow_managed_hooks_only`). https://github.com/openai/codex/blob/main/docs/config.md
11. OpenAI Codex repo — *default auto-review policy*. https://github.com/openai/codex/blob/main/codex-rs/core/src/guardian/policy.md
12. OpenAI Agents SDK — *Guardrails and human review*. https://learn.chatgpt.com/api/docs/guides/agents/guardrails-approvals
13. Model Context Protocol — *Specification 2025-11-25, Security and Trust & Safety*. https://modelcontextprotocol.io/specification/2025-11-25
14. OpenCode — *Permissions*. https://opencode.ai/docs/permissions
15. Agent Skills — open standard overview. https://agentskills.io
16. mini-agi repo (local): `AGENTS.md`; `docs/adr/ADR-0002-skills-discovery-location.md`; `docs/adr/ADR-0003-hitl-reviewer-memory-anchored.md`; `docs/adr/ADR-0004-checkpoint-rollback.md`; `docs/adr/ADR-0009-sandbox-first.md`; `docs/adr/ADR-0012-worker-sandbox-landlock.md`; `docs/adr/ADR-0014-owasp-agentic-mapping.md`; `memory/canonical/entries/2026-08-06/2026-08-06-001.md`; `memory/episodic/checkpoints.log`

## Verdict

**Established** (primary-source verified):
- The dominant pattern is defense-in-depth at the *tool-call boundary*, not at the prompt: a fixed evaluation order (hooks → deny → ask → mode → allow → callback) with deny rules that bind even in bypass modes (Claude), and a two-layer sandbox + approval-policy model (Codex).
- Self-modification of config/skills is stopped structurally: Codex makes the agent's own `.codex` and `.agents` dirs recursively read-only inside the writable workspace; project config is trust-gated and cannot override provider/auth/telemetry keys; managed (org) config layers cannot be overridden by users or the agent.
- Self-modification of memory is bounded rather than blocked: memory stores are separate (user-written vs agent-written), opt-in, size-capped (Claude 200 lines/25KB), secret-redacting (Codex), and explicitly "context, not enforcement" — deterministic enforcement is delegated to hooks/settings.
- Human signoff exists at several granularities: per-call approval callbacks, `needsApproval` tool flags with resumable interrupted runs (OpenAI), workspace-trust dialogs for loading skills/config, and org-managed "ask" tool categories. Codex adds an automatic reviewer-agent alternative with a fail-closed policy.
- Provenance is used as a gate only in the memory-anchored reviewer systems (mini-agi's mandatory fact-id citations; ADR-0003) and as audit evidence elsewhere (Codex `tool_decision` source field, Claude `modified` timestamps, mini-agi content-hash fact ids). MCP explicitly refuses to enforce these at protocol level and defers to host consent flows.

**Uncertain:**
- Whether any framework enforces *versioned rollback* of its own config/skills automatically. Only mini-agi showed a hard rollback journal (checkpoint BEGIN/VERIFY + `reset --hard` to last green); vendor docs describe trust dialogs, permissions, and git-based "roll back in small increments" (Codex) but no enforced auto-rollback of config/skills. I found no vendor doc claiming an automatic rollback gate for agent-written memory/config — absence of evidence, not proof of absence.
- Effectiveness data (bypass rates, tripwire precision) is not published in the docs I reached; the "judge alone ~45% vs judge+tools ~94%" figure appears only as a citation inside mini-agi's ADR-0003 and was not re-verified from its source paper.

**What would settle it:**
- Vendor-published adversarial/red-team evaluations of the permission layers (Claude, Codex) under prompt-injection of the "edit your own skill/memory" kind.
- A source showing whether any framework automatically reverts agent-written memory/skill changes that break their own loaders (mini-agi is the only one with an enforceable rollback; the question of "who audits the memory writer" is answered only by Claude's size-cap/error-repair and Codex's opt-in + redaction).

Note: the Codex "Codex Security" page (https://learn.chatgpt.com/codex/security) describes the vulnerability-scanning product and was excluded as out of scope; the codex sandbox internals (Landlock/Seatbelt policies) were not fetched as PDFs and are only described via the public docs cited.
