# Architecture — mini-agi condensed (post `condense-core`, 2792d5d)

Status: design baseline for the codex REWORK (0/8) findings 1-10. Tracks ADR-0016
(gap lifecycle + verified closure). This is a spec, not a code change; where it
disagrees with a removed module's docs, this document wins.

## 1. Business model and the one pipeline

mini-agi is a single-binary agent kernel whose business model is

```
research -> knowledge -> patterns -> implementation
```

Every module serves one of those four stages. There is exactly one verification
doctrine: *verified before trusted* (ADR-0011). A run's `outcome.achieved` is the
run's OWN claim until `loop verify` executes the run's declared deterministic gate
(`verify_command` in `verify_target`) and records the pass. Nothing else closes a
gap. The measurement machinery (composite scoring, judge calibration, registers,
metrics, health, audit, eval-gate) was removed because it protected nothing; the
gate IS the verification. The pipeline and its exact function boundaries are in §6.

## 2. Authoritative data model

The filesystem is the only source of truth; nothing lives only in memory.

| Path | Record | Writer | Reader |
| --- | --- | --- | --- |
| `evals/cases/<case>/run.json` | one run's self-claim + declared gate (`Run`) | worker / supervisor | loopcmd, supervisor |
| `evals/ledger/<case>.json` | the authoritative gap lifecycle record (`Gap`) | **loopcmd only** | loopcmd, MCP, gate |
| `tickets/TICKET-<n>.md` | work item (status is a derived view) | dispatch / verify | ticket, loopcmd |
| `tickets/claims.md` | lease registry (never hand-edited) | ticket claim/release | ticket, loopcmd |
| `memory/canonical/entries/**` | append-only canonical facts | memory consolidate/signoff, dream | memory, gate |
| `memory/review/contested-*.md` | contested/enforcement facts for human signoff | memory, dream | memory signoff |
| `memory/derived/**` | generated views (never hand-edited) | memory derive | every session |
| `.agents/skills/<name>/SKILL.md` | patterns with optional verify hook | skill add | skills, gate |
| `memory/episodic/checkpoints.log` | edit checkpoint journal | checkpoint.sh | journal audit, gate |
| `.miniagi.json` + `MINIAGI_*` | runtime bounds | human | config |

### 2.1 `Run` (eval.rs) — the frozen input contract

Kept as-is except the dead fields removed (§10). Fields with live consumers:

- `goal: String`, `scope: Vec<String>` — the task (read by loopcmd, supervisor).
- `outcome.achieved: bool` — the run's OWN claim; never trusted alone.
- `verify_command: Option<String>`, `verify_target: Option<String>` — the gate.
  **Both must be present for a case to be dispatchable or verifiable** (§5.1, finding 4).
- `tokens_total`, `cost_usd` — budget inputs to the enforcement layer (§5.2).
- `trajectory: Vec<Step>`, `kernel_version`, `n_steps`, `n_toolcalls`,
  `latency_seconds`, `mode`, `extra` — the truthful trace header (written by
  `clifmt::build_run_draft`).

### 2.2 `Gap` — the one authoritative lifecycle record (new)

Per-case record at `evals/ledger/<case>.json`, written atomically (temp-file +
rename) under the claims lock. Schema:

```json
{
  "case": "flailing",
  "state": "dispatched",
  "ticket": "TICKET-12",
  "claimant": "worker-01",
  "dispatched_at": "2026-08-12T09:00:00Z",
  "attempts": 2,
  "closed_by": "flailing-rerun",
  "verified_at": null,
  "gate": "scripts/verify.sh",
  "note": null
}
```

`state` is one of `open | dispatched | closed | exhausted | unverifiable`. A row is
created only by `dispatch`; only `loopcmd` mutates it. Ticket `status: CLOSED` is a
DERIVED view of this record — the ledger is the authority for redispatch
prevention, never the ticket string.

### 2.3 Ticket + lease

Unchanged contract (ticket.rs): JSON or markdown tickets, `TICKET-<n>` ids, claims
registry guarded by the `O_EXCL` claims lock (30s stale-steal, 10s max wait).
`claim_ticket`/`release_ticket` are the ONLY writers of the registry.

