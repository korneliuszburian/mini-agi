# PIPELINE-DESIGN.md — research → knowledge → patterns → implementation

Status: design baseline for the condensed kernel (commit `2792d5d`). Makes the
business model in `ARCHITECTURE-CONDENSED.md` §1 real and minimal. Every stage
name below is a real CLI command, MCP tool, or kernel function; where the design
adds a seam that does not exist yet, it is marked **[wiring]**. The one
verification doctrine applies throughout: a run's `outcome.achieved` is the
run's OWN claim until its declared gate passes (`loop verify`); the gate IS the
verification, nothing else closes a gap (ADR-0011).

```
 source material → dream distiller → staged facts → auditor verdicts
   → promote (HITL) → canonical → derive → patterns (.agents/skills)
   → slice spec → loop dispatch → worker → gate verify → ledger CLOSED
```

Stages 1-4 = `research → knowledge`, stage 6 = `knowledge → patterns`,
stages 7-8 = `patterns → implementation`. Stage 5 (derive) is deterministic and
touches both sides.

---

## 1. INPUT contract — what "research/source material" is

### 1.1 File, location, provenance

The pipeline consumes ONE kind of input: a markdown file at
`knowledge/sources/<slug>.md` (the registry; `knowledge/sources/registry.json`
indexes it). The `research` skill's raw output lands at `research/<slug>.md`;
promoting it to the input registry is one file move plus a provenance header
(no tooling — the file is the unit).

