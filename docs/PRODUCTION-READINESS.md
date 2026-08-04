# mini-agi — Production Readiness Audit (2026-08-04)

Research-backed assessment of what a single-binary agent kernel needs to
be production-ready, grounded in this repo's current seams and in the
literature (Anthropic engineering, OWASP Agentic 2026, OpenAI governance,
semver.org, cargo-dist/cargo-deny/cargo-vet, reproducible-builds.org).
Unconfirmed items are marked. This is a companion to
`docs/HARDENING-AUDIT.md` (the hardening backlog is largely DONE; this
report opens the next layer).

---

## A. Executive summary

**Where the kernel already leads the field.** The literature is explicit
that LLM-judge-only evaluation is insufficient without calibration
(Anthropic, "Demystifying evals for AI agents"). mini-agi's
deterministic verifier + calibration corpus + judge-drift precision
(`verifier.rs`, `memory/derived/calibration.md`), its pass^k-style reruns,
audit-trail intent, checkpoint journal, and Landlock sandbox are all
ahead of the practices most shops run — no sourced kernel does all of
this.

**The four gaps that matter now** (each has a concrete seam below):

1. **No release pipeline** — no tags→gate→artifacts→signing→release
   flow, no `cargo-deny`, no `--version` beyond `CARGO_PKG_VERSION`.
2. **Budget caps are boundary-only, not loop-enforced.** Wall/step/cost
   caps exist at the worker and ingest seams (hardening slice 2); the
   loop itself does not carry per-run max-token/max-cost gates.
3. **The audit log is too narrow.** `verify.log` + journal cover
   verifier + checkpoints; the OWASP/OpenAI governance practice is a
   comprehensive, append-only, principal+hash action log for ALL kernel
   actions.
4. **No capability-vs-regression split** in the eval corpus — every case
   is gated the same way, so saturation/drift is invisible.

---

## B. Distribution & release engineering

### B.1 Semver policy (declare the surface)

The repo is `0.3.0`; SemVer FAQ: if it's in production use it should
probably be 1.0.0. For now keep 0.x but **declare the public surface**:
CLI flags/subcommands, MCP methods + JSON-RPC shape, `run.json`/eval
fixture format, `verify_command` contract. Then a declared-surface
change bumps MINOR (0.x minor = your "major"); PATCH for non-surface
bug fixes. `mini-agi-core` (a library) needs stricter API semver than
the binary. Document this in `docs/releases.md`.

### B.2 Release pipeline (Phase 1 — do now)

Tag-gated GitHub Actions workflow (ripgrep's `release.yml` is the
reference): `git tag vX.Y.Z` → job A runs `scripts/verify.sh` on the tag
commit + checks tag == Cargo.toml version → job B matrix-builds
`x86_64`/`aarch64-unknown-linux-musl` with `--release --locked` →
strip + tar.gz + per-artifact `.sha256` + unified `sha256.sum` →
`actions/attest-build-provenance` (Sigstore keyless, verifiable with
`gh attestation verify`) → GitHub Release (draft→publish). `cargo-dist`
generates ~90% of this + installers if wanted; the custom gate stays a
prerequisite job either way. **Caveat: musl + landlock needs a real test
build (unconfirmed — pure syscall wrapper, should build).**

### B.3 Supply chain (proportionate)

- **Do now:** `cargo-deny` in CI (advisories + licenses + duplicate-version
  bans) — single highest value/effort. One `deny.toml`.