## 3. The ONE gap lifecycle

A **gap** is a case whose `run.json` reports `achieved=false`.

```
                dispatch (valid + unclaimed)
 OPEN     ─────────────────────────────────▶  DISPATCHED
   │                                              │       │
   │ no verifier declared                         │       │ verify FAILS
   ▼                                              │       │ (attempts += 1)
 UNVERIFIABLE (never dispatchable)                │       ▼
                                                  │   DISPATCHED
                                                  │       │
                                                  │       │ attempts > max_rerun_attempts
                                                  │       │ (or budget spent)
                                                  │       ▼
                                                  │   EXHAUSTED   (terminal)
                                                  │
                                                  │ verify PASSES (gate + achieved)
                                                  ▼
                                             CLOSED        (terminal)
```

Terminal states: `CLOSED`, `EXHAUSTED`, `UNVERIFIABLE`. A terminal state is never
re-entered: redispatch is refused even if `run.json` later changes.

### 3.1 Transitions (all guarded by the claims lock)

- **OPEN → DISPATCHED** — `loopcmd::dispatch`. Preconditions (all checked in ONE
  validation function BEFORE any write): case name is a plain segment; `run.json`
  parses; `achieved == false`; `verify_command` **and** `verify_target` both present
  (finding 4); no ledger row in a terminal state; no active claim. Under the claims
  lock, in order: create ticket if missing → claim it (lease) → write `spec.md` →
  write the ledger row. On ANY failure mid-sequence, roll back every write this call
  performed (remove the created ticket, release the claim, remove the spec) so no
  orphan lease exists.
- **DISPATCHED → DISPATCHED** — `loopcmd::verify` fails (gate FAIL or
  `achieved == false`): `attempts += 1`, state unchanged, claim retained.
- **DISPATCHED → CLOSED** — `loopcmd::verify` sees gate PASS **and**
  `achieved == true`. One atomic close under the claims lock: write ledger
  `state=closed`, `closed_by=<closing rerun dir>`, `verified_at` → `release_ticket`
  → write the ticket file's `status: CLOSED`. If any step fails the whole close
  rolls back (stays DISPATCHED); a claim is never half-released.
- **DISPATCHED → EXHAUSTED** — `attempts > max_rerun_attempts`, or accumulated
  budget exceeds `max_cost_usd` / `budget_cost`. Ledger state set, claim released,
  ticket left OPEN with a note. Dispatch skips it forever.
- **OPEN → UNVERIFIABLE** — the case declares no `verify_command` or no
  `verify_target`. `objective`/`status` record the reason; `dispatch` refuses
  naming the missing field. The case stays OPEN but is not dispatchable until a
  human fixes `run.json`.

### 3.2 Rerun semantics (normalized)

- A rerun lives in `evals/cases/<case>-rerun` (first) and `<case>-rerun-2`,
  `<case>-rerun-3`, … (subsequent). Normalization: `rerun_dir(base, n)` =
  `base-rerun` for n=1, `base-rerun-{n}` for n>=2; `is_rerun_case` and
  `count_reruns` use exactly that form (loopcmd.rs approximates it today — make it
  the single normalizer).
