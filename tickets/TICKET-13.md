# Ticket

- id: TICKET-13
- title: Enforce the derived-brief working-set cap (MAX_BRIEF_BYTES is dead code)
- goal (one sentence): Make the context brief a real 8KiB working set again by enforcing the declared cap in render_brief and re-anchoring the budget metrics on it.
- domain: memory

## Evidence (measured 2026-08-09)

- `memory::MAX_BRIEF_BYTES = 8192` ("Derived brief size cap in bytes
  (context budget; `PoC`: 8192)") is declared at memory.rs:31 and used
  NOWHERE — the Phase 8 slice-6 rewrite of `render_brief` (fact
  linking) dropped the cap.
- `mini-agi budget`: canonical 402029B -> brief 477881B (x0.84 — the
  brief is LARGER than the canonical source; 58x the declared cap).
- The `leverage_ratio` metric only compares brief against canonical, so
  it cannot see the cap violation — it reports "sane" at 0.84 while the
  working set is 58x its design budget. Sessions following AGENTS.md
  ("read the brief before working") load ~120k tokens of context.

## Root cause

`render_brief` renders every fact body + link line into the brief
without any budget. The ranking machinery already exists
(`ranked_facts` — enforced(3) + link-degree(2) + recency, deterministic)
but is only used by the query path (`select_budgeted`), not by derive.

## Fix scope

1. `render_brief` ranks via `ranked_facts` and fills to
   `MAX_BRIEF_BYTES` (fact body + link line count toward the cap; a
   small notice reservation keeps the output strictly <= cap). Truncated
   facts get a notice line pointing at `memory/canonical/` (append-only
   source, nothing lost).
2. `derive` returns (written, total, fragments) so the CLI can print
   `derived: context-brief.md (N/M facts)` truthfully (main.rs
   derive_text).
3. `budget`: re-anchor on the cap — the "brief is larger than
   canonical" warning becomes "brief exceeds the 8KiB working-set cap";
   the budget test's leverage bound (0,5] (written for the uncapped
   dump) is replaced by `brief_bytes <= MAX_BRIEF_BYTES` + leverage > 0.
4. Tests: render_brief respects the cap (strict byte assert), enforced
   facts present when they fit, deterministic across calls; cli.rs
   derive string updated.
5. CHANGELOG entry.

## Verification

- `cargo test` green (new cap test + updated budget/derive tests).
- Real corpus: `mini-agi derive` -> `context-brief.md` <= 8192 bytes,
  `mini-agi budget` shows brief << canonical.
- `scripts/verify.sh` ALL GREEN (derive determinism gate covers the
  new output).

## Closure evidence (2026-08-10, goal session)

- Implementation landed in master (commit `c204658` "dogfood: enforce the brief working-set cap (TICKET-13)"): `render_brief` ranks via `ranked_facts` (enforced>links>recency) and fills to `MAX_BRIEF_BYTES` with a notice reservation; `derive` returns (written, total, fragments); budget re-anchored on the cap.
- Measured: derived brief = 7648 bytes <= 8192 cap; `budget` reports "canonical 448058B -> brief 7648B (x58.58)".
- Determinism gate: second derive does not change the brief (verify.sh derive step covers it).
- Tests: strict byte-cap assert + enforced-fact presence + determinism (memory.rs suite), cli derive string updated.

Status: CLOSED (evidence above).
