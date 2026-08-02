# ADR-0002: Skills live in `.agents/skills/` (standard discovery), not `skills/`

Status: accepted (2026-08-02)

## Context

Phase 0 plan placed product skills under `skills/` at repo root. Codex
(and Claude, Cursor, opencode, Windsurf, ...) scan per-project discovery
paths: `./.agents/skills/`, `../.agents/skills/`, `$REPO_ROOT/.agents/skills/`
(first match wins), then user `~/.agents/skills/`, admin `/etc/codex/skills/`
(Agent Skills open standard; developers.openai.com/codex/skills). A plain
`skills/` directory is NEVER scanned — skills there would be dead.

## Decision

1. Project-scoped skills live in `.agents/skills/<name>/SKILL.md` (+ resources).
2. `skills/` at root is removed. Phase 2 `mini-agi skill add` installs into
   `.agents/skills/`; `mini-agi skill install --global` targets `~/.agents/skills/`.
3. Global dirs remain frozen (`~/.agents/skills.disabled`): no leakage into
   unrelated projects (per-project default, ADR-0011 v2 spirit).
4. `AGENTS.md` references (.agents/checks/review-rubric.md,
   .agents/skills/caveman) are now satisfied by ports of the PoC originals.

## Consequences

- Skills are immediately usable by any Agent Skills client without symlinks.
- `mini-agi` gets a single source dir to manage/verify from.
