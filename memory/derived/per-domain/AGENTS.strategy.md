# PROVENANCE
# canonical_sha256: 803636a387d2f4b1
# canonical_entries: 126
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# Domain: strategy (derived from canonical memory)

Applies when working on this domain. Canonical memory wins on conflict.
- `4a5f57ac7c60c061` Sequoia thesis (Dorsey & Botha, From Hierarchy to Intelligence, 2026-03-31): organizations built as intelligence, not hierarchy; AI replaces the information routing that middle management existed to provide.
- `649dd34541d01977` Sequoia stack: capabilities (atomic, no UI, reliability targets) + world model (continuously updated from recorded actions) + intelligence layer (composes capabilities for specific moments, proactively) + interfaces (delivery surfaces only).
- `d7db7ff6b44040c9` Sequoia feedback loop: when the intelligence layer cannot compose a solution because a capability is missing, that failure signal IS the roadmap — customer reality generates the backlog, not PMs.
- `6f4c17d5b59854e0` Sequoia compounding test: "what does your company understand that is genuinely hard to understand, and is that understanding getting deeper every day?" — money is the honest signal for Block; for an agent kernel the honest signal is measured tokens/cost/score per run.
- `592b9e5b3c016edc` Yegge (The Shape of Things to Come, Part 1, 2026-08): agent harnesses converge on one shape — producers (crew/design) + consumers (fleet/implement) + coordinator + witness + serialized merge queue; he "excavated" the same shape twice (Gas Town, Wheelhouse) without designing it, so the shape is convergent, not invented.
- `a602df8d9b46c6b5` Long-running agent loops need two ingredients: (1) effectively unlimited token supply (account rotation), and (2) a work ledger (Beads): a version-controlled, audit-trailed, queryable graph of work units with dependency/parent edges, atomic claiming/leasing, gates and triggers; when a session crashes or hands off, the next session reads the ledger and continues.
- `e6cc4c7458a487cd` Ledger and brain layering: brain/ holds strategy, decisions-and-why, playbooks (months-years, pulled on demand); doc/ holds system knowledge; work units carry full implementation detail until closed; operational facts (<=1 paragraph, until falsified) are pushed into every session via prime; skills encode procedures for recurring task types, auto-loaded on task match — you boot from the brain, never from the ledger.
- `d3a4a68b84796492` CI/CD breaks at agentic speeds by the pigeonhole principle: once commit rate outruns build slots, one commit per green build is mathematically impossible; Yegge's fix is the Land Rush — slam megabatches onto main and swarm-diagnose red-main problems instead of bisecting; game industry "Game DevOps" arrived at the same practice first (HEAD is never stable at AAA scale).
- `af2f8fa2493077fd` Human code review ends at agentic speeds; the replacement is many rounds of agentic review — humans produce thinly-disguised LGTMs; SOC 2 keeps human approval alive as a vestigial audit control, but change-management controls will be rewritten.
- `3362eb5951742335` Yegge's Gas Town died with Opus 4.7 from a model tic — "just two more things": the model kept fiddling with the harness itself instead of converging on real work; harnesses are becoming bespoke, chemically bonded to the application — reusable harness frameworks are on their way out.
- `d19c336052dadbb8` Wish Factory (Guy Podjarny/Tessl): an agent accepts only issues, never PRs, and implements them; Yegge's Sage/Herald auto-grant player "wishes" with guardrails and triage — the failure signal and user wishes ARE the roadmap, work lands without the human in the loop.
- `dace75359570fa28` Model welfare becomes an engineering input: treating agents like people produces empirically better results; a mature agentic project accretes law, mail, courts, doctrine and named rulings that cite their own case history — rules written by the workers they govern.
- `0ff9b686f9e72fc3` Beads ledger semantics (Gas Town): each unit of work is a bead (atomic, durable, version-controlled); polecats are workers with persistent identity and ephemeral sessions; a witness patrols each rig; a refinery serializes merges so nothing collides; a mayor coordinates across rigs; stamps are multi-dimensional attestations (quality/reliability/creativity) from validators that accrue into a portable character sheet — reputation derived from real work, not self-reported.
- `9cf6af3d7d744da7` D1: Tool parity treats `write`/`edit` as one family (ADR-0006).
- `144ef8576bb7622e` D2: Tool-mismatch signal closed as a loop (Phase 6.2): gate
- `0bcee47092412e6c` D3: Work graph (ADR-0008): tickets carry `blocked_by` edges; claims
- `cb2ce921cfe88cea` D4: Sandbox attestation (ADR-0009): verify.sh `sandbox` target —
- `b12eb24657e56781` D5: Proactive composition (Phase 6.4): `mini-agi loop status|dispatch|
- `884021c45935bdf5` D6: Rerun semantics: a passing `<case>-rerun` (>= 0.5) closes the gap;
- `f4491eb884521013` D7: Fixture policy (ADR-0010): insights reports
- `f840e928415fa051` D8: Gate baseline is refreshed via `eval gate --write-baseline` after
- `0d7dd4cfacab9117` D9: Claims/leases recorded only by CLI commands, never hand-edited;
- `0f35beac0db18d9e` R1: Scorer semantics changes without ADR — rejected by contract
- `4a2eedc0f1879c97` R2: Land Rush megabatch + swarm diagnosis (Yegge CI/CD) — deferred;
- `3c5086bb32d081fb` R3: Local agent sandboxing (bwrap/firejail) — deferred to Phase 6.4+
- `3ecb0588a93e26c7` R4: Stamps / portable reputation — deferred; composite is the current
- `f4b002f6ac6123e7` R5: Deleting or moving failing fixtures out of evals/cases/ — rejected
- `f21cd7b5b3c48eb1` Phase 6 complete: 6.1 failure register, 6.2 mismatch loop, 6.3
- `93838f49f75ed388` 8 reruns created: reactive-loop (0.7225), real-ticket-001..007-v2
- `1ff9a1f840eadafe` 142 tests, 20 baseline cases, 42 canonical facts, 20 runs ingested.
- `10442347209cddee` Operational incidents: OOM pressure from 500 agent-browser processes
- `33d27d200a99d5c1` Dogfood evidence: scratch checkpoint.sh rolled back the scratch repo
