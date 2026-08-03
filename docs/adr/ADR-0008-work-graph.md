# ADR-0008 — work graph: blocking edges and claim leases

Status: accepted (2026-08-03)

## Context

Phase 6.4 (proactive composition) needs parallel, non-colliding agents.
Two gaps were identified against the convergent harness shape (Yegge,
"Shape of Things to Come Part 1", 2026-08; canonical
`2026-08-03-002`):

1. **No dependency edges between work units.** Yegge's Beads ledger is a
   graph: beads carry dependency and parent/child edges, gates and
   triggers. Our tickets are an unordered set — `ticket validate` checks
   each file in isolation, so nothing expresses "TICKET-9 blocks
   TICKET-10" and nothing prevents an agent from starting work whose
   prerequisite is open.
2. **No claim/lease mechanism.** "Parallel agents that don't step on
   each other" requires atomic claiming; we currently rely on a manual
   rule (one integrator, isolated worktrees). The failure register and
   mismatch register (ADR-0005, Phase 6.1/6.2) prove the value of
   recorded state; claiming is the next recorded state.

## Decision

1. **Blocking edges are ticket data.** Both accepted ticket forms gain
   an optional `blocked_by` field: a list of `TICKET-<n>` ids whose
   completion this ticket depends on (markdown frontmatter list, or
   JSON array). Absent = no dependencies. `ticket validate` cross-checks
   edges: every referenced id must exist in `tickets/`, and the graph
   must be acyclic (no ticket blocks itself, directly or transitively).
2. **Claims live in one registry, `tickets/claims.md`**, written only by
   `ticket claim <id> [claimant]` and `ticket release <id>` — never
   hand-edited (same rule as derived views). A claim records
   `(ticket, claimant, since)`; claiming an already-claimed ticket by
   another claimant fails (lease semantics). Releasing a ticket you do
   not hold fails. Re-claiming by the same claimant is a no-op
   (idempotent). Claims never touch ticket files, so the PoC JSON
   contract is untouched.
3. **`ticket graph`** prints the dependency graph (edges and cycles are
   visible), and `ticket claim <id>` refuses a ticket with unresolved
   `blocked_by` — you cannot lease work whose prerequisites are open
   (unless `--force`).
4. This is additive: existing tickets (no `blocked_by`) parse and
   validate exactly as before; `claims.md` absence is an empty registry.

## Consequences

- A parallel session can now ask "who holds what" (`ticket claim
  --list`) before starting; the integrator sees the whole claim set in
  one file.
- `ticket validate` becomes the graph gate; CI (verify.sh) already runs
  ticket validation — add edge checks there.
- Reputation/stamps (portable attestation, Yegge F-008) are explicitly
  deferred; composite score is the current single-dimension stamp.
- Land Rush / swarm diagnosis (F-003) is deferred: mini-agi still runs
  serial gates, which is correct at current scale.
