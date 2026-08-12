# PROVENANCE
# canonical_sha256: b07609823a7d5c16
# canonical_entries: 105
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# Domain: verification (derived from canonical memory)

Applies when working on this domain. Canonical memory wins on conflict.
- `72ab6c3e92661d05` SLSA v1.0 verifying-artifacts prescribes verifying the provenance signature, checking the statement's `subject` matches the digest of the artifact in question, looking up builder identity against a configured root of trust, and comparing buildType/externalParameters/canonical source repo against expectations — 'Any unrecognized externalParameters SHOULD cause verification to fail' (slsa.dev/spec/v1.0/verifying-artifacts).
