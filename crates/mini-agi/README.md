# mini-agi

Single-binary agent kernel: **enforcement-bound memory, evaluation,
skills registry, orchestration** — exposed as CLI + MCP server so ANY agent
(Codex, Claude, Cursor, opencode) plugs into the same verified brain.

The kernel lives in `mini-agi-core`; this crate is the thin shell: CLI
(`main.rs`) + stdio MCP server (`mcp.rs`) + repo scaffolding (`init.rs`).

```sh
cargo install mini-agi        # or: cargo install --path crates/mini-agi
mini-agi init                 # scaffold a repo with a verified brain
mini-agi mcp                  # stdio MCP server (14 tools)
scripts/verify.sh             # the deterministic gate
```

See the workspace README for the full CLI surface, MCP wiring and
dogfooding notes.
