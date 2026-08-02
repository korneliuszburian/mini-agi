# mini-agi

Single-binary agent kernel: **enforcement-bound memory, evaluation,
skills registry, orchestration** — exposed as CLI + MCP server so ANY agent
(Codex, Claude, Cursor, opencode) plugs into the same verified brain.

- Kernel: Rust, edition 2024, pinned toolchain (1.97.1), zero-unsafe, std-only
  kernel crate (`mini-agi-core`).
- Behavioral contract: the Python PoC (`mini-agi` v1-spec-reference) is frozen;
  its tests are ported 1:1 as integration tests.
- Plan of record: [`docs/PLAN.md`](docs/PLAN.md).

## Status

Phase 0 (foundations) in progress: workspace, core kernel (hash + store),
pinned toolchain, strict lints, CI.

## Building

```sh
rustup show                # verify pinned 1.97.1
cargo test                 # unit + integration tests
cargo clippy --all-targets -- -D warnings   # lint gate
cargo fmt --check          # format gate
```

## Layout

```
crates/mini-agi-core/   kernel: hash, store, memory, eval, skills, journal
crates/mini-agi/        binary: CLI + MCP server + adapters
skills/                 project skills, each with a verify test
memory/                 the kernel's own memory (dogfooding)
docs/PLAN.md            plan of record
```

## License

MIT