- `attempts` = 1 (original) + number of rerun dirs.
- Only the BASE case has a ledger row. Rerun dirs are attempt artifacts and never
  get their own rows. `verify` accepts `<base>`, `<base>-rerun`, or
  `<base>-rerun-N`, strips the suffix, and closes the BASE (the gate always runs on
  the base's declared `verify_command`/`verify_target`); `closed_by` records the
  exact closing dir.
- `loop status` renders one row per base case: state, attempts, claimant, ticket,
  budget spent, and the closing rerun dir when closed.

### 3.3 Redispatch prevention

`dispatch`/`objective`/`pick_target` consult the ledger, not the ticket string:
- any terminal state → skipped (with the reason);
- a DISPATCHED row with a live claim → skipped (leased);
- a claim older than `max_wall_seconds` + a 15-minute margin is reported by
  `loop status` as a STALE lease (a leak, not an auto-release — the human or the
  claimant releases it). No silent lease theft.

## 4. Module responsibilities and seams

### Kernel crate — `mini-agi-core` (std-only, no platform deps)

- **`memory`** — the knowledge store. Reads `memory/canonical/entries/**`,
  `memory/canonical/preserved.md`; writes canonical entries (consolidation/signoff/
  dream/supersede kinds), the review queue, derived views. Seams: `consolidate`
  (buffer → facts/queue), `signoff` (queue → canonical, HITL-gated), `derive`
  (canonical → brief + fragments), `query_facts`/`select_budgeted`/`ranked_facts`
  (read), `canonical_fingerprint`/`provenance_block` (provenance),
  `write_supersede_entry` (lineage). Owns the rule that a preserved id is never
  superseded.
- **`store`** — pure path/parse helpers for the entry store (`next_entry`,
  `parse_canonical_facts`, `extract_fact_ids`). No writes of its own.
- **`dream`** — the knowledge→memory audit seam (D2). Pure logic: distiller/auditor
  prompts, JSON extraction, staging layout, verdict application. Writes
  `memory/staging/<date>/<seq>.md`, `<seq>.verdicts.json`, `<seq>.promotion.json`;
  promotes verdicts into canonical (via `memory`) or the review queue. Model
  invocations happen OUTSIDE this module, through the binary worker seam.
- **`eval`** — the `Run`/`Step`/`Outcome` data model only. No logic beyond
  `Run::achieved()`.
- **`loopcmd`** — the gap loop and the ONLY owner of the gap ledger. Reads
  `evals/cases/**/run.json`, tickets, claims, ledger; writes tickets, `claims.md`,
  `artifacts/<ticket>/spec.md`, `evals/ledger/<case>.json`. Exposes
  `status`/`dispatch`/`objective`/`verify`, `resolve_target` (§5.1), and the rerun
  normalizer. Enforces every budget bound it declares (§5.2) and the rule that
  verify never writes canonical (§5.4).
- **`skills`** — the pattern registry. Reads `.agents/skills/<name>/SKILL.md`;
  writes installed skills and the `disabled:` flag; runs verify hooks. Enforces
  name-path safety on every join.
- **`harness`** — the counterfactual gate. Reads repo files (target/candidate),
  runs `scripts/verify.sh`, writes `docs/harness/HARNESS-*.md` + `ledger.md`.
  Refuses to counterfactually validate the gate itself.
- **`ticket`** — ticket lifecycle + the lease registry. Reads `tickets/TICKET-*.md`
  + `tickets/claims.md`; writes claims.md (claim/release) and, on gap close, the
  ticket's `status: CLOSED`. Holds the claims lock (`lock_claims`) that every
  dispatch/verify/close transition uses.
- **`journal`** — checkpoint-journal parsing + completeness audit. Reads
  `memory/episodic/checkpoints.log`. Backs the gate's `checkpoint` step.
- **`config`** — `.miniagi.json` + `MINIAGI_*` env overlay. Loads **strict** for
  loop commands: malformed JSON or a malformed bound is an error, never a silent
  default (§5.2).
- **`redact`** — deny-by-default credential redaction (pure). Applied to every
  captured action, the gate command, and the gate target before persistence or
  display (§5.3).
- **`hash`** — sha256[:16] fact ids and source hashes (pure).
- **`capture`** — transcript → trajectory parser (pure). `parse_transcript`,
  `extract_result`, `completed`. Total on hostile input; never invents a step.
- **`worker`** — the kernel's ONLY subprocess-execution seam: `run_capped`,
  `run_capped_idle` (wall cap + idle cap), `budget_violations`,
  `parse_opencode_usage`. Every command that executes (worker, verifier gate, skill
  hooks via `sh -c`) passes through here or the sandbox wrapper; no other module
  calls `std::process::Command` for gates.

### Binary crate — `mini-agi` (Linux-only `landlock` dep in this crate)

- **`worker`** — the codex/opencode adapter + verified iteration
  (`run_verified_iteration`): budget-capped, Landlock-sandboxed worker runs,
  failure distillation, run.json draft via `clifmt::build_run_draft`. Writes
  `codex.log`, `run.json`.
- **`supervisor`** — the AFK `loop run` executor: resolves a case/ad-hoc goal into
  a verifiable spec (`resolve`), drives `run_verified_iteration`, writes
  `progress.md` and `REPORT.md`. **Currently compiled but unwired** — §6 wires it
  (`mini-agi loop run` + `loop_run`/`run_status`/`run_report` MCP tools).
- **`sandbox`** — Landlock write-containment policy (`apply`), applied by the
  `exec-sandbox` wrapper process. Read+execute everywhere, writes confined to the
  allow-set. Explicit degradation warning when Landlock is unavailable.
- **`clifmt`** — the truthful run.json draft builder. `outcome.achieved` is always
  false in a draft; actions are redacted; trace header stamped.
- **`mcp`** — the stdio MCP server. Owns the tool registry, which becomes the
  single source of truth for tool schemas AND for `init`'s generated config (§6.5).
- **`init`** — repo scaffold. Regenerates `.codex/config.toml`, `opencode.json`,
  `AGENTS.md`, scripts from the actual registry; never clobbers existing files.

## 5. Security invariants

### 5.1 Target resolution (finding 3)

One seam, `loopcmd::resolve_target(root, declared) -> Result<PathBuf, String>`, used
by `verify`, `supervisor::resolve`, and the worker:

1. `declared` empty → error ("no verify_target").
2. Relative → `root.join(declared)`; absolute → used as-is.
3. `canonicalize()` the result (resolves symlinks).
4. The canonical path MUST stay under the canonical root (lexical `starts_with`
   after canonicalization), UNLESS `.miniagi.json` sets
   `allow_outside_targets: true`. Default `false`: an outside target is rejected
   with the resolved path in the error.
5. The target must exist and be a directory.

Outside targets are therefore OPT-IN and explicit, never implicit. A symlink
planted inside the repo that escapes the root is caught by step 4 (resolution
happens on the canonicalized path).

### 5.2 Command execution bounds (findings 2, 3)

- Every gate run (`loop verify`, the iteration core's verifier, the supervisor fix
  verifier) executes through `worker::run_capped("sh", ["-c", cmd], target, 120)` —
  a hard 120s wall cap — and its captured output is truncated at **8 MiB**. No bare
  `Command::output()` exists for gate commands.
- Worker runs execute through `run_capped_idle` with the configured
  `max_wall_seconds` and `max_idle_seconds`; step/cost caps are enforced post-hoc
  via `budget_violations`; `max_tokens` via the transcript-bytes/4 governor.
- `config` is fail-closed for loop commands: a malformed `.miniagi.json` or a
  non-numeric `MINIAGI_*` bound is a hard error that refuses dispatch/verify, not a
  warning that silently means "unlimited" (finding 2).
- `--budget-cost` (CLI `loop objective`) is parsed strictly: non-numeric → error.
- `max_rerun_attempts` is ENFORCED at dispatch time (a case with
  `attempts > bound` is skipped as exhausted), not merely reported by `status`.

### 5.3 Redaction (finding 3)

- `clifmt::build_run_draft` already redacts every captured action (`redact::redact`).
- Extend the same call to `verify_command` and `verify_target`: redacted BEFORE
  being written into `spec.md`, `run.json`, the ledger `gate` field, and before
  being printed by `loop verify`/`loop status`.
- The redactor is deny-by-default on the byte stream; the original key/flag text
  stays visible so commands remain readable.

### 5.4 HITL for canonical writes (finding 5)

- A kernel-level approval seam in `memory`: every canonical write entry point
  (`consolidate` non-dry-run, `signoff`, `dream::apply_verdicts` promote path,
  `write_supersede_entry`) takes an `Approval { reason: String, principal: String }`;
  an empty reason is refused at the WRITE LAYER, not just the CLI. CLI and MCP pass
  `--approve`/`approve` through.
- `loop verify` NEVER writes canonical. Gap closure records the fact in the ledger
  (`note` + fact id) and appends it to the episodic buffer; promotion to canonical
  is the human's `mem consolidate --approve` (ADR-0010 keeps signoff human by
  design). The current `let _ = consolidate(require_signoff: false, ...)` inside
  `verify` is deleted.
