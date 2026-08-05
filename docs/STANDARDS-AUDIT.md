# Agentic-engineering standards audit (2026-08-05)

Audit of the skills/standards ecosystem, commissioned to answer: is our
agentic engineering top-tier at every stage and plane, and what would a
UNIVERSAL STANDARDS PACKAGE look like — plugging the mini-agi MCP
server into ANY repo brings all work standards with it?

## Skills inventory (34 registered)

- Repo-local (`.agents/skills/`, 15): wayfinder, to-spec, to-tickets,
  implement, tdd, verify, checkpoint, review, code-review,
  diagnosing-bugs, orchestrate, compact, handoff, ingest-knowledge,
  caveman.
- Global (`~/.agents/skills/`, 19 symlinks into krn-codex-skills):
  codebase-design, delivery-loop, domain-modeling, prototype,
  slice-work, source-to-decision, target-repo-work,
  setup-repository-workflow, typescript-engineering, writing-great-
  skills, managing-codex-capabilities, omarchy + the 7 overlapping
  copies of local skills. One DEAD symlink: `reviewer-handoff` (no
  SKILL.md).
- opencode-only: cube-css (UI styling).

## Plane coverage

Covered: research→decisions (source-to-decision, domain-modeling,
wayfinder), spec (to-spec), tickets (wayfinder/to-tickets/slice-work),
implementation+tdd (implement, tdd, codebase-design,
typescript-engineering), verification (verify, checkpoint — dogfood),
review (review, code-review), delivery (delivery-loop), diagnosis
(diagnosing-bugs), memory (compact, handoff, ingest-knowledge),
communication (caveman, compact), repo adoption (setup-repository-
workflow, target-repo-work), prototyping (prototype), meta
(writing-great-skills, managing-codex-capabilities).

GAPS: frontend/UI verification (cube-css styles but nothing verifies a
UI: visual regression, a11y, E2E), security review (rubric has the
dimension, no dedicated skill), product/design thinking, release
automation (doc only), performance diagnosis, per-language engineering
(only TypeScript), cross-repo skill-source sync.

OVERLAPS: review vs code-review (two generations of the same idea);
implement/to-spec/to-tickets/wayfinder/slice-work exist in BOTH
local and global (version-drift risk); delivery-loop vs orchestrate
near-twins.

## The universal standards package

ALREADY PORTABLE: `mini-agi init` embeds into ANY repo — AGENTS.md
skeleton (10 sections), review-rubric.md, scripts/verify.sh (portable:
cargo steps skip without Cargo.toml, kernel steps [skip] without the
kernel), checkpoint.sh, .codex/config.toml (full MCP registration +
HITL approval map), CLAUDE.md shim, opencode.json. The MCP surface
delivers the live standards: loop_status/memory_query/run_verify/
loop_verify/eval_gate/checkpoint_audit/ticket_*/budget/health (37
tools, 11 HITL-gated).

MISSING (prioritized):
1. repo-adoption skill — one-command adoption: init → install the 8
   generic skills → AGENTS fragment → first verify run.
2. frontend-verification skill — E2E/visual/a11y verification with the
   same deterministic-gate contract as verify.sh (fills the one plane
   gap; relevant to the human-review-gate rule for frontend work).
3. security-review skill — dependency audit + threat model + secret
   scan as a gate step (feeds the rubric's Security dimension).
4. AGENTS.md generator — per-stack templates (Rust/TS/web) instead of
   one skeleton; init --lang.
5. standards-version manifest — versioned package so init can diff and
   upgrade embedded files instead of skip-if-exists.

## Next

- AFK v4 (parallel-planner) ships first; then the standards-package
  goal: repo-adoption + frontend-verification skills + the human-review
  gate per domain (frontend = mandatory HITL — zero-trust rule).
