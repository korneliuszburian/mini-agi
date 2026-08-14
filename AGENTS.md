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
ADRs: `docs/adr/` (ADR-0001..0014 local). Master plan: `docs/PLAN.md`.
Canonical memory import source: `agentic-core@HEAD`.

## Verification discipline (verified before trusted)

- A run's `outcome.achieved` is the run's OWN claim until
  `mini-agi loop verify <case>-rerun` confirms it: the declared
  `verify_command` executes in the resolved `verify_target` and the
  kernel reports CLOSED/OPEN. `loop verify` closes a gap ONLY when the
  run is achieved AND the base's declared gate passes (or
  `--allow-unverified` is explicit). Never report a run as successful
  without its verifier.
- The gap lifecycle is authoritative in `evals/ledger/<case>.json`
  (written by `loop dispatch`/`loop verify` under the claims lock);
  terminal states (closed/exhausted/unverifiable) are never redispatched.

## Harness evolution (Phase 8-9: guarded)

- `mini-agi harness` snapshots the harness spec + gate ledger row.
- `mini-agi harness verify <target> <candidate> [--claims]` swaps a
  candidate, runs the gate, and ACCEPTS only when the swap leaves the
  gate FULLY green (observed failures reduced to zero); a partial
  reduction that still leaves any failure REJECTS (fail-closed). A claim
  of fixing a failure never observed before the edit is REJECTED with
  evidence (Phantom Guardrails). The gate itself (scripts/verify.sh) is
  never its own counterfactual subject.
- Harness/AGENTS changes that cannot show a fully-green failure
  reduction land as normal documentation commits — the counterfactual
  gate only justifies failure-reducing edits.

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
- Dream-loop (D2): `mini-agi dream --source <material> --approve <reason>`
  parses distilled facts and consolidates them into canonical directly
  (HITL — canonical writes need --approve). The strong-model AUDITOR stage
  is NOT wired in the CLI/MCP path: enforcement-bound facts
  (`enforced_by`) always route to the human queue (ADR-0010 signoff) via
  `memory/review/`; `dream --promote <manifest>` applies an externally
  produced auditor verdicts manifest (idempotent via the promotion
  receipt).
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

This repo registers mini-agi as an MCP server for codex (`.codex/config.toml`;
details: `docs/CODEX-INTEGRATION.md`, semantics: `docs/AFK-SUPERVISOR.md`).
Every codex session here follows the same discipline:

- Session start: `loop_status` (open gaps), `memory_query <topic>` (facts
  before re-research), `checkpoint_audit` (journal state). Knowledge given
  once must not be re-asked.
- Post-run: a gap is UNVERIFIED until `loop_verify <case>` closes it — never
  claim success on an unverified run.
- Writes (loop_dispatch, loop_objective, loop_verify, memory_signoff/consolidate/derive,
  skill_add, dream) require an `approve` reason (HITL) — the kernel's
  memory-signoff gate is human by design (ADR-0010).
- Verification: `./scripts/verify.sh` ALL GREEN before a run is real.

## Process rules (hard lessons, standards-polish S3)

These rules exist because each one cost real work when violated:

- NEVER fuse destructive commands (pkill, rm -rf, git reset --hard)
  with edits in one shell line.
- NEVER restore the checkpoint journal through git (`git checkout --
  memory/episodic/checkpoints.log`): it destroys working-tree journal
  lines and orphans BEGINs. Repair through checkpoint.sh only.
- NEVER chain a `grep -c` (or any 0-matches command) with `&&` — it
  exits 1 on zero matches and silently breaks the chain.
- Edit scripts that write files MUST write at the end: an abort before
  the write loses the whole edit. Each fix lands in its own script.
- The reviewers are DEVIL'S ADVOCATES: adversarial, roasting,
  evidence-first. A review that finds nothing is suspect; a
  disposition that rejects a finding must disprove it with evidence;
  a finding that survives current-code verification is accepted.
- The human-review gate is per-domain: default system-side review
  (the kernel's loop + codex reviewer); domains marked HITL-required
  (frontend) get a mandatory human approval checkpoint — the zero-
  trust rule the user set.

Incident postmortems: `docs/INCIDENTS.md`.
