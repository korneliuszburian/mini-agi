# TICKET-006-v2 gate evidence

## `make verify`

```text
fmt: py_compile ok
checkpoint 90ff1b8: clean-step (clean)
checkpoint 018e5b4: clean-green (clean)
----------------------------------------------------------------------
Ran 72 tests in 1.822s

OK
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpfwfdg91z/evals/cases/real-ticket-004/run.json
captured 1 steps, 123 tokens, cost ~$0.0004
wrote /tmp/tmpc1f0qjxm/evals/cases/real-ticket-004/run.json
captured 1 steps, 123 tokens, cost ~$0.0
wrote /tmp/tmps48jxkr_/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp_zzhl_6t/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpwnkx0vms/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp9dktfwwj/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpqw3tk6c7/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpvbxox2bs/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpnkgayuj0/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpi85eqn29/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp2c_ev59o/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpzyzqownu/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpbxscde3q/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpi10paqhm/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpb2gn60ns/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpt5tiev_c/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp8v74ught/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpg_vbtba0/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpnlcsvggd/evals/cases/real-ticket-004/run.json
FAIL: invalid case name: ../escape
test: unittest ok
typecheck: ast ok
skip non-JSON ticket: tickets/REVIEW-001-v2.md
skip non-JSON ticket: tickets/REVIEW-003-v2.md
skip non-JSON ticket: tickets/TICKET-001-gates.md
skip non-JSON ticket: tickets/TICKET-001.md
skip non-JSON ticket: tickets/TICKET-002-gates.md
skip non-JSON ticket: tickets/TICKET-002.md
skip non-JSON ticket: tickets/TICKET-003-gates.md
skip non-JSON ticket: tickets/TICKET-003.md
skip non-JSON ticket: tickets/TICKET-004-gates.md
skip non-JSON ticket: tickets/TICKET-004.md
skip non-JSON ticket: tickets/TICKET-005-gates.md
skip non-JSON ticket: tickets/TICKET-005.md
skip non-JSON ticket: tickets/TICKET-006.md
validate-schemas: ok
ok: checkpoint cascade complete (every VERIFY has BEGIN)
verify: ALL GREEN
```

Exit: 0

## `make provenance`

```text
ok: memory/derived/context-brief.md
ok: memory/derived/per-domain/AGENTS.general.md
ok: memory/derived/per-domain/AGENTS.agent-harness.md
ok: memory/derived/per-domain/AGENTS.codex-runtime.md
PASS: 4 derived views in sync with canonical 1bc9b293dd0b21a3
```

Exit: 0
