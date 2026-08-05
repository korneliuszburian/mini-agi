# docs — the map

Single entry point for the mini-agi documentation. Everything the repo
knows is reachable from here; anything not listed is either generated
(memory/derived) or historical (see notes).

## How the repo is organized

| Path | Kind | Notes |
| --- | --- | --- |
| `docs/` | documentation | this map is the entry point |
| `docs/adr/` | decisions | ADR-0001..0014, load-bearing (ADR-0003 memory anchors, ADR-0010 signoff, ADR-0011 verifiable reward, ADR-0012 sandbox) |
| `memory/` | kernel memory | `canonical/` hand-written source of truth (append-only, provenance), `derived/` generated (never edit), `episodic/` journal+logs |
| `evals/` | eval corpus | `cases/<case>/run.json` (load-bearing: baseline, gate, loop), `golden/`, `results/baseline.json`, `references/`, `hidden/` |
| `scripts/` | the gate | `verify.sh` + `gate-lib.sh` (shared step helpers), `checkpoint.sh`, demo scripts |
| `tickets/` | issue tracker | TICKET-*.md, load-bearing (ticket_* MCP tools read them) |
| `artifacts/` | run evidence | `artifacts/<case-name>/` — run reports (REPORT.md stays TRACKED evidence; transcripts codex.log/progress.md/run.json are gitignored runtime artifacts); `artifacts/<ticket>/` holds the orchestrate skill's spec.md drafts |
| `.batch/`, `.supervisor/` | runtime batch/supervisor state | gitignored (parallel batch worktrees at `.batch/<id>`, per-run handles at `<workdir>/.supervisor/`) |
| `.krn/` | local state | gitignored (review dispositions, local scratch) |
| `.codex/` | codex integration | `config.toml` MCP registration + approval allowlist |
| `crates/` | the kernel | `mini-agi-core` (std-only kernel), `mini-agi` (binary: CLI + MCP + sandbox + supervisor) |

## Docs index

| Doc | Covers | Read when |
| --- | --- | --- |
| `docs/CHALLENGE.md` | the founding charter (verbatim, never paraphrase) | first |
| `docs/PLAN.md` | master plan / phases | planning |
| `docs/ADR-*` (docs/adr/) | architecture decisions | changing anything load-bearing |
| `docs/AFK-SUPERVISOR.md` | the supervised verified-iteration loop: `loop run`, session resume, templates, two-phase liveness, self-hosting proofs | working on the supervisor |
| `docs/CODEX-INTEGRATION.md` | codex as a client: MCP registration, approvals, the supervisor tools (loop_run/run_status/run_report) | working on the codex surface |
| `docs/PRODUCTION-READINESS.md` | distribution, release, operations | shipping |
| `docs/RELEASING.md` | release procedure | cutting a release |
| `docs/HARDENING-AUDIT.md` | the hardening audit record | security/robustness history |
| `docs/EXPERIMENTS.md` | EXP-001..013: proof-of-advantage records | evidence for verified-iteration claims |
| `docs/RESEARCH-2026-08.md` | the AFK/Ralph/Sandcastle research pass | the supervisor's grounding |
| `docs/VERIFIABLE-REWARD-RESEARCH.md` | the verifiable-reward research (verifier discipline) | verification semantics |
| `docs/METRICS.md` | metrics/telemetry notes | observability |
| `docs/STANDARDS-AUDIT.md` | skills/standards inventory, plane coverage, gaps, the universal standards package | standards work |
| `docs/harness/` | harness evolution records (Phantom Guardrails) | harness changes |
| `AGENTS.md` | the repo contract (top of the tree, always loaded) | every session start |
| `CHANGELOG.md` | release history | release notes |
| `README.md` | the public face | onboarding |

## Overlap conventions

- `PRODUCTION-READINESS.md` points at `docs/AFK-SUPERVISOR.md` for the
  supervisor details — it does NOT duplicate them.
- `CODEX-INTEGRATION.md` is the CLIENT surface (registration, approvals,
  tools); `AFK-SUPERVISOR.md` is the SEMANTICS (loop, resume, template,
  liveness). Cross-referenced, not duplicated.

## Load-bearing paths (do not move without an ADR)

`evals/cases/`, `evals/golden/`, `evals/results/baseline.json`,
`memory/canonical/`, `memory/derived/`, `memory/episodic/`,
`scripts/verify.sh`, `scripts/gate-lib.sh`, `scripts/checkpoint.sh`,
`tickets/`, `docs/adr/`.
