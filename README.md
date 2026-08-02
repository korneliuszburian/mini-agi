# mini-agi

Single-binary agent kernel: **enforcement-bound memory, evaluation,
skills registry, orchestration** — exposed as CLI + MCP server so ANY agent
(Codex, Claude, Cursor, opencode) plugs into the same verified brain.

- Kernel: Rust, edition 2024, pinned toolchain (1.97.1), zero-unsafe, std-only
  kernel crate (`mini-agi-core`). No async, no heavy deps.
- Behavioral contract: the Python PoC (`mini-agi` v1-spec-reference) is frozen;
  its tests are ported 1:1 as integration tests (memory hashes, eval scores,
  checkpoint-gate + audit semantics, schema validation).
- Charter (founding goal, verbatim): [`docs/CHALLENGE.md`](docs/CHALLENGE.md).
- Plan of record: [`docs/PLAN.md`](docs/PLAN.md). ADRs: `docs/adr/`.

## Status

All phases 0-5 complete, 110 tests green, gate ALL GREEN, v0.3.0:

| Phase | Deliverable |
| --- | --- |
| 0 | memory engine (consolidate/signoff/derive/provenance) + CLI |
| 1 | eval engine (4D scoring, golden, baseline, regression gate) |
| 2 | skills registry (frontmatter, verify hooks, `skill add`) + 15 skills with completion criteria |
| 3 | checkpoint journal audit (T008 semantics), typed contracts, metrics |
| 4 | stdio MCP server (21 tools, codex+claude framings) + adapters; PROVEN live: codex wrote facts through MCP |
| 5 | `init` scaffold, CI on pinned toolchain, demo, dogfood, v0.2.0 |

## The deterministic gate

```sh
scripts/verify.sh    # fmt, clippy -D warnings, tests, eval gate,
                     # checkpoint audit, provenance, stats, budget
```

A silent target is a failing target. `checkpoint.sh begin/verify` wraps
every edit step; the journal is audited by the gate (every VERIFY needs an
earlier BEGIN).

## CLI

```
mini-agi mem consolidate <buffer> [--domain d] [--require-signoff] [--dry-run]
mini-agi mem signoff <queue> <index>
mini-agi derive [--brief-only]
mini-agi provenance
mini-agi eval score <run.json>
mini-agi eval gate [--tolerance 0.05] [--write-baseline]
mini-agi skill list | show <name> | verify <name> | add <source>
mini-agi checkpoint audit
mini-agi validate <eval-run|ticket|spec|verdict> <document.json>
mini-agi stats | budget
mini-agi mcp                  # stdio MCP server (14 tools)
mini-agi init                  # scaffold a repo with a verified brain
mini-agi ticket list|show|validate <id>
mini-agi run ingest <run.json> [--retro <md>]
mini-agi insights | backlog | resume
mini-agi mcp                  # stdio MCP server (21 tools)
```

## MCP

Any MCP client connects over stdio:

```json
{ "mcpServers": { "mini-agi": { "command": "mini-agi", "args": ["mcp"] } } }
```

opencode: see `opencode.json` in this repo (dogfoods the kernel as an MCP
server). Codex: `.codex/agents/*.toml` subagents + `.codex/config.toml`.

## Dogfooding

The repo is its own first customer: canonical memory in `memory/canonical`
(provenance-gated), derived views in `memory/derived`, checkpoint journal in
`memory/episodic/checkpoints.log`, 15 verifiable skills in `.agents/skills/`,
11 eval cases in `evals/`. The gate measures the repo itself (AGENTS chain
vs 32KiB cap, skills list vs 2% budget, memory leverage ratio).

## Building

```sh
rustup show                # verify pinned 1.97.1
cargo build --release
cargo install --path crates/mini-agi   # install as `mini-agi` (workspace root is a virtual manifest)
```

## Layout

```
crates/mini-agi-core/   kernel: hash, store, memory, eval, skills, journal,
                        contract, metrics
crates/mini-agi/        binary: CLI (main.rs) + MCP server (mcp.rs)
.agents/skills/         15 verifiable skills (ADR-0002)
.agents/checks/         review rubric + anchor self-test
scripts/                verify.sh, checkpoint.sh, hitl-loop.template.sh
memory/                 the kernel's own memory (dogfooding)
evals/ tickets/         eval fixtures + real tickets (PoC port)
docs/                   CHALLENGE, PLAN, ADRs
```

## License

MIT
