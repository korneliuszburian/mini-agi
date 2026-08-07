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

All phases 0-11 complete (memory, eval, skills, journal, MCP, loop, verifier,
harness evolution, Rust quality gates), 424 tests green, gate ALL GREEN, v0.3.0:

| Phase | Deliverable |
| --- | --- |
| 0 | memory engine (consolidate/signoff/derive/provenance) + CLI |
| 1 | eval engine (4D scoring, golden, baseline, regression gate) |
| 2 | skills registry (frontmatter, verify hooks, `skill add`) + 17 skills with completion criteria |
| 3 | checkpoint journal audit (T008 semantics), typed contracts, metrics |
| 4 | stdio MCP server (39 tools, codex+claude framings) + adapters; PROVEN live: codex wrote facts through MCP |
| 5 | `init` scaffold, CI on pinned toolchain, demo, dogfood |
| 6 | intelligence loop closure (ADR-0005): gaps become tickets, `loop dispatch/verify` |
| 7 | senior runtime quality, failure register + MAST + reflections |
| 8 | verifiable reward layer: `run verify`, best-state regression bound |
| 9 | verified before trusted: judge-drift calibration, counterfactual harness gate |
| 10 | the kernel improves itself: harness evolution, honest codex capture, codex review |
| 11 | Rust quality gates (cycle 34): warnings-deny, clippy all-features, property tests |

Runtime thresholds (loop target, regression tolerance, worker caps) are
tunable via `.miniagi.json` or `MINIAGI_*` env vars — see
`docs/HARDENING-AUDIT.md` (P0-2).

## The deterministic gate

```sh
scripts/verify.sh    # build, fmt, clippy -D warnings, tests, eval gate,
                     # checkpoint audit, provenance, stats, budget,
                     # insights, audit (memory-load + verifier attribution)
```

A silent target is a failing target. `checkpoint.sh begin/verify` wraps
every edit step; the journal is audited by the gate (every VERIFY needs an
earlier BEGIN).

## Verified iteration — the breakthrough pattern

`mini-agi codex --iterate N` is the kernel's verified-reward loop: a
worker (e.g. codex, or a blind single-shot generation under
`--blind-worker`) runs, the kernel verifies the outcome with a
deterministic verifier the worker cannot see, and on failure it re-
invokes a fresh worker with a distilled, per-case checklist of the
failing hidden cases — bounded by budget caps, with the attempt chain
recorded in the run.json.

Measured (EXP-012 N=5, EXP-013 N=10, 4 hidden-suite task classes, same
model): plain best-of-k blind single-shots pass 25-50% (Wilson 95% CI
[0.13-0.30, 0.41-0.70]); the kernel's verified-iteration loop passes
82.5-100% (CI [0.67-0.84, 0.92-1.00]). Non-overlapping CIs; on below-
bar tasks the separation is total (P 0/30 vs K 23/30 in EXP-013). The
pattern pays exactly where solo is below the bar (the literature's
headroom prediction, Reflexion/AWM) — and it is honest: the same
controls rejected 7 false kernel-vs-plain claims before this.

```
mini-agi codex spec.md work/ --verify "make verify" --target work/ \
  --iterate 5 --blind-worker --hidden-dir /abs/hidden-suite
```

## Data-dir contract

The kernel operates on a repo root resolved as: `AGENTIC_ROOT` env var,
else the current directory. On first use in an empty dir it bootstraps
the `memory/` + `evals/` + `tickets/` + `scripts/` skeleton (no files —
run `mini-agi init` for the full scaffold). Runtime thresholds and
budgets live in `.miniagi.json` (+ `MINIAGI_*` env overrides); hard
per-run budget gates (`max_tokens`/`max_cost_usd`) block a `loop verify`
close on breach.

## CLI

```
mini-agi mem consolidate <buffer> [--domain d] [--require-signoff] [--dry-run]
mini-agi mem signoff <queue> <index>
mini-agi mem query <keyword> [--domain d] [--raw] [--budget N]
mini-agi mem supersede <body> --supersedes <id,...> [--domain d]
mini-agi mem preserve|unpreserve <id,...>
mini-agi mem verify                # integrity gate (duplicates/lineage/preserve)
mini-agi derive [--brief-only] [--snapshot <name>] [--replay <name>]
mini-agi provenance
mini-agi eval score|steps <run.json>
mini-agi eval gate [--tolerance 0.05] [--write-baseline]
mini-agi eval judge-drift | judge-recalibrate | hidden [<dir>] | mismatches [<run>]
mini-agi skill list | show <name> | verify <name> | add <source>
mini-agi checkpoint audit
mini-agi validate <eval-run|ticket|spec|verdict> <document.json>
mini-agi stats | budget
mini-agi mcp                  # stdio MCP server (39 tools)
mini-agi init                  # scaffold a repo with a verified brain
mini-agi ticket list|show|validate|validate-graph <id>
mini-agi run ingest <run.json> [--retro <md>]
mini-agi run verify <run.json> [--dry-run]   # deterministic verifier
mini-agi run verify-audit <run.json>         # vacuous-verifier audit
mini-agi run failures <run.json>             # repetition register
mini-agi loop status|dispatch|verify|run|objective|parallel
mini-agi resume | insights | backlog | health | audit | status [--json]
mini-agi dream --source <md> | --idle | --promote
mini-agi harness snapshot | verify <target> <candidate> [--claims]
mini-agi ui [--port N]       # local dev server
mini-agi codex <spec> <workdir> --verify <cmd> --target <dir> [--iterate N]
                      # captured worker run; --iterate N = verified-
                      # iteration loop: on verifier failure, re-invoke a
                      # fresh worker with the distilled failure register
                       # (EXP-012: turns blind single-shots from 50% to
                       # 100% verified pass where solo is below the bar)
```

## MCP

Any MCP client connects over stdio:

```json
{ "mcpServers": { "mini-agi": { "command": "mini-agi", "args": ["mcp"] } } }
```

opencode: see `opencode.json` in this repo (dogfoods the kernel as an MCP
server). Codex: `.codex/agents/*.toml` subagents + `.codex/config.toml`.

Write tools need HITL: every tool that changes the worker tree or canonical
memory (`loop_dispatch`, `loop_objective`, `memory_signoff`,
`memory_consolidate`, `memory_derive`, `run_ingest`, `ticket_claim`/`release`,
`skill_add`, `harness`, `loop_run`) requires a non-empty `approve` reason
argument in the kernel — the call is refused without it. `memory_consolidate`
with `dry_run: true` is the one read-only exception.

## Dogfooding

The repo is its own first customer: canonical memory in `memory/canonical`
(provenance-gated), derived views in `memory/derived`, checkpoint journal in
`memory/episodic/checkpoints.log`, 16 verifiable skills in `.agents/skills/`,
26 eval cases in `evals/`. The gate measures the repo itself (AGENTS chain
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
.agents/skills/         16 verifiable skills (ADR-0002) + caveman (reference)
.agents/checks/         review rubric + anchor self-test
scripts/                verify.sh, checkpoint.sh, hitl-loop.template.sh
memory/                 the kernel's own memory (dogfooding)
evals/ tickets/         eval fixtures + real tickets (PoC port)
docs/                   CHALLENGE, PLAN, ADRs
```

## License

MIT
