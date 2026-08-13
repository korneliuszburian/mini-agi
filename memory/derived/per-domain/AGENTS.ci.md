# PROVENANCE
# canonical_sha256: e0be821c4adcd362
# canonical_entries: 120
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# Domain: ci (derived from canonical memory)

Applies when working on this domain. Canonical memory wins on conflict.
- `72a7827c7f713a00` GitHub Actions runs `run:` steps on Linux/macOS with default shell `bash -e {0}` and `shell: bash` as `bash --noprofile --norc -eo pipefail {0}`; fail-fast is enforced via `set -e` and `-o pipefail`; a container run step defaults to `sh`, not bash (GitHub Actions Workflow syntax).
