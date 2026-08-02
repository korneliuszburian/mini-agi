# TICKET-002-v2 gate output

## make verify

```text
fmt: py_compile ok
checkpoint b8ccb0f: clean-step (clean)
----------------------------------------------------------------------
Ran 51 tests in 0.848s

OK
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpcueskbhs/evals/cases/real-ticket-004/run.json
captured 1 steps, 123 tokens, cost ~$0.0004
wrote /tmp/tmpc5tkk5z1/evals/cases/real-ticket-004/run.json
captured 1 steps, 123 tokens, cost ~$0.0
wrote /tmp/tmpibu4h12m/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp9vj24cf1/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp99y8csue/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpgo_pteha/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp3m_was75/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpxhto35lg/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpbay9ys8w/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpoa9nr5dd/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpoh24r45w/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpw5cavqsf/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpe4n5osh9/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpvh1b9vi5/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmph3hv0v3p/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmptyik9ir6/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp17mtq9i5/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpq5ncxycl/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp4mwjirdn/evals/cases/real-ticket-004/run.json
FAIL: invalid case name: ../escape
test: unittest ok
typecheck: ast ok
skip non-JSON ticket: tickets/TICKET-001-gates.md
skip non-JSON ticket: tickets/TICKET-001.md
skip non-JSON ticket: tickets/TICKET-002.md
validate-schemas: ok
ok: checkpoint cascade complete (every VERIFY has BEGIN)
verify: ALL GREEN
```

## make provenance

```text
ok: memory/derived/context-brief.md
ok: memory/derived/per-domain/AGENTS.general.md
ok: memory/derived/per-domain/AGENTS.agent-harness.md
ok: memory/derived/per-domain/AGENTS.codex-runtime.md
PASS: 4 derived views in sync with canonical 1bc9b293dd0b21a3
```