- No consolidation error is ever swallowed: every `consolidate`/`signoff` result is
  checked and propagated. A failed write is a failed operation, never a silent skip.
- `dream` (CLI) gains the `--approve` requirement the MCP `dream` tool already has.

### 5.5 Path safety and lease integrity

- Plain-segment validation everywhere a user string becomes a path component: case
  names (`case_is_plain_segment`), ticket ids (`find_ticket` digit-prefix), skill
  names, snapshot names. No `..`/separator/leading-dot traversal.
- Claims are leased under the `O_EXCL` lock with stale-steal; dispatch/verify/close
  are atomic under it; mid-dispatch rollback removes orphan tickets/claims/specs;
  the close sequence releases the lease and marks the ticket closed in one locked
  step (finding 1).

## 6. End-to-end pipeline (finding 6)

| Stage | Where | Boundary (function) | Artifact |
| --- | --- | --- | --- |
| 1. Source material | research skill (external) lands findings | — | `knowledge/sources/<slug>.md` |
| 2. Distill | cheap model via `run_opencode_worker` | `dream::distiller_prompt` → worker seam → `dream::parse_distilled_facts` → `dream::write_staging` | `memory/staging/<date>/<seq>.md` |
| 3. Audit | strong model, independent of the distiller | `dream::auditor_prompt` → worker seam → `dream::parse_audit_verdicts` → `dream::write_verdicts` | `<seq>.verdicts.json` (persisted, truthful) |
| 4. Signoff / promote | kernel applies the RECORDED verdicts | `dream::apply_verdicts` → `memory::write_canonical_entry` (kind `dream`) OR review queue (conflict/enforced/preserved) → `memory::signoff` (HITL) | canonical entries / `memory/review/` |
| 5. Derive | deterministic views | `memory::derive` | `memory/derived/context-brief.md`, fragments |
| 6. Pattern | knowledge → skill with a verify hook | `skills::install_skills` / `skills::verify_all_skills` | `.agents/skills/<name>/SKILL.md` |
| 7. Implementation | gap → slice → supervised worker | `loopcmd::dispatch` (gap ledger) → `supervisor::run` / `worker::run_verified_iteration` | `artifacts/<ticket>/spec.md`, `<workdir>/run.json` |
| 8. Deterministic closure | the run's own gate | `loopcmd::verify <base>-rerun` (ledger CLOSED + lease release + ticket CLOSED) | `evals/ledger/<case>.json` state=closed |

