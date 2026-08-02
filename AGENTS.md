# AGENTS.md — mini-agi (Rust product)

mini-agi: single-binary agent kernel — enforcement-bound memory, evaluation,
skills registry, checkpoint journal. CLI + MCP server. Rust, edition 2024.

Canonical spec: PoC at `/home/krn/coding/krn/mini-agi` (tag `v1-spec-reference`)
is the FROZEN behavioral contract — its 82 tests, 11 eval cases and golden
trajectories define behavior we port; do not "improve" semantics without an
ADR. Master plan: `docs/PLAN.md`.

## Toolchain (pinned)

- `rust-toolchain.toml` pins `1.97.1` (stable, 2026-07) + rustfmt + clippy.
- Dependencies: `sha2 0.11`, `serde 1`, `serde_json 1`, `thiserror 2`, `clap 4`.
- Kernel crate (`mini-agi-core`) stays std-only + the above. No async/tokio
  without an ADR. No unsafe (`unsafe_code = "forbid"` in workspace lints).

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

## Communication (no yapping)

- Facts and next actions. No filler, no restating the task, no praise.
- If the user says "caveman mode", follow `.agents/skills/caveman/SKILL.md`
  for every response until told to stop.

## Terminal conditions

- Max 3 implementer retries per ticket, max 40 steps, goal re-check after
  every stage. On any violation: stop, checkpoint, report — no reactive
  loops.
