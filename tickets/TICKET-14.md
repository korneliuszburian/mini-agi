# Ticket

- id: TICKET-14
- title: Enforce the skills-registry working-set cap (SKILLS_BUDGET_CHARS is a reporting bound, not a cap)
- goal (one sentence): Make the skills list a real 2% working set again by enforcing the declared `SKILLS_BUDGET_CHARS` in a ranked, budgeted listing and re-anchoring `budget` on it.
- domain: skills

## Evidence (measured 2026-08-09)

- `mini-agi budget` reports: `Skills list: 7831 chars for 17 skills (97.9% of 2% budget)` — 98% of the declared working-set budget with 17 skills; every new skill (`skill_add` is a normal repo flow) pushes the list toward and past 100%, where the report degrades to a bare WARN.
- `metrics.rs::budget()` (lines 152-179) sums the frontmatter chars of EVERY skill in `.agents/skills/<name>/SKILL.md` with no ranking and no truncation: `SKILLS_BUDGET_CHARS` is used only to compute a percentage and `skills_over_budget`. The "2% budget" is a measurement, not an enforced working set — the same dead-code pattern TICKET-13 found in `MAX_BRIEF_BYTES`.
- The audit (docs/STANDARDS-AUDIT.md, 2026-08-05) lists "skills budget 97.9%" as a gap; the research pass (research/which-mechanisms-for-continuous-self-improvement-in-llm-agent.md, Verdict + opinion 1) recommends an evidence-of-use-ranked, capped skills listing (Voyager skill-library findings, arXiv:2305.16291: retrieval-limited library that compounds).

## Root cause

`budget()` measures an unbounded registry. Unlike memory (`ranked_facts`/`select_budgeted`), the skills registry has no ranking machinery and no cap enforcement — agents see the full unbounded list.

## Fix scope

1. `skills.rs`: add `budgeted_list(root, cap_chars) -> (list, total, shown)` — rank deterministically (enabled first, verify-hooked second, then alphabetical), fill to `SKILLS_BUDGET_CHARS` (chars, matching the existing char-counting convention), and append a notice line when truncated (`... N more skills in .agents/skills/`); the result is strictly <= cap.
2. `metrics.rs::budget()`: derive `skills_list_bytes` from the budgeted list so the report prints `N/M skills (pct)` truthfully; the `skills_over_budget` WARN becomes unreachable (keep as an invariant in tests).
3. `main.rs cmd_budget` + `mcp.rs` budget tool: surface the shown/total counts.
4. Tests: cap respected (strict char assert), deterministic across calls, notice present when truncated, over-budget fixture.
5. CHANGELOG entry.

## Verification

- `cargo test` green (new cap tests + updated budget tests).
- Real corpus: `mini-agi budget` shows skills list <= `SKILLS_BUDGET_CHARS` regardless of registry growth.
- `scripts/verify.sh` ALL GREEN.

## Follow-up (not in this ticket)

Per-skill usage/evidence counters to rank by actual use (Voyager evidence-of-use, ADR-0015 §2c) — requires instrumentation of skill invocation.

## Closure evidence (2026-08-10, goal session)

- Implementation landed in master (commit `11c7fae` "dogfood: enforce the skills-list working-set cap (TICKET-14)"): `skills::budgeted_list` (ranked enabled/verify-hooked first, char-capped at `SKILLS_BUDGET_CHARS` with truncation notice), metrics and `skill_list` MCP surface the bounded list.
- Delta shipped today (`ticket14-report-shown`): `BudgetReport.skills_shown` added; `mini-agi budget` now prints `N/M skills` truthfully and notes truncated skills outside the working set.
- Measured: "Skills list: 7797 chars for 17/17 skills (97.5% of 2% budget)" — registry is currently fully within the cap; growth past the cap truncates with the notice.
- Tests: cap respected (strict char assert), deterministic ordering, notice present when truncated, over-budget fixture (skills.rs suite).

Status: CLOSED (evidence above).
