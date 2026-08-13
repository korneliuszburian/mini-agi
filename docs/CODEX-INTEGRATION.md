# codex integration (AFK-SUPERVISOR S4)

mini-agi is a first-class MCP server for codex sessions in this repo.

## Registration

`.codex/config.toml` (project-scoped, committed):

```toml
[mcp_servers.mini-agi]
command = "cargo"
args = ["run", "-q", "-p", "mini-agi", "--", "mcp"]
trusted = true
default_tools_approval_mode = "auto"
enabled_tools = ["loop_status", "loop_dispatch", "loop_objective", "loop_verify", "memory_query", "memory_consolidate", "memory_derive", "memory_signoff", "provenance", "skill_add", "skill_list", "skill_show", "checkpoint_audit", "dream"]
```

- `cargo run` resolves the kernel from source — no machine-specific paths in
  the committed config. For a lower-latency setup, install the binary once
  (`cargo install --path crates/mini-agi`) and switch `command = "mini-agi"`.

> `mini-agi init` now emits this full config automatically (`.codex/
> config.toml`): the MCP server registration (pointing at the current
> binary), the `enabled_tools` allow-list, and `approval_mode = "prompt"`
> on every write tool. `init` skips the file if one already exists, so the
> committed config above stays authoritative when present.
- The contract text (`instructions`) is NOT a config key — codex reads it
  from the MCP server's `initialize` response (MCP spec, `InitializeResult.
  instructions`), emitted by `crates/mini-agi/src/mcp.rs` (512 chars,
  self-contained: results are provenance-bound; a run stays unverified
  until `loop verify` (or the supervised verifier) passes).
- Schema is per the codex 0.146 manual: `enabled_tools` (allow-list),
  `default_tools_approval_mode`, per-tool `approval_mode` under
  `[mcp_servers.mini-agi.tools.<name>]`.
- Verify registration: `codex mcp list` shows `mini-agi ... enabled`.

## Per-tool approval — the explicit allowlist

39 tools, no wildcard (`enabled_tools`). Reads (`loop_status`, `memory_query`,
`loop_verify`, `checkpoint_audit`, `provenance`,
...) run `auto`; writes (`loop_dispatch`, `loop_objective`,
`skill_add`, `dream`, `memory_consolidate/
derive`) and `harness` run `prompt` (HITL). `memory_signoff` stays `prompt`
by design — the signoff gate is human (ADR-0010); a session cannot silently
merge its own memory.

Every one of those write tools ALSO enforces the gate inside the kernel
itself: the MCP handler requires a non-empty `approve` reason argument and
refuses the call without it (`error: <tool> requires an approval reason`).
This is defense in depth — even a codex config mistake (a write tool
accidentally set to `auto`) still cannot fire a worker-tree or canonical-
memory write without an explicit `approve` string. The exception is
`memory_consolidate` with `dry_run: true`, which is read-only and skips the
requirement. `memory_signoff` is additionally gated by the review-queue
signoff contract (ADR-0010), so it needs both a queue entry and `approve`.

## The supervisor tools (AFK v3 — launch and poll)

- `loop_run` (supervisor CLI `mini-agi loop run`, not an MCP tool — a write that changes the worker
  tree; the `approve` reason is required): launches a verified-iteration
  run in the BACKGROUND (detached child; MCP tool timeouts cannot kill
  it) and returns the run handle. The resolved case target/verifier are
  honored; one run per workdir (a second launch while one is active is
  refused).
- `run_status` (auto): poll the handle — alive (identity-aware: pid +
  start-time, zombies report dead), report ready, progress tail.
- `run_report` (auto): the run report — reads are constrained to the
  workdir (a handle whose launch.json points elsewhere is invalid).
- The run ends in a reviewable REPORT.md + run.json (verified by the
  kernel's own verifier; the sequential-reviewer template can add an
  independent review pass). `loop verify <case>` still closes gaps.

## The codex-session contract (AGENTS.md fragment)

1. Session start: `loop_status` + `memory_query` + `checkpoint_audit`.
2. Post-run: `run_verify <run.json>` — no unverified success claims.
3. Writes go through the prompt gate; `./scripts/verify.sh` ALL GREEN is the
   repo's own gate before any run counts.
