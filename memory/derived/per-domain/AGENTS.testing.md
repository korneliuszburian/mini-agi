# PROVENANCE
# canonical_sha256: 51b38b96d3d65647
# canonical_entries: 131
# derived_at: regenerated deterministically by mini-agi derive
# rule: if this file's canonical_sha256 differs from `mini-agi provenance` output, re-run derive

# Domain: testing (derived from canonical memory)

Applies when working on this domain. Canonical memory wins on conflict.
- `1e1b3ca106a78e3b` The formal notion of vacuity in verification originates from temporal model checking: Beer, Ben-David, Eisner, and Rodeh formalize vacuity and interesting witnesses and detect them in temporal model checking ("Efficient Detection of Vacuity in Temporal Model Checking", FMSD 18(2):141-163, 2001, DOI 10.1023/A:1008779610539).
- `439ade46d55d5c15` mini-agi's kernel defines a declared verifier as VACUOUS if it passes on an empty target and refuses to trust it; its audit runs the verifier on the known-good verify_target (gold check) AND on an empty temp directory (counterfactual check), reporting PASS only when the gate accepts the real target and rejects the empty one (crates/mini-agi-core/src/verifier.rs:163-239).
- `6a7be71b250b61c6` Mutation testing is the canonical 'inject a fault and require the gate to detect it' technique: deliberately injected faults (mutants) must be killed by the test suite; surviving mutants mean tests failed to detect an injected fault (Jia & Harman, IEEE TSE 37(5):649-678, 2011).
- `fd74e35848d96223` mini-agi encodes the falsifier as its proof discipline: '1 focused falsifier for one changed runtime contract, migration, authority boundary, parser, or reproduced bug' — a test that fails when the changed behavior is wrong (AGENTS.md).
- `40120af48e7950b9` mini-agi's audit_verifier_vacuous refuses to trust a verifier it could not test against a counterfactual: if the empty-dir run cannot be set up or executed, the audit errors rather than trusting the gate (crates/mini-agi-core/src/verifier.rs:246-278).
- `94c7f8f5460c8d25` Bash `for` loop: 'If there are no items in the expansion of words, no commands are executed, and the return status is zero.' A verifier that loops over matched files silently passes with zero iterations (GNU Bash Reference Manual §3.2.5.1).
- `9ced6c2413b3c6c5` Bash pipeline exit status without pipefail is the exit status of the last command; with pipefail it is the last (rightmost) command to exit non-zero. `find ... | verify` hides a failing middle stage unless pipefail is on (Bash Reference Manual §3.2.3).
- `8559cdb96268dd29` Bash `set -e` (errexit) does not abort when a failing command is part of a while/until command list, part of an if test, part of a `&&`/`||` list (except the command following the final one), or any non-last pipeline command. A verifier relying on `set -e` alone can exit 0 despite a failed intermediate check (Bash Reference Manual §4.3.1).
- `5ddf0e1af53c0dda` GNU xargs: if standard input is completely empty, by default the command is run once even with no input; `-r`/`--no-run-if-empty` suppresses this. A gate built as `... | xargs verify` executes its verifier once on empty input (GNU Findutils manual §8.4.1).
- `c318ea3b6d3d57a6` GNU `find -exec command {} +` / `-execdir`: 'The result is always true' — find's exit status does not reflect whether the executed command succeeded; a gate ending in such a find is green whenever find runs (GNU Findutils manual §3.3.2).
- `d7094e95f846a4e7` GNU grep exit status: 0 if a line is selected, 1 if no lines selected, 2 on error — the anti-footgun: `grep -q` as the last stage fails on an empty match set, making 'assert something exists' gates non-vacuous (GNU Grep 3.12 manual §2.3).
- `fc88da0620bc3b99` pytest treats a zero-test run as an error: exit code 5 = 'No tests were collected' (pytest docs, Exit codes).
- `dfbedd918b8faaa3` Jest's `--passWithNoTests` flag allows the test suite to pass when no files are found. Inference (opinion): the flag exists because the default is to fail on a test run that finds nothing; opting in recreates the vacuous pass.
- `346fe871acd99e57` pytest-cov `--cov-fail-under MIN` fails if total coverage is less than MIN. Opinion: this only bounds quantity of execution, not whether executed code is the code under review; with an empty suite it is not a strong non-vacuity guarantee (a coverage threshold cannot detect that the wrong branch was checked out).