Exact boundaries: stages 2-3 cross the crate seam at `worker::run_opencode_worker`
(the binary owns process execution; `dream` owns prompts/parsing). Stages 7-8 cross
at `loopcmd::dispatch`/`loopcmd::verify` (the kernel owns lifecycle; the binary owns
the worker process). Stage 6 is the only stage with a human decision point by
default: `skill add` requires approval, and the skill must carry a verify hook to
count as a *pattern* rather than a reference.

The wiring this document adds: `supervisor` gains CLI (`mini-agi loop run
<goal-or-case>`) and MCP (`loop_run`, `run_status`, `run_report`) entry points that
route through the gap ledger; `dream`'s CLI runs the full stage 2-4 sequence (it
currently skips the auditor and consolidates directly).

### 6.5 MCP registry as the single source of truth (finding 7)

`mcp.rs`'s `TOOLS` table becomes the one registry: each `ToolDef` carries a typed
JSON-Schema fragment (`{"type":"object","properties":{...},"required":[...]}`) and
`tools/list` returns `name`, `description`, and `inputSchema`. The registry is
exposed (`pub`) so `init::codex_config` and a new `mini-agi mcp --dump-schema` build
`.codex/config.toml` and the docs FROM it. The stale 39-entry allowlist in
`init.rs` is deleted; the repo's own `.codex/config.toml` and
`docs/CODEX-INTEGRATION.md` are regenerated from the real registry.

## 7. Test strategy

### 7.1 The current 45 (frozen baseline — must stay green)

