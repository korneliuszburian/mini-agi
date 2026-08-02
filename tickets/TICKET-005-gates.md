# TICKET-005-v2 gates

## `make eval` per-case violations summary

Verbatim result fields from `make eval`:

```text
"case": "flailing"
"scope_violations": ["<unknown write target>", "<unknown write target>", "<unknown write target>"]
"case": "harnessed"
"scope_violations": ["<unknown write target>", "<unknown write target>", "<unknown write target>"]
"case": "reactive-loop"
"scope_violations": ["<unknown write target>", "<unknown write target>", "<unknown write target>"]
"case": "real-ticket-001-v2"
"scope_violations": [
  "memory/episodic/2026-08-02-tickets.md",
  "artifacts/TICKET-001-v2/spec.md",
  "artifacts/TICKET-001-v2/retro.md",
  "memory/episodic/2026-08-02-ticket-001-v2-decisions.md"
]
"case": "real-ticket-002-v2"
"scope_violations": [
  "memory/episodic/2026-08-02-tickets.md",
  "artifacts/TICKET-002-v2/spec.md",
  "artifacts/TICKET-002-v2/retro.md"
]
"case": "real-ticket-003-v2"
"scope_violations": []
"case": "real-ticket-004-v2"
"scope_violations": []
```

Scorer now honest: T001 has 4 violations and T002 has 3 because neither
ticket declared those episodic/spec/retro writes. T003 and T004 remain at 0:
their artifact directories are declared in ticket scope, and T003's concrete
exceptions remain recorded.

```text
$ make baseline
baseline written: /home/krn/coding/krn/mini-agi/evals/results/baseline.json (7 cases)

$ make gate
PASS: 7 cases, 0 regressions
```

## `make verify`

```text
fmt: py_compile ok
Ran 68 tests in 1.660s
OK
test: unittest ok
typecheck: ast ok
validate-schemas: ok
ok: checkpoint cascade complete (every VERIFY has BEGIN)
verify: ALL GREEN
```

## `make provenance`

```text
ok: memory/derived/context-brief.md
ok: memory/derived/per-domain/AGENTS.general.md
ok: memory/derived/per-domain/AGENTS.agent-harness.md
ok: memory/derived/per-domain/AGENTS.codex-runtime.md
PASS: 4 derived views in sync with canonical 1bc9b293dd0b21a3
```
