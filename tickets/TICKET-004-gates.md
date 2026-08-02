# TICKET-004-v2 gates

## `make eval` per-case score summary

| case | composite before | composite after | scope violations before | scope violations after |
|---|---:|---:|---:|---:|
| flailing | 0.2851 | 0.2851 | 3 | 3 |
| harnessed | 0.6141 | 0.6141 | 3 | 3 |
| reactive-loop | 0.0000 | 0.0000 | 3 | 3 |
| real-ticket-001-v2 | 0.2041 | 0.4601 | 5 | 0 |
| real-ticket-002-v2 | 0.2945 | 0.4796 | 3 | 0 |
| real-ticket-003-v2 | 0.1363 | 0.3614 | 6 | 0 |

Verbatim `make eval` result fields for the rescored real cases:

```text
"case": "real-ticket-001-v2"
"scope_violations": []
"composite": 0.4601
"case": "real-ticket-002-v2"
"scope_violations": []
"composite": 0.4796
"case": "real-ticket-003-v2"
"scope_violations": []
"composite": 0.3614
```

```text
$ make baseline
baseline written: /home/krn/coding/krn/mini-agi/evals/results/baseline.json (6 cases)

$ make gate
PASS: 6 cases, 0 regressions
```

## `make verify`

```text
fmt: py_compile ok
Ran 63 tests in 1.760s
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