- **Core unit tests — 26** (`crates/mini-agi-core`, `src/lib.rs`):
  - `capture` (10): transcript parsing, honest-`ok` capture, look-ahead exit
    binding, noise filtering, total-on-hostile-input, unicode case-folding,
    completion-marker position, `<result>` extraction.
  - `worker` (16): wall-cap kill, idle-cap kill, completion grace, budget_violations
    boundaries (strict, at-cap-not-violation, None=unlimited), opencode usage
    parsing (reported cost wins, rate-card fallback, zero-cost beats estimate,
    truncated telemetry, no-panic on huge counts).
- **Binary unit tests — 8** (`crates/mini-agi`):
  - `clifmt` (5): draft is never a success claim, action redaction, `ok` preserved,
    tool/step counts, failed capture stays failed.
  - `sandbox` (3): empty policy, dedup+skip-missing, symlink canonicalization.
- **Integration — 11** (`crates/mini-agi-core/tests/consolidate.rs`): the PoC
  behavioral port — extraction, provenance frontmatter, sequence numbering,
  cross-entry dedup, dry-run purity, signoff routing, double-promote rejection.

Total 45. These are behavior-locked by the `v1-spec-reference` lineage; the condense
did not change their assertions.

### 7.2 New tests (finding 8) — one falsifier per transition/boundary

New `crates/mini-agi-core/tests/gap.rs` (the loop lifecycle, on tmp roots):

1. OPEN→DISPATCHED creates ticket + claim + spec + ledger row in one call.
2. dispatch refuses a case with an active claim (no double lease).
3. dispatch refuses a terminal (closed/exhausted) case even if run.json was
   re-edited.
4. verify FAIL → stays DISPATCHED, attempts increments, claim retained.
5. verify PASS → ledger closed + claim released + ticket CLOSED (atomic: assert all
   three after one call).
6. verify PASS closes the BASE when invoked on `<base>-rerun-2` (normalization).
7. redispatch after close is refused.
8. mid-dispatch spec-write failure rolls back the ticket + claim (no orphan lease).
9. dispatch rejects a case missing `verify_command`; rejects one missing
   `verify_target` (two asserts, finding 4).
10. `resolve_target`: relative→root; absolute-inside-ok; absolute-outside→reject;
    symlink escape→reject (finding 3).

New `config`/`worker` tests:

11. malformed `.miniagi.json` → strict load errors (finding 2).
12. malformed `MINIAGI_MAX_STEPS` env → strict load errors, not unlimited.
13. `objective` stops dispatching when accumulated budget exceeds `budget_cost`.
14. `objective` skips a case past `max_rerun_attempts` (exhausted).
15. gate run is wall-capped: a `verify_command` that hangs is killed at 120s.
16. gate output > 8 MiB is truncated.

New MCP/init tests:

17. `tools/list` returns a valid `inputSchema` (typed properties) for every tool
    (finding 7).
18. `init`'s generated `.codex/config.toml` enable-list equals the MCP registry
    exactly (no stale entries) (finding 7).
19. `verify_command`/`verify_target` with embedded credentials are `[REDACTED]` in
    spec.md, ledger, and `loop verify` output (finding 3).

New HITL tests:

20. canonical write without an approval reason is refused at the memory layer
    (finding 5).
21. `loop verify` produces no new canonical entry (closure lives in the ledger +
    episodic buffer only) (findings 1, 5).
22. a failing `consolidate` propagates its error (no `let _ =`) (finding 5).

## 8. Resolution of the codex findings

