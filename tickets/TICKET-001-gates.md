# TICKET-001-v2 gate evidence

## Retry 1 — `make verify`

```text
fmt: py_compile ok
checkpoint c2b9774: clean-step (clean)
----------------------------------------------------------------------
Ran 45 tests in 0.491s

OK
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpyy3bz8wo/evals/cases/real-ticket-004/run.json
captured 1 steps, 123 tokens, cost ~$0.0004
wrote /tmp/tmpv3ae59rx/evals/cases/real-ticket-004/run.json
captured 1 steps, 123 tokens, cost ~$0.0
wrote /tmp/tmp5eq6l20f/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpm9fz6fsh/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp2z7u3lcl/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmppogzfijk/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpoicf57sn/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpw8sp_4zj/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpj445evi8/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpoyiigsyj/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpd8sfz_a2/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpw1fyywo4/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpmbapd1k6/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp2eio2xxw/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp65bp_4yv/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpnylnhvqp/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp9kn1w1o1/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmphduhordo/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpzt22pmb6/evals/cases/real-ticket-004/run.json
FAIL: invalid case name: ../escape
test: unittest ok
typecheck: ast ok
skip non-JSON ticket: tickets/TICKET-001-gates.md
skip non-JSON ticket: tickets/TICKET-001.md
validate-schemas: ok
ok: checkpoint cascade complete (every VERIFY has BEGIN)
verify: ALL GREEN
```

## Retry 1 — `make provenance`

```text
ok: memory/derived/context-brief.md
ok: memory/derived/per-domain/AGENTS.agent-harness.md
ok: memory/derived/per-domain/AGENTS.codex-runtime.md
PASS: 3 derived views in sync with canonical 4e68633ae7662752
```

## `make verify`

```text
fmt: py_compile ok
checkpoint 54e78c4: clean-step (clean)
----------------------------------------------------------------------
Ran 44 tests in 0.359s

OK
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpzgas4gn9/evals/cases/real-ticket-004/run.json
captured 1 steps, 123 tokens, cost ~$0.0004
wrote /tmp/tmpu4dw0a2i/evals/cases/real-ticket-004/run.json
captured 1 steps, 123 tokens, cost ~$0.0
wrote /tmp/tmp7s1uqtnk/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpqbyagmnw/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpp0r_wj9a/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpkny2ysi_/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpwog3d4sr/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmptg6bj0mg/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpsdxybsji/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpali9a2v1/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpnjp59t5f/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpdn9qj5l7/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp3_tbh8kx/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpksd3be2d/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmprtf4oe4b/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpszk7l0t6/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmp2mghmwqp/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpsp0hakl9/evals/cases/real-ticket-004/run.json
captured 1 steps, 0 tokens, cost ~$0.0
wrote /tmp/tmpddux8xel/evals/cases/real-ticket-004/run.json
FAIL: invalid case name: ../escape
test: unittest ok
typecheck: ast ok
skip non-JSON ticket: tickets/TICKET-001.md
validate-schemas: ok
ok: checkpoint cascade complete (every VERIFY has BEGIN)
verify: ALL GREEN
```

## `make provenance`

```text
ok: memory/derived/context-brief.md
ok: memory/derived/per-domain/AGENTS.agent-harness.md
ok: memory/derived/per-domain/AGENTS.codex-runtime.md
PASS: 3 derived views in sync with canonical 4e68633ae7662752
```
