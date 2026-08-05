# Skills research (standards-polish S5)

Deep research on the skill layer: design defects, routing, dogfood
seams, top-tier patterns, and a prioritized roadmap. Base:
docs/STANDARDS-AUDIT.md, docs/INCIDENTS.md, the SKILL.md files, and
docs/RESEARCH-2026-08.md (Matt Pocock / Sandcastle / industry).

## Design defects (evidence-first)

- D1: verify hooks exist but are never gated — verify.sh has no skills
  target; hooks run only via `skill verify <name>` one at a time.
  3/15 local skills carry hooks; the rest are docs with frontmatter.
- D2: completion criteria are unverifiable self-reports — implement's
  "tests red before green" has no quoted output; to-spec has no quality
  gate. Contrast verify/review, which demand quoted output.
- D3: the core implementation skill (implement) is 26 lines of prose
  with no procedure, thin for its role; orchestrate's stages have no
  handoff contract.
- D4: duplication — review vs code-review (two generations), 7 skills
  dual-registered local+global (silent drift), dead reviewer-handoff
  symlink, delivery-loop vs orchestrate near-twins.
- D5: no versioning/provenance in frontmatter; install_skills does
  remove_dir_all + blind copy.
- D6: composition is prose ("use /tdd"), not a contract — a rename
  breaks nothing detectably.
- D7: `disable-model-invocation: true` on wayfinder/to-spec/to-tickets/
  implement conflicts with the GLOBAL routing contract's phrasing
  (~/.claude/CLAUDE.md "Workflow routing" — the repo AGENTS.md has no
  routing table) — the routing is a HUMAN contract, and that ambiguity
  is undocumented.

## Routing

The global routing contract (~/.claude/CLAUDE.md) is a flat 19-owner
list — good principle, incomplete tree: missing leaves (quick answer,
skill authoring, repo adoption), the wayfinder→to-spec→slice-work→
to-tickets pipeline is presented as parallel choices with overlapping
boundary tests, and orchestrate (the pipeline driver) is not routed.
No routing telemetry exists (the kernel records runs, not skills), so
"most-invoked" claims are unsupported until telemetry lands.

## Dogfood seam (state vs prose — currently inverted in two places)

- The kernel OWNS tickets (claim/lock/graph) yet wayfinder and
  to-tickets re-implement claims and blocking in prose — the skills
  should call `ticket_claim` / `ticket_validate_graph`.
- The verify skill correctly defers to scripts/verify.sh (shared
  artifact). verify.sh should run ALL skill hooks (D1 fix).

## Top-tier patterns we lack

Skill-as-contract is a stub (3/15 have hooks; 12 are prompts). No
skill tests (writing-great-skills prescribes forward-trials + a prompt
matrix; nothing runs them). No composition/sandboxing (the `sandbox`
frontmatter is parsed but unused). No evaluation loop (evals cover
runs, not skills; no routing telemetry). No versioning (Voyager's
versioned cards, RHI's harness-as-data have no skill-side counterpart).

## Roadmap (next 2-3 goals)

- Goal A (small/high): `skill verify --all` wired into verify.sh (D1);
  dedup — delete reviewer-handoff, single owners for review/code-review
  and delivery-loop/orchestrate, content-hash check for dual-registered
  skills; rewrite implement/SKILL.md as a procedure with per-step
  Done-when; artifact-based criteria for to-spec.
- Goal B (medium): kernel seams — skill attribution in run ingest,
  wayfinder/to-tickets call the ticket tools instead of prose; new
  skills: repo-adoption, security-review (frontend-verification is
  DEFERRED by the user).
- Goal C (medium-large): routing as a decision tree with the missing
  leaves + the escalation chain; prompt-matrix forward-trials for the
  core skills; routing telemetry; skill-hook failures as eval-gate
  input.

## Definition of done (skill layer)

A reviewer finds no space when: (1) every procedural skill has a
verify hook that RUNS in the deterministic gate; (2) every completion
criterion is auditable against an artifact (no self-reports); (3)
every skill has exactly one owner (no dual registration, no dead
symlinks); (4) every routed skill has a tested prompt matrix; (5)
skills carry version+provenance and install/init can diff; (6) kernel
state (tickets, verdicts, journal) is the substrate skills call, never
re-implemented in prose; (7) routing telemetry exists and is reviewed
in the skill audit.
