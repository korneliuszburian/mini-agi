# ADR-0007 — typed contracts: handoff and ticket documents are schema-validated

Status: accepted (2026-08-03; file created 2026-08-04 as the missing
authority — the ADR number was referenced by `main.rs` and `contract.rs`
but the file did not exist; this document records the decision that was
already implemented).

## Context

The kernel exchanges typed documents between components and agents:
eval runs (`run.json`), tickets, slice specs, and verdicts. Ad-hoc JSON
that "looks right" causes silent failures downstream — a missing
`outcome.achieved`, a ticket without a goal, a spec without acceptance
criteria. The PoC (`scripts/validate.py`) already enforced a handoff
contract; the Rust port kept the behavior but the authority for it was
never written down (the ADR-0007 file was missing while `main.rs` and
`contract.rs` cited it).

## Decision

All typed documents pass through a schema validation step before they
are trusted:

- `run.json` is validated by `eval::Run::validate` before scoring,
  verification, or ingest — a run that does not parse or that violates
  the schema is rejected with a message, never scored with defaults.
- Tickets are validated against the `ticket` contract (`mini-agi ticket
  validate`, `mini-agi validate ticket <file>`) — required fields
  (id, title, goal, domain) and optional gated fields are checked.
- Slice specs and verdicts carry the same discipline where the PoC
  defined it.

This is the load-bearing seam behind "verified before trusted": a
document whose schema is wrong cannot become an input to the eval gate,
the verifier, or the checkpoint journal.

## Consequences

- `contract.rs` (mini-agi-core) is the single validator; CLI subcommands
  dispatch to it.
- Validation errors surface as explicit failures, not silent fallbacks.
- Adding a new document type means adding a validator in `contract.rs`,
  not ad-hoc checks in callers.

## Supersedes / related

- PoC `scripts/validate.py` semantics (ported 1:1).
- ADR-0007 was referenced by `main.rs` (`validate` command) and
  `contract.rs` before this file existed; the reference is now
  resolvable.
