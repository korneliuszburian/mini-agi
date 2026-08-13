# PROVENANCE
# canonical_sha256: e689e353ca78a8e9
# canonical_entries: 132
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# Domain: mini-agi (derived from canonical memory)

Applies when working on this domain. Canonical memory wins on conflict.
- `1bf27bb3c381dc94` mini-agi does counterfactual audit at gate-authoring time: the kernel re-runs the declared verify_command in an empty directory before trusting it and requires exit != 0 there, and also requires PASS on the real known-good target — both directions checked in audit_verifier (crates/mini-agi-core/src/verifier.rs:176-239); the empty-directory counterfactual is wired into iteration (audit_verifier_vacuous, lines 246-285).
- `acb5e902643f2ed0` mini-agi's gate treats a silent target as a failure: 'every target must exit 0 AND produce output; a silent target is a failing target' (scripts/verify.sh:3-4), enforced in step() which fails if output is empty (scripts/gate-lib.sh:12).
- `352a278f5c64baa2` mini-agi executes verify_command via `sh -c` with current_dir set to verify_target (crates/mini-agi-core/src/verifier.rs:87-94), errors if verify_target is not a directory (lines 81-86), reports three-way status verified/disagrees/unverified so a verifier contradicting the run's own claim is surfaced (lines 137-148; ADR-0011), and treats a hung gate (>120s) as a disagreement rather than a pass (verifier.rs:24-26, 95-127).