| # | Finding | Decision (this document) |
| --- | --- | --- |
| 1 | One authoritative gap lifecycle; atomic close; redispatch prevention; rerun normalization | §2.2 `Gap` ledger + §3 state machine; close is one atomic locked step (ledger+lease+ticket); redispatch consults the ledger; `rerun_dir`/`is_rerun_case` normalizer in §3.2. |
| 2 | Restore enforcement; reject malformed budgets | §5.2: strict config load, strict `--budget-cost`, enforced `max_rerun_attempts`, `budget_cost` governor in `objective`, `budget_violations` wired into verify. |
| 3 | Resolve targets; sandbox gates; timeout/output cap; redact commands | §5.1 `resolve_target` seam; gate runs through `run_capped` (120s, 8 MiB); commands/targets redacted before persistence/display. |
| 4 | Transactional dispatch; reject missing verifier+target | §3.1: one pre-write validation requiring BOTH fields; mid-dispatch rollback under the claims lock. |
| 5 | Restore HITL; stop ignoring consolidation failures | §5.4: kernel-level approval seam; `loop verify` never writes canonical; every consolidate result checked; CLI `dream` gains `--approve`. |
| 6 | Connect research→knowledge→patterns→implementation | §6 pipeline table with the exact function boundaries; wire `supervisor` (`loop run` + `loop_run`/`run_status`/`run_report`); `dream` runs distiller→auditor→promote. |
| 7 | Valid MCP input schemas; regenerate init/config from the actual registry | §6.5: registry in `mcp.rs` emits `inputSchema`; `init::codex_config`, `docs/CODEX-INTEGRATION.md`, and the repo's `.codex/config.toml` are generated from that registry (14 tools today; 17 after wiring `loop_run`/`run_status`/`run_report`). |
| 8 | Focused tests per transition/boundary | §7.2, 22 tests keyed to the transitions and boundaries. |
| 9 | Record semantics in an ADR | §9: ADR-0016. |
| 10 | Delete fake compat fields and stale docs | §10. |

## 9. ADR-0016 (to be written by the implementer)

`ADR-0016-condensed-gap-lifecycle.md`: the `Gap` ledger, the state machine
(open/dispatched/closed/exhausted/unverifiable), the lease semantics and atomic
close, rerun normalization, the fail-closed budget rule, the target-resolution
policy (outside targets opt-in), and the HITL rule that verify never writes
canonical. Supersedes the composite/threshold semantics ADR-0011 carried forward.

## 10. Dead fields and stale docs to delete/regenerate (finding 10)

Removed from code (no live consumers since the 13 modules were cut):

- `loopcmd::LoopRow.composite`, `rerun_composite`, `best_composite`,
  `repair_signal`; `LoopStatus.composite_avg` (all fabricated constants).
- `eval::RepairSignal` (last consumer was `repair_signal`).
- `eval::Run.golden`, `reflection`, `mast` (consumers — failure register, mismatch
  register, golden eval — are gone).
- `config::Config.regression_tolerance` (no eval gate consumes it).
- `harness::spec_text`: drop the composite/threshold/tolerance/mismatch/registers
  lines; rewrite to the 9-step gate (build, fmt-check, clippy, tests, skills,
  checkpoint, provenance, derive, sandbox) + the counterfactual rule.
- `clifmt::build_run_draft`'s `outcome.tests` / `outcome.typecheck` keys — emit only
  `outcome: {"achieved": false}` (the `eval::Outcome` model has no such fields).

Regenerated from the actual registry (finding 7):

- `.codex/config.toml` — 39 stale tool entries (audit, budget, eval_gate,
  eval_score, eval_steps, health, insights, resume, run_failures, run_ingest,
  run_verify, skill_verify, stats, ticket_*, validate, backlog, …) → the real
  registry.
- `init.rs::codex_config` — the same stale 39-entry array → generated.
- `init.rs::AGENTS_MD` — advertises `mem verify`, `mem supersede/preserve/unpreserve`,
  `resume`, `health`, `audit`, `insights` → rewritten for the condensed CLI.
- `docs/CODEX-INTEGRATION.md` — `run_verify`, `eval_gate`, `run_ingest`,
  `ticket_claim`, `ticket_release`, `resume` → rewritten to the real registry.
- `docs/AFK-SUPERVISOR.md` — describes `mini-agi loop run`, currently unwired;
  rewritten to the wired seam (§6).
- `docs/README.md` — load-bearing-paths table cites `evals/results/baseline.json`
  and `evals/golden/` as gate inputs; neither is read by the 9-step gate.
  Rewrite the path table to the records in §2.
- `docs/PRODUCTION-READINESS.md`, `docs/HARDENING-AUDIT.md` — cite removed modules
  (health, metrics, audit, eval-gate, verifier); trim to what the condensed kernel
  actually implements.

Keep (still live): `evals/cases/`, `memory/canonical/`, `memory/derived/`,
`tickets/`, `docs/adr/`, `scripts/verify.sh` + `gate-lib.sh` + `checkpoint.sh`.