- **Company-kernel tier:** `cargo-vet` with exemptions ratcheting down
  (Mozilla's audit-trail), CycloneDX SBOM attached to releases,
  cosign-with-own-key. **Solo tier: overkill.**
- Locked rustc (`rust-toolchain.toml`) + `--locked` builds are already
  right; add `--remap-path-prefix` for bit-reproducibility and never
  embed build timestamps.

### B.4 "Single binary" honesty

The binary needs `memory/` + `evals/` at runtime. Make the story true:
`--data-dir`/env override with XDG defaults, first-run auto-init of the
layout from an embedded seed skeleton, and ship the seed + README in
the release tarball.

### B.5 Version in the binary

`--version` already flows from `env!("CARGO_PKG_VERSION")` via clap.
Optional: a `build.rs` embedding `git describe` as `0.3.0+<sha>`
(timestamp-free). Keep a Keep-a-Changelog discipline (already the
practice).

---

## C. Production operations

### C.1 Capability vs regression evals

Label each case `mode: capability|regression` in `run.json`; apply a
strict gate to regression cases (~100% continuously) and a monitoring
gate to capability cases (hill-climbing). Add a **reference solution
per case** (a case whose rerun can't reproduce the reference fails the
audit) — seam: `evals/cases/<case>/` + `audit.rs`. Add a **trial-isolation
guard** so a rerun never reads a prior run's outputs (contamination is
Anthropic's documented failure) — seam: `worker.rs`/`capture.rs`. Watch
for **saturation** (100% = no signal) and **broken tasks** (0% at
pass^100 usually means a broken case, not a weak agent).

### C.2 Observability (do NOT buy a SaaS)

`run.json` is already a structured trace (step, tool, ok, goal_aligned,
tokens, output_tokens). Promote it to a **versioned trace schema**: add a
header with `kernel_version`, `n_steps`, `n_toolcalls`, latency. Publish
the four signals that matter (tokens/run, tool-call count, model choice,
success rate) into `docs/METRICS.md` (already kernel-owned). OTel GenAI
conventions are "Development" status — name ops with their vocabulary
(plan/execute_tool/invoke_agent) for future mechanical migration, don't
adopt OTel now.

### C.3 Eval drift & judge calibration

`judge-drift` is the concrete realization of Anthropic's
"calibrate model graders against deterministic ground truth" — keep it,
and make **disagreement a recalibration trigger** (when drift exceeds a
threshold, flag `calibration.md` for refresh). Seam: `verifier.rs`.

---

## D. Governance

### D.1 Comprehensive audit log

Widen the audit from verify-commands to ALL kernel actions: run, verify,
checkpoint begin/pass/fail, harness swap, memory append — append-only,
timestamped, principal + content hash. Seam: extend `audit.rs`, new
`memory/episodic/actions.log` beside `verify.log`, hook `journal.rs`.

### D.2 Least-authority sandbox

OWASP Agentic 2026's top risk is "excessive agency". Extend Landlock
(ADR-0012) from one policy to **per-skill / per-tool-class** read-only vs
write rules, configured in `.miniagi.json`. Seam: `sandbox.rs`.

### D.3 OWASP mapping ADR

Write an ADR mapping each OWASP Agentic Top 10 2026 risk to a concrete
mini-agi control (or an explicit "not applicable") — a defensible,
auditable taxonomy. Seam: `docs/adr/`.

### D.4 HITL approval gate

Promote `scripts/hitl-loop.template.sh` from a diagnostic script to a
first-class approval gate for risky tool classes, logged to the audit
log. Seam: `worker.rs` permission check.

---

## E. Reliability

- **Hard per-run budget gates in the loop** (max steps / tokens / cost):
  wall/step/cost caps already exist at the worker + ingest seams; make
  the loop carry them so a dispatched run declares and obeys its budget.
  Seam: `loopcmd.rs` + `.miniagi.json`.
- **Resume-from-checkpoint** for interrupted runs (Anthropic: durable
  execution, resume not restart): persist the last committed step
  incrementally. Seam: `journal.rs` + `capture.rs`.
- **Idempotency keys** for lease claim / memory append (content-hash
  dedup already exists via fact ids). Seam: `ticket.rs`, `memory.rs`.
- **Retention policy** for journal/checkpoints/verify.log (LangGraph's
  unbounded-growth warning). Seam: `journal.rs` + `checkpoint.sh`.
- **Let the worker see tool failure and adapt** — already done
  (failures.md → slice specs); keep it as the default over blind
  backoff.

---

## F. Memory & state (where the kernel is furthest along)

The append-only canonical + fact ids + provenance + derived views is
already event-sourcing-shaped. Extend it:

1. **Explicit snapshot/replay**: a `snapshot` + `replay` pair proving a
   derived view is the deterministic materialization of the canonical
   log (the provenance fingerprint already verifies it). Seam:
   `memory.rs` + `derive`.
2. **Enforce the two-tier split in paths**: thread-scoped working state
   (`memory/episodic/`) vs durable facts (`memory/canonical/`) —
   LangGraph's checkpointer/store distinction. Mostly there.
3. **Version the fact format** (schema version in the canonical header /
   fact id) so replay stays deterministic across kernel upgrades.
4. **Mine outcomes, not trajectories** (Anthropic: grade outcomes, not
   paths): keep `mismatch.rs` as a soft/debug signal, never a hard gate.

---

## G. Backlog (mapped to seams)

### P0 — safety + loop stability
1. Release pipeline + `cargo-deny` + sha256/attestations (B.2/B.3).
2. Hard per-run budget gates in the loop (E).
3. Comprehensive audit log (D.1).

### P1 — quality / predictability / DX
4. Capability-vs-regression case labels + per-mode gates (C.1).
5. Reference solutions + trial-isolation guard (C.1).
6. `run.json` versioned trace header + `kernel_version` (C.2, F.3).
7. Per-skill least-authority sandbox (D.2).
8. Data-dir contract + first-run auto-init (B.4).
9. judge-drift recalibration trigger (C.3).

### P2 — nice to have
10. snapshot/replay pair (F.1).
11. OWASP-mapping ADR (D.3), HITL approval gate (D.4).
12. Resume-from-checkpoint + retention policy (E).
13. crates.io publish, Nix flake, Homebrew tap (company tier).

### Deferred / rejected (named)
- AI-observability SaaS (LangSmith/Langfuse/Arize/Braintrust) — the
  `run.json` trace is the observability; a SaaS adds a network service
  to a std-only single binary.
- OTel GenAI conventions today — "Development" status, will churn.
- CRDT agent-memory — no evidence it is a production pattern.
- `cargo-vet` / SBOM / own cosign keys for a solo project — company tier.
- Path-grading as a hard gate — conflicts with "grade outcomes, not
  paths"; `mismatch.rs` stays soft.

## Implementation status (2026-08-04)
P0/P1 backlog implemented slice by slice (each verify ALL GREEN + pushed
+ CI green):
- **B.2/B.3 supply chain + release pipeline — DONE.** `deny.toml` +
  `cargo deny check advisories licenses bans` in CI (plain shell step —
  the action container conflicts with the pinned toolchain on musl);
  tag-gated `.github/workflows/release.yml` (gate prerequisite, tag==
  Cargo.toml version, musl+glibc matrix --release --locked, sha256 +
  attest-build-provenance, GitHub Release draft). aarch64 musl deferred
  (needs cross-rs; landlock on musl unconfirmed).
- **D.1 comprehensive audit log — DONE.** `audit::append_action`
  appends `<utc> <principal> <action> <content-hash> <detail>` rows to
  `memory/episodic/actions.log` at the loop-verify / run-ingest /
  run-verify seams; the audit validates row shape.
- **E hard per-run budget gates — DONE.** Config `max_tokens`
  (+`max_cost_usd`); the ticket spec declares the caps; `loop verify`
  BLOCKS close on a breach even when composite+verifier+gate pass.
- **C.2/F.3 run.json trace header — DONE.** `kernel_version`, `n_steps`,
  `n_toolcalls`, `latency_seconds` on `eval::Run` (serde defaults —
  legacy runs parse unchanged), stamped by the capture draft.
- **B.4 data-dir contract — DONE.** `AGENTIC_ROOT` root override +
  `init::bootstrap` first-run skeleton auto-init (no files, no clobber);
  documented in README.

### Implementation status (run 2, 2026-08-04)
P1 backlog delivered slice by slice (each verify ALL GREEN + pushed +
CI green):
- **C.1 capability/regression labels + per-mode gates — DONE.** run.json
  `mode: capability|regression` (default regression); `eval gate` treats
  a capability-case composite drop as a monitored CAPABILITY DROP (not a
  hard fail) while a regression-case drop stays a hard failure.
- **C.1 reference solutions + trial-isolation guard — DONE.** References
  under `evals/references/<case>.json` (bootstrapped from the verified
  reruns); the audit flags a case with a rerun lacking a matching
  reference (missing, or composite/achieved divergence beyond 0.05) and
  flags a rerun whose trajectory reads a sibling case dir (trial
  contamination). Current corpus: 11 match, 0 missing, 0 contaminated.
- **D.2 per-skill least-authority sandbox — DONE.** Skills declare
  `sandbox: read-only` frontmatter; the worker routes through the
  Landlock wrapper with NO workdir write access for a read-only spec
  (only codex's own state dir stays writable).
- **C.3 judge-drift recalibration trigger — DONE.** Config
  `min_judge_precision` (default 1.0); when verifier-vs-judged precision
  drops below it the audit warns AND appends a dated note to
  `memory/episodic/calibration-trigger.log`. Current corpus stays 1.000.
- **F.1 derive snapshot/replay — DONE.** `derive --snapshot <name>`
  records canonical+brief hashes; `derive --replay <name>` regenerates
  and reports MATCH / DIVERGENT (deterministic materialization proof).
  Live: snapshot pre-migration -> replay MATCH.

### Remaining backlog (honest)
- P1: reference solutions per case + trial-isolation guard; capability/
  regression labels + per-mode gates; per-skill least-authority Landlock;
  judge-drift recalibration trigger. All real, all need the eval-corpus
  work that follows.
- P2: snapshot/replay pair, OWASP-mapping ADR, HITL approval gate,
  resume-from-checkpoint + retention, crates.io/Nix/Homebrew.
- Rejected (named in section G): observability SaaS, OTel now, CRDT
  agent-memory, cargo-vet/SBOM/own cosign for solo, path-grading as a
  hard gate.

## Status
Grounded in the current worktree and the fetched sources. Unconfirmed
items are marked. Implementation follows the same slice discipline as
the hardening backlog.
