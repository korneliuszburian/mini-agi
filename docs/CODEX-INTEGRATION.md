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
enabled_tools = ["loop_status", "memory_query", "run_verify", ...]
```

- `cargo run` resolves the kernel from source — no machine-specific paths in
  the committed config. For a lower-latency setup, install the binary once
  (`cargo install --path crates/mini-agi`) and switch `command = "mini-agi"`.
- The contract text (`instructions`) is NOT a config key — codex reads it
  from the MCP server's `initialize` response (MCP spec, `InitializeResult.
  instructions`), emitted by `crates/mini-agi/src/mcp.rs` (512 chars,
  self-contained: results are provenance-bound; a run stays unverified
  until `run_verify` passes).
- Schema is per the codex 0.146 manual: `enabled_tools` (allow-list),
  `default_tools_approval_mode`, per-tool `approval_mode` under
  `[mcp_servers.mini-agi.tools.<name>]`.
- Verify registration: `codex mcp list` shows `mini-agi ... enabled`.

## Per-tool approval — the explicit allowlist

36 tools, no wildcard (`enabled_tools`). Reads (`loop_status`, `memory_query`,
`run_verify`, `loop_verify`, `eval_gate`, `checkpoint_audit`, `provenance`,
...) run `auto`; writes (`loop_dispatch`, `loop_objective`,
`ticket_claim/release`, `skill_add`, `run_ingest`, `memory_consolidate/
derive`) and `harness` run `prompt` (HITL). `memory_signoff` stays `prompt`
by design — the signoff gate is human (ADR-0010); a session cannot silently
merge its own memory.

## The codex-session contract (AGENTS.md fragment)

1. Session start: `loop_status` + `memory_query` + `checkpoint_audit`.
2. Post-run: `run_verify <run.json>` — no unverified success claims.
3. Writes go through the prompt gate; `./scripts/verify.sh` ALL GREEN is the
   repo's own gate before any run counts.