Required structure (the research skill's binding contract, verbatim):

```
# <slug>                                  # one question/topic, stated precisely
- source: <url or path>                   # primary source, required
- date: YYYY-MM-DD                        # required
- researched_by: <agent/worker id>        # required
## Findings                               # every claim carries a nearby citation
- **<claim>** (fact|estimate|opinion) ... Source: <name> <url>
## Sources                                # first-party only; no invented URLs
## Verdict                                # established / uncertain / what would settle it
```

Enforcement rules (from the research skill contract; the distiller does not
re-verify these, the operator's audit step does):
- NO FABRICATION: an unverifiable claim is labeled
  `unknown — not verifiable from the sources I reached`, never invented.
- A claim without a nearby source is labeled `opinion`, never `fact`.
- Hedged claims STAY hedged in the file and in canonical (EVIDENTIAL REGISTER,
  `dream::distiller_prompt`).

### 1.2 The dream distiller's exact behavior

`mini-agi dream --source knowledge/sources/<slug>.md` [**wiring**: the CLI today
parses the file as pre-distilled JSON and consolidates directly, skipping the
distiller+auditor — `ARCHITECTURE-CONDENSED.md` §8 finding 6; this section is the
target contract].

Stage 2 — DISTILL (cheap model, no canonical writes):

1. `dream::distiller_prompt(material)` builds the prompt: extract durable,
   load-bearing facts (decisions, mechanisms, evidence, constraints); skip
   ephemera (status updates, task noise, greetings); keep the source's register.
2. Binary seam: `worker::run_opencode_worker(workdir, "cheap-model", prompt,
   wall_cap, idle_cap)` — same budget/sandbox contract as loop runs.
3. `dream::parse_distilled_facts(output)`: first balanced `[...]` JSON array of
   `{"body", "domain"}`; bodies < 8 chars dropped; malformed elements skipped,
   never fatal. On zero facts: bounded retry with `dream::distiller_retry_feedback()`.
4. `dream::write_staging(root, staged, source, extracted_by)` writes
   `memory/staging/<date>/<seq>.md` — one `## S-NNN (domain)` block per fact,
   provenance header (`date`, `source`, `extracted_by`) by construction.

What the distiller REJECTS: status updates, greetings, task noise, bodies < 8
chars, unparseable JSON (after retry). It does NOT reject hedge-wording — it
preserves it.

Stage 3 — AUDIT (strong model, independent of the distiller, still no
canonical writes):

1. `dream::auditor_prompt(staged, canonical_index)` — the index is
   `memory::read_facts` rendered as `id: body`. Verdict vocabulary:
   `promote | duplicate | conflict | reject`, with `existing_id` on
   duplicate/conflict and a reason. Binding failure-mode check (TrustMem):
   before promoting, test for OMISSION, CORRUPTION (hedge→confidence upgrade),
   FABRICATION — any one means `conflict`/`reject`, never `promote`.
2. `worker::run_opencode_worker(..., "strong-model", ...)`; on zero verdicts,
   bounded retry with `dream::auditor_retry_feedback()`.
3. `dream::parse_audit_verdicts(output, staged)` — out-of-range indexes and
   unknown verdicts are dropped.
4. `dream::write_verdicts(staged_path, verdicts)` persists `<seq>.verdicts.json`
   — the TRUTHFUL audit that `dream promote` later applies. The audit is never
   re-run at promotion time; the recorded verdicts are the authority.

Stage 4 — PROMOTE (HITL, the only canonical write):

`mini-agi dream --promote memory/staging/<date>/<seq>.md --approve "<reason>"`
[**wiring**: new subcommand; the MCP `dream` tool gains the same split] →
`dream::apply_verdicts(root, staged, verdicts, source, dry_run)`:

- `promote` → `memory::write_canonical_entry(kind="dream")`, unless the body
  carries `enforced_by:` (ADR-0010 → `memory::append_contested` to
  `memory/review/contested-<date>.md`) or collides with a preserved id
  (`memory::preserved_ids`) — both route to the human queue.
- `conflict` → human queue with the auditor's reason.
- `duplicate` → byte-identical body: skip; differing body: supersede
  (`memory::write_supersede_entry`, lineage in frontmatter `- supersedes:`) —
  never a silent edit. A preserved `existing_id` routes to the queue instead
  (preservation is a stronger contract).
- `reject` → skip with the reason.
- `dream::write_promotion_receipt` writes `<seq>.promotion.json` (with
  `staged_sha256`) LAST; `receipt_matches_staged` proves the promoted bytes were
  the audited bytes.

`--approve` is required (kernel-level HITL, `ARCHITECTURE-CONDENSED.md` §5.4):
an empty reason is refused at the write layer. Retry/cadence: `dream --idle`
loads + freshness-guards; a busy box skips (AGENTS.md contract).

---

## 2. KNOWLEDGE layer — audit, signoff, conflicts, query

Canonical is `memory/canonical/entries/<date>/<seq>.md` (`## F-NNN
<16-hex-id>` blocks, sha256[:16] = `hash::fact_id(body)`), append-only, dated,
provenance on every entry. `memory/derived/**` is generated, never hand-edited;
on any conflict canonical wins.

- **Audit / HITL signoff** — the two entry routes:
  - Uncontested promote (`dream promote`/`memory::consolidate`) writes
    canonical directly under `--approve <reason>`.
  - Contested/enforced/preserved facts land in `memory/review/contested-*.md`;
    `mini-agi mem signoff <queue> <index> --approve "<reason>"` →
    `memory::signoff` promotes one (double-promote is refused, `FactKnown`).
  - `mem consolidate <buffer> [--domain X] [--approve] [--dry-run]` is the
    episodic path: `memory::extract_candidates` (FACT: lines and bullets ≥ 8
    chars) → dedup by id → `memory::write_canonical_entry(kind="consolidation")`.
    `--dry-run` reports without writing (zero-loss discipline, EXP-014).
  - `loop verify` NEVER writes canonical: gap closure records the fact in the
    ledger (`note`) and the episodic buffer; promotion to canonical is the
    human's `mem consolidate --approve`. This is enforced, not polite.
- **Duplicates** — dedup by fact id is deterministic; wording-variants go to the
  queue (`require_signoff`) or become supersede entries. `memory::supersede_edges`
  + `memory::supersede_cycles` keep lineage a DAG; a preserved id is never
  superseded.
- **Conflicts** — the auditor decides at promote time; `conflict` verdicts are
  not auto-resolved, they go to the human queue with `existing_id` so the
  signoff decision is evidenced.
- **Query** — `mini-agi mem query [keyword] [--domain X]` / MCP
  `memory_query` → `memory::query_facts`: domain exact + keyword
  case-insensitive substring, then `memory::ranked_facts` (enforced 3 +
  link-degree 2 + recency). Budgeted retrieval: `memory::select_budgeted` for
  context loads. `mini-agi provenance` prints `canonical_fingerprint` — the
  provenance gate's input.
- **Derive** — `mini-agi derive` / `memory_derive` (MCP) →
  `memory::derive`: `render_brief` (≤ 8192 B `context-brief.md`) +
  `render_domain_agents` → `memory/derived/per-domain/AGENTS.<domain>.md`
  fragments, each carrying the fact ids it derives from. The gate proves
  determinism (derive twice, hash equal).

MCP surface: `memory_consolidate`, `memory_signoff`, `memory_derive`,
`memory_query`, `provenance`. Writes require `approve` (refused without it).

---

## 3. PATTERN layer — derived from knowledge, not separate

A **pattern** is a skill at `.agents/skills/<name>/SKILL.md` **with a `verify`
hook** (skills.rs ADR-0002 contract). A skill without a hook is a `ref`, never a
pattern — `skill list` shows `[verify]` vs `[ref]`. Patterns are GROUNDED in
canonical: the SKILL.md frontmatter `source:` cites the fact ids it
operationalizes, so every pattern traces to knowledge and every pattern update
is a knowledge update.

```
---                    # frontmatter (the contract)
name: retry-backoff    # must equal the dir name
description: ...       # short, for the budgeted listing
version: 1.0.0         # semver — the pattern's own version
source: mini-agi memory: <16-hex ids...>   # grounding facts (patterns derived from knowledge)
verify: sh -c "..."    # deterministic self-test run from the repo root
type: procedural       # or "mode" (e.g. caveman; hook-less legitimately)
---
# procedure body ...
## Done when            # the skill lint (skills::lint_skill) requires
- [ ] ... artifact anchor (path/quoted output/commit) ...
```

**Derivation** (knowledge → pattern):
1. A knowledge cluster that recurs (auditor keeps promoting the same domain;
   `memory::render_domain_agents` already emits `AGENTS.<domain>.md`) is the
   trigger. Writing the SKILL.md is agent+human work, but it must cite the fact
   ids (`source:`) and encode a checkable criterion (the verify hook / `Done
   when` + artifact anchor).
2. The verify hook makes the pattern falsifiable: it runs from the repo root;
   a failing hook fails the gate's `skills` step → the pattern is fixed or
   disabled (`skills::set_disabled`, P2-14 quarantine), never silently stale.

**Spreading** (patterns → agent context):
- Automatic: `memory::derive` regenerates per-domain AGENTS fragments (the
  hyperlocal context that carries the grounding ids).
- Explicit: `skill list` / `skill show <name>` / MCP `skill_list`/`skill_show`;
  agents get the registry as a BOUNDED listing (`skills::budgeted_list`, 8000
  char cap, verify-enabled ranked first) — progressive disclosure, no unbounded
  skill dumps.
- Scaffolding: `mini-agi init` regenerates `.codex/config.toml`, `opencode.json`,
  AGENTS.md, scripts FROM the real MCP registry (`mcp.rs` `TOOLS`); never
  clobbers existing files; CLAUDE.md import-shim is opt-in (`--claude-shim`,
  EXP-017 finding).

**Versioning and updates**: `version` + `source` are mandatory frontmatter
(`skills::verify_all_skills` reports `no_version`). Install from a git source:
`mini-agi skill add <git-url|owner/repo|path> --approve "<reason>"` →
`skills::install_skills`. Install is a DIFF: `skill_hash` (framed hash of the
whole dir) compares, only a differing unit is replaced, the old dir is renamed
to `<name>.local-before-<nanos>` — nothing is ever destroyed by an update. A
pattern whose grounding facts change gets a new version + updated `source:` +
re-verified hook; the canonical lineage (`supersedes:`) records the knowledge
change, the version bump records the pattern change.

---

## 4. IMPLEMENTATION loop — pattern → work → verified close

A **gap** = `evals/cases/<case>/run.json` with `achieved=false` AND both
`verify_command` and `verify_target` present (a case missing either is
`unverifiable` — never dispatchable, finding 4). The gate the worker cannot
see is the only close condition.

```
OPEN ──dispatch──▶ DISPATCHED ──gate FAIL──▶ DISPATCHED (attempts+1)
                        │                        │ attempts > max_rerun_attempts
                        │ gate PASS + achieved    ▼
                        └──────────────▶ CLOSED (terminal)   EXHAUSTED (terminal)
```

1. **Dispatch**: `mini-agi loop dispatch <case> --claimant <id>` (or MCP
   `loop_dispatch`, or batch `loop_objective --budget-cost $X`) →
   `loopcmd::dispatch`: under the claims lock (`ticket::lock_claims`, O_EXCL,
   30s stale-steal), create the ticket `tickets/TICKET-<n>.md` if missing,
   claim the lease, write `artifacts/TICKET-<n>/spec.md`
   (`loopcmd::write_spec`: goal, `verify_command in verify_target`, acceptance =
   `loop verify <case>-rerun`), write the ONE ledger row
   `evals/ledger/<case>.json` (`Gap`). Any mid-sequence failure rolls the whole
   call back — no orphan lease, no orphan ticket.
2. **Worker**: `mini-agi codex artifacts/TICKET-<n>/spec.md <workdir>
   --iterate 3 [--blind-worker] [--verify CMD --target DIR]`, or the supervised
   `mini-agi loop run <goal-or-case> --workdir <dir>` (supervisor `[wiring]`)
   → `supervisor::resolve` (case run.json or ad-hoc goal, verifier required) →
   `worker::run_verified_iteration`: budget-capped, Landlock-sandboxed,
   `clifmt::build_run_draft` writes a TRUTHFUL `run.json` (`achieved=false`
   always in a draft, actions redacted, trace header stamped). On gate failure
   it re-invokes a fresh worker with the distilled failure feedback, bounded by
   `--iterate` — the EXP-012/013 mechanism that turns blind below-the-bar
   generations into verified passes.
3. **Gate verify**: `mini-agi loop verify <case>-rerun --claimant <id>` (MCP
   `loop_verify`) → `loopcmd::verify`: resolve the target
   (`loopcmd::resolve_target`, outside targets opt-in), run the case's declared
   `verify_command` in `verify_target` via `worker::run_capped("sh", -c, 120s
   wall, 8 MiB)`. Gate PASS **and** `achieved=true` → one atomic close under
   the lock: ledger `state=closed` + `closed_by=<rerun dir>` + `verified_at` →
   release the lease → ticket `status: CLOSED`. Any failure leaves the state
   DISPATCHED with `attempts += 1`, claim retained. Verify NEVER writes
   canonical.
4. **Rerun semantics**: `evals/cases/<case>-rerun`, `<case>-rerun-2`, ...;
   `loopcmd` normalizes (`rerun_dir`/`is_rerun_case`/`count_reruns`); only the
   BASE case has a ledger row; verify on any suffix closes the base.
5. **Redispatch prevention**: terminal states (`closed | exhausted |
   unverifiable`) are never re-entered — dispatch consults the LEDGER, not the
   ticket string; a `DISPATCHED` row with a live claim is skipped; a lease older
   than `max_wall_seconds` + 15 min is reported by `loop status` as STALE (a
   leak the human/claimant releases — no silent lease theft).
6. **The gate** (`scripts/verify.sh`): build, fmt-check, clippy, tests, skills,
   checkpoint, provenance, derive, sandbox (CI-only). For a non-Rust project the
   pipeline gate is the project's OWN declared verifier (e.g. `npm run check`)
   via `loop verify`/`run verify` — NOT the embedded cargo gate (EXP-017
   finding).

MCP surface: `loop_status`, `loop_dispatch`, `loop_objective`, `loop_verify`
(+ `loop_run`/`run_status`/`run_report` once supervisor is wired — 14→17
tools).

---

## 5. MEASUREMENT — honest, and only what a decision reads

The condense removed the measurement machinery (composite scoring, judge
calibration, registers, metrics, health, audit, eval-gate) because it
protected nothing — "the gate IS the verification". This design does NOT
rebuild it. What is measured, and by whom:

| Question | Signal | Owner | Decision it feeds |
| --- | --- | --- | --- |
| Did the run pass? | the run's OWN gate exit code (`loop verify`, `scripts/verify.sh`) | `loopcmd::verify` / gate | close or retry — this is the only close condition |
| Does a closed gap STAY closed? | ledger terminal states; redispatch refuses CLOSED/EXHAUSTED/UNVERIFIABLE | `loopcmd::dispatch` (enforced, not reported) | skip re-work; `loop status` renders state+attempts |
| What did it cost / take? | `run.json`: `tokens_total`, `cost_usd`, `latency_seconds`, `n_steps`, `n_toolcalls` (truthful, `clifmt::build_run_draft`); `attempts` = 1 + reruns; `loop objective --budget-cost` accumulates `budget_spent` | worker / loopcmd | `max_rerun_attempts`, budget governor, EXPAND/PARTIAL/STOP |
| Is a pattern still alive? | its verify hook in the gate's `skills` step; a failing hook → gate red → fix or `set_disabled` | `skills::verify_all_skills` | keep vs demote vs disable |
| Is a pattern USED? | count of dispatched `spec.md`s whose `source:` cites the pattern's grounding fact ids (`rg` over `artifacts/**/spec.md`) — a derived count, not a judged score | agent/human, `loop status` | promote to a repo fragment / demote to ref |
| Is the pipeline itself worth it? | the weekly W/C/B usage-log row (wins, costs, bad memories), EXP-017 protocol | human | EXPAND / PARTIAL / STOP — this is the honest instrument, not a score |

Honest negatives, stated up front: a worker's in-run pattern usage is NOT
measured (it would need transcript instrumentation the condense removed).
"Does a pattern get used" is answered by (a) hooks staying green and (b) spec
citations — a low-cost adoption count. If either signal cannot be justified as
feeding a decision, it is cut (see §7).

---

## 6. Minimal worked example — one source → one fact → one pattern → one slice → verified close

1. **Source**: `knowledge/sources/retry-after-parsing.md`
   ```
   # retry-after-parsing
   - source: https://www.rfc-editor.org/rfc/rfc7231#section-7.1.1.1
   - date: 2026-08-12
   - researched_by: opencode-worker-1
   ## Findings
   - **Retry-After accepts both delta-seconds and an HTTP-date; a client that
     parses only the integer form fails on the date form** (fact, same source).
   ## Sources
   - RFC 7231 §7.1.1.1 (first-party).
   ## Verdict
   - Established: both forms are valid; integer-only parsers are a documented
     interop failure mode.
   ```
2. **Distill + audit**: `mini-agi dream --source knowledge/sources/retry-after-parsing.md`
   → distiller emits `S-000`; auditor (against the canonical index) verdicts
   `promote` — the claim is not in canonical → `memory/staging/2026-08-12/001.md`
   + `001.verdicts.json`.
3. **Promote (HITL)**: `mini-agi dream --promote memory/staging/2026-08-12/001.md
   --approve "rate-limit research from RFC 7231"`
   → canonical `## F-000 <9f3d…16hex>` (kind `dream`); `001.promotion.json` written last.
4. **Pattern**: `.agents/skills/retry-backoff/SKILL.md` with
   `source: mini-agi memory: <9f3d…>` and
   `verify: sh -c "test -f src/http/retry.rs && grep -q 'Retry-After' src/http/retry.rs"`
   → `skill list` shows `retry-backoff [verify]`.
5. **Derive**: `mini-agi derive` → `AGENTS.rate-limiting.md` fragment carries the id.
6. **Slice**: `evals/cases/retry-after/run.json` — `goal: "parse HTTP-date
   Retry-After, not just delta-seconds"`, `achieved:false`,
   `verify_command: "npm run check"`, `verify_target: <target repo>`.
7. **Dispatch**: `mini-agi loop dispatch retry-after --claimant opencode`
   → `TICKET-12` + lease + `artifacts/TICKET-12/spec.md` (cites the fact id) +
   `evals/ledger/retry-after.json` `state=dispatched`.
8. **Worker**: `mini-agi codex artifacts/TICKET-12/spec.md <workdir> --iterate 3
   --verify "npm run check" --target <workdir>` → gate PASS → truthful `run.json`
   at `evals/cases/retry-after-rerun/run.json` (`achieved:true` after the gate).
9. **Verify close**: `mini-agi loop verify retry-after-rerun --claimant opencode`
   → ledger `state=closed`, `closed_by=retry-after-rerun`, lease released,
   ticket `CLOSED`.
10. **Close the knowledge loop**: the closing `note` lands in the ledger + the
    episodic buffer; if the closing lesson is durable, the human runs
    `mem consolidate --approve` (verify never writes canonical).
11. **Measure**: `loop status` shows `retry-after attempts=2 CLOSED`; `run.json`
    `cost_usd`/`tokens_total` recorded; the spec cites `<9f3d…>` → the pattern
    adoption count += 1.

---

## 7. Anti-slop — what to CUT further when slop appears

1. **A measurement no decision reads is slop.** The only admission path for new
   machinery is the counterfactual gate (`mini-agi harness verify <target>
   <candidate> --claims <failure>`): it must show an OBSERVED failure reduction.
   A claim that cannot show one is REJECTED (Phantom Guardrails) and the change
   lands as a documentation commit — same rule that governs AGENTS.md/harness
   edits. Never re-add composite scoring, judge calibration, failure/mismatch
   registers, metrics, health, audit, or eval-gate.
2. **A gate step that never fails is slop.** If `scripts/verify.sh` has a step
   with no red in N runs, cut it. The gate stays 9 steps (or fewer).
3. **A skill without a verify hook is a `ref`, not a pattern** — it must not
   block the gate and must not be counted as a pattern. A pattern whose hook
   fails is fixed or disabled; a disabled pattern stays quarantined
   (`skills::set_disabled`), not re-run.
4. **Staging is a queue, not an archive.** `memory/staging/<date>/` files with
   no `*.promotion.json` are pending work; un-promoted batches older than the
   cadence are anomalies → promote or delete, never accumulate. An empty review
   queue (`memory/review/`) is HEALTH, not a problem — never manufacture
   contested facts to justify the signoff ceremony.
5. **No derived-view or journal edits, ever.** `memory/derived/**` is generated
   (canonical wins on conflict); `memory/episodic/checkpoints.log` is
   append-only and never repaired via `git checkout --` (INCIDENTS #2).
6. **One lifecycle record per case.** The `Gap` ledger row is the sole authority
   for redispatch; ticket strings and run.json are derived views. Duplicate
   lifecycle state is slop.
7. **Tool count is a budget.** 14 MCP tools today, 17 after wiring
   `loop_run`/`run_status`/`run_report`. A new tool needs a named consumer;
   a tool without one is removed. `init` regenerates `.codex/config.toml` from
   the registry so no stale allowlist ever returns.
8. **Operational rules (INCIDENTS) are anti-slop law:** never fuse destructive
   commands with edits in one shell line; never chain `grep -c` with `&&`;
   edit scripts write at the end, one fix per script.
9. **The pipeline's own cost is measured, not assumed** (EXP-017 W/C/B). If the
   weekly row shows cost > 30 min with no attributable wins → STOP expanding,
   cut ceremony first, and let the connector/CI/trail ideas EARN their place
   through §7.1, not the reverse.
10. **The kernel's value is verified-iteration when solo is below the bar plus
    persistent cross-run memory** (EXP-009/010/011 negatives, EXP-012/013
    positive). Any pipeline feature pitched as "beats solo codex" on tasks solo
    solves is slop by pre-registration — the controls already rejected that
    claim 7 times; do not re-litigate it with new tasks.
