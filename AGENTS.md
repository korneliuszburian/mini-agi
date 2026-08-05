# AGENTS.md — mini-agi (Rust product)

mini-agi: single-binary agent kernel — enforcement-bound memory, evaluation,
skills registry, checkpoint journal. CLI + MCP server. Rust, edition 2024.

Three generations, one lineage (ADR-0001):
- v1 `agentic-core` — loop proof; its canonical facts = knowledge source.
- v2 `mini-agi` (tag `v1-spec-reference`) — FROZEN behavioral contract: its
  82 tests, 11 eval cases and golden trajectories define behavior we port;
  do not "improve" semantics without an ADR. In case of divergence
  PoC wins over v1.
- v3 = this repo.

Charter (founding goal, verbatim, NEVER lose or paraphrase): `docs/CHALLENGE.md`.
ADRs: `docs/adr/` (ADR-0001..0010 local). Master plan: `docs/PLAN.md`.
Canonical memory import source: `agentic-core@HEAD`.

## Verification discipline (Phase 8-9: verified before trusted)

- A run's `outcome.achieved` is the run's OWN claim until
  `mini-agi run verify <run.json>` confirms it: the declared
  `verify_command` executes in `verify_target` and the kernel reports
  verified / disagrees / unverified. `loop verify` closes a gap ONLY
  when composite >= 0.5 AND the verifier passes (or `--allow-unverified`
  is explicit). Never report a run as successful without its verifier.
- `run verify --dry-run` prints the command without executing it.
- `mini-agi eval judge-drift` reports how often the judged outcome
  disagrees with the deterministic layer (calibration corpus:
  memory/derived/calibration.md) — treat disagreement as a signal.
- Failure register entries carry MAST classification (14 modes) and
  reflections; `loop dispatch` injects them into slice specs — never
  repeat a recorded failure.

## Harness evolution (Phase 8-9: guarded)

- `mini-agi harness` snapshots the harness spec + gate ledger row.
- `mini-agi harness verify <target> <candidate> [--claims]` swaps a
  candidate, runs the gate, and ACCEPTS only on observed failure
  reduction; a claim of fixing a failure never observed before the edit
  is REJECTED with evidence (Phantom Guardrails). The gate itself
  (scripts/verify.sh) is never its own counterfactual subject.
- Harness/AGENTS changes that cannot show an observed failure reduction
  land as normal documentation commits — the counterfactual gate only
  justifies failure-reducing edits.

## Toolchain (pinned)

## Toolchain (pinned)

- `rust-toolchain.toml` pins `1.97.1` (stable, 2026-07) + rustfmt + clippy.
- Dependencies: `sha2 0.11`, `serde 1`, `serde_json 1`, `thiserror 2`, `clap 4`.
- Kernel crate (`mini-agi-core`) stays std-only + the above. No async/tokio
  without an ADR. The BINARY crate (`mini-agi`) additionally carries the
  Linux-only `landlock 0.4` dep for the worker sandbox (ADR-0012); the
  kernel crate itself has no platform-specific deps. No unsafe (`unsafe_code = "forbid"` in workspace lints).

## Verification is deterministic

- `cargo test` must be green after any code change. Quote real output.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must
  pass; they are CI gates. A pass you did not observe is a failed gate.
- Behavior ports from PoC are locked by tests copied from the spec (same
  inputs, same expected outputs, including exact 16-hex fact ids).

## Checkpointing (Coherence Collapse protection)

- `checkpoint.sh begin <label>` BEFORE every edit step.
- `checkpoint.sh verify <label>` after gates pass (rolls back on red).
- Journal semantics (T008 amendments): a BEGIN is resolved by a subsequent
  VERIFY-PASS or VERIFY-FAIL; an unpaired BEGIN is an anomaly unless it is
  the literal last line (verification in progress). Never edit the journal.

## Memory rules (canonical-first)

- `memory/canonical/` is the only hand-written source of truth (append-only,
  dated, provenance on every entry, fact ids = sha256[:16] matching PoC).
- Derived views (`memory/derived/`, brief, fragments) are generated, never
  hand-edited; on conflict canonical wins.
- Facts are enforced or reference: a fact with an `enforced_by` check is
  bound to a gate that runs in CI (ADR-0010). The kernel's own memory is a
  first-class dogfood user.
- Read the brief + index before working. Knowledge given once must not be
  re-asked or re-researched.

## Code review rules

- Review with the rubric in `.agents/checks/review-rubric.md` (correctness,
  security, tests, scope, each 0-2; APPROVE >=7, FIX-MINOR 5-6, REWORK <5).
- Default to action: check, mark, route. Ask a human only for REWORK or
  ambiguous contract decisions.
- Reviews are fresh-session and independent; implementer self-reviews are
  not independent evidence.
- Reviewer is MEMORY-ANCHORED (ADR-0003): a verdict must cite canonical
  fact ids it relies on; a review without memory anchors fails the gate.
  Deterministic gates run before the LLM judge, never after it alone.

## Communication (no yapping)

- Facts and next actions. No filler, no restating the task, no praise.
- If the user says "caveman mode", follow `.agents/skills/caveman/SKILL.md`
  for every response until told to stop.

## Terminal conditions

- Max 3 implementer retries per ticket, max 40 steps, goal re-check after
  every stage. On any violation: stop, checkpoint, report — no reactive
  loops.

## codex sessions in this repo (AFK-SUPERVISOR S4)

This repo registers mini-agi as an MCP server for codex (`.codex/config.toml`).
Every codex session here follows the same discipline:

- Session start: `loop_status` (open gaps), `memory_query <topic>` (facts
  before re-research), `checkpoint_audit` (journal state). Knowledge given
  once must not be re-asked.
- Post-run: a run is UNVERIFIED until `run_verify <run.json>` passes — never
  claim success on an unverified run. Close gaps with `loop_verify <case>`.
- Writes (loop_dispatch, memory_signoff/consolidate/derive, run_ingest,
  ticket_claim/release, skill_add, harness) require a prompt (HITL) — the
  kernel's memory-signoff gate is human by design (ADR-0010).
- Verification: `./scripts/verify.sh` ALL GREEN before a run is real.
