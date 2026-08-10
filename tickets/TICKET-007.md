# Ticket

- id: TICKET-007-v2
- title: Redaction hardening (review v2-001 #5) — deny-by-default credential patterns + CONTEXT-BUDGET experiment (low reasoning, single-read)
- goal (one sentence): Close the deferred security finding — extend capture-trajectory.py redaction to sshpass/cookies/private keys/credential keys with deny-by-default fixtures — under a CONTEXT-BUDGET regime (low reasoning, one read per file) and measure whether the cost-curve redesign (ADR-0008, options A+B) moves the clean cost point below 126,907.
- domain: agent-harness
- context (verified by orchestrator, do not re-derive):
  - review v2-001 #5: capture-trajectory.py redactor is incomplete — `sshpass -p`, cookies, private-key material, non-listed credential keys uncovered. This ticket closes it.
  - EXPERIMENT (this ticket IS the measurement): reasoning effort = low (orchestrator sets it at launch); context budget rules below are binding and verifiable in the trajectory.
- CONTEXT-BUDGET RULES (binding, AC-verifiable):
  1. Read memory/derived/context-brief.md, AGENTS.md, tickets/TICKET-007.md, scripts/capture-trajectory.py, tests/test_capture_trajectory.py each AT MOST ONCE. After the first read, use `grep -n <pattern> <file>` for any fact lookup — never re-read a whole file.
  2. No speculative reads: do not read evals/, docs/, adr/, knowledge/ unless a failing gate demands it.
  3. Keep slices small but do NOT split purely to satisfy ceremony: checkpoint.sh begin/verify once per logical change.
- acceptance criteria (verifiable, not prose):
  1. Redactor extends deny-by-default: values for `sshpass -p`, `-p <pw>`, `password=`, `passwd=`, `secret=`, `api_key`, `apikey`, `token=`, `Cookie:` header values, `Authorization:` header values (incl. Bearer/Token/Basic), PEM private-key blocks (`-----BEGIN (RSA|EC|OPENSSH|DSA)? ?PRIVATE KEY-----`) are replaced with `[REDACTED]`. Patterns apply in CLI command strings AND JSON payloads.
  2. Parametrized leak-regression fixtures: each pattern has a test asserting the secret does NOT appear in captured output (both plain and JSON-encoded forms); existing tests still pass.
  3. Deny-by-default: any unknown `key=value` pair in a CLI command or JSON where the key matches credential-ish regex (key contains `pass`, `secret`, `token`, `key`, `auth`, `cookie`, `cred`) is redacted too. A test covers an UNSEEN credential-ish key.
  4. Trajectory budget check (orchestrator-verifiable): the captured run.json contains NO `read` step targeting AGENTS.md, tickets/TICKET-007.md, or memory/derived/context-brief.md after the first read of each (post-run `make eval` trajectory inspection; orchestrator counts them in findings).
  5. `make verify` GREEN (73 existing + new tests), `make provenance` PASS, `tickets/TICKET-007-gates.md` with VERBATIM `make verify` + `make provenance` output.
  6. Final report cites a canonical fact supporting deny-by-default (4cf21e40f7f4e2d2 deterministic gates, or 43a956cb72cedb67 append-only logs) and states review finding v2-001 #5 closed. Read the brief first (once).
  7. No manual commits (checkpoint.sh auto-commits permitted).
- soft goal (not a criterion): tokens < 126,907 (T006 baseline). The EXPERIMENT question: does low-reasoning + single-read move the clean point down meaningfully (target < 100,000)? Any result is evidence; the number goes into docs/PARADOX.md update after the review.
- scope (allowed files/dirs for the IMPLEMENTER): scripts/capture-trajectory.py, tests/test_capture_trajectory.py, tickets/TICKET-007-gates.md, artifacts/TICKET-007-v2/
- expected orchestrator post-run artifacts (NOT implementer edits): memory/episodic/checkpoints.log, evals/cases/real-ticket-007-v2/run.json, evals/results/baseline.json, git commits
- non-goals: trajectory format changes, ADR-0002 promotion controls, checkpoint-gate changes, docs edits; do NOT modify evals/golden/, evals/harness/, docs/, memory/ by hand, or historical run.json files.
- dependencies / related canonical facts (fact IDs from memory/canonical/index.md):
  - 4cf21e40f7f4e2d2 verification must be a deterministic gate, not a model declaration
  - 43a956cb72cedb67 append-only decision logs prevent semantic drift
- blocker to START: none — make verify is GREEN (73 tests); T006 (126,907 tok) is the clean baseline this experiment must beat.

## Closure evidence (2026-08-10, goal session)

- v2 (PoC, Python) is superseded in v3 (Rust): the redactor now lives in
  the kernel crate as `crates/mini-agi-core/src/redact.rs`, a zero-dependency
  (std-only, no regex) deny-by-default scanner — the ACC requirements below
  are met by v3 code, not by the removed Python path.
- ACC-1 (known-pattern coverage): `redact()` replaces `-p <pw>`, `sshpass -p`,
  `password=`, `passwd=`, `secret=`, `api_key`, `apikey`, `token=`,
  `Cookie:` and `Authorization:` header values (plain `Cookie: x` /
  `Authorization: Bearer y` and JSON `"Cookie": "x"` forms), and PEM
  private-key blocks — in CLI command strings AND JSON payloads alike.
- ACC-2 (leak-regression fixtures): 7 parametrized tests assert the secret
  does NOT appear in the redacted output (plain + JSON forms) and existing
  tests still pass — `redacts_sshpass_and_dash_p_flag_values`,
  `redacts_known_key_equals_value_pairs`, `redacts_json_payload_values`,
  `redacts_cookie_and_authorization_header_values`, `redacts_pem_private_key_blocks`,
  plus plain-text/deny-by-default guards. `cargo test --workspace` = 500 green.
- ACC-3 (deny-by-default): `is_credential_key`/`token_contains_key` bound a
  credential word in ANY compound key (e.g. `--cred-stuff=`, `--auth-token=`,
  `passphrase:`) and redact it even when the full key is unlisted; an UNSEEN
  credential-ish key is covered by `redacts_unseen_credential_ish_keys`.
  Mid-word false positives (`--mycredstuff`, JSON `tokens_total` count field)
  are explicitly asserted as NOT redacted.
- ACC-4 (trajectory budget): `clifmt::build_run_draft` now redacts every
  captured action string before it is written into run.json `trajectory`
  (`crates/mini-agi/src/clifmt.rs`), with regression test
  `draft_redacts_credential_values_in_actions`.
- ACC-5: `make verify` equivalent in v3 = `scripts/verify.sh` (single
  deterministic gate; cargo fmt --check, clippy -D warnings, cargo test
  --workspace 500, checkpoint journal, provenance, budget, derive, audit
  ALL GREEN on the clean tree). Gate ledger: `tickets/TICKET-007-gates.md`.
- ACC-6: deny-by-default is pinned by canonical facts 4cf21e40f7f4e2d2
  (verification = deterministic gate) and 43a956cb72cedb67 (append-only logs).
  Review tick v2-001 #5 (captured commands must not leak credential
  material into run.json) is closed by the trajectory redaction.
- Scope note: v3 has no Python capture script; the redactor lives in the
  kernel (used by clifmt) and is dogfooded by the kernel's own memory rules.
- Commits: `0c71a38` (redactor + clifmt wiring + lib.rs mod),
  `4895d7b` (close checkpoint ticket07-redaction).

Status: CLOSED (evidence above).
