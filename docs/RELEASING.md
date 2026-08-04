# Releasing mini-agi

Production-readiness (docs/PRODUCTION-READINESS.md, section B). The
release path is the tag-gated pipeline in `.github/workflows/release.yml`
(gate → version check → musl+glibc matrix → sha256 + attestations →
GitHub Release draft); crates.io is a supplementary channel.

## Publish checklist (crates.io)

1. Bump the workspace version in `Cargo.toml` (both crates, kept in
   lockstep) + fill the `[Unreleased]` → `vX.Y.Z` section of
   `CHANGELOG.md`.
2. `cargo publish --dry-run -p mini-agi-core` — must pass (metadata,
   license, README). A `--allow-dirty` refusal means uncommitted
   changes: commit first.
3. **Publish `mini-agi-core` FIRST** — `mini-agi` depends on it with a
   version requirement; the binary's dry-run cannot resolve it until the
   core crate is live on crates.io (`no matching package named
   mini-agi-core found` is the expected symptom before step 3).
4. `cargo publish -p mini-agi-core`
5. `cargo publish -p mini-agi`
6. Tag the matching commit `vX.Y.Z` and push the tag — the release
   pipeline builds the static artifacts with checksums + attestations.

## Artifact channel

- `git tag vX.Y.Z && git push origin vX.Y.Z` triggers release.yml:
  verify.sh gate on the tag → tag==Cargo.toml version check →
  `x86_64-unknown-linux-musl` + `-gnu` `--release --locked` → tar.gz +
  per-artifact sha256 + `sha256.sum` → `attest-build-provenance` →
  GitHub Release draft (publish manually after review).
- aarch64 musl is a follow-up (needs cross-rs; landlock-on-musl build
  unconfirmed).
- Nix: `flake.nix` builds from source with the pinned 1.97.1 toolchain
  (`nix build .#default`); not yet CI-tested (nix absent locally).

## Single-binary data-dir contract

The released binary expects `AGENTIC_ROOT` (or the CWD) to hold
`memory/` + `evals/`; on first use in an empty dir it bootstraps the
skeleton (`mini-agi init` for the full scaffold). See README "Data-dir
contract".
