## Findings

All claims below cite the primary source directly. Labels: **[fact]** = stated in the cited primary source; **[opinion]** = recommendation/rationale in the primary source (official guidance, not measurement); **[estimate]** = judgment I am adding.

### 1. Compiler lints and warnings-deny

- **[fact]** rustc runs lints during compilation and they can produce a warning, an error, or nothing. Warn-by-default lints exist because the code "might be a bug" without being wrong. (rustc book, "Lints" — https://doc.rust-lang.org/rustc/lints/index.html)
- **[fact]** Lint levels are `allow`, `expect`, `warn`, `force-warn`, `deny`, `forbid`, configurable via `-A/-W/-D/-F` flags, attributes, or `--cap-lints`; `forbid` cannot be overridden downward, and `--force-warn` takes precedence over everything. (rustc book, "Lint Levels" — https://doc.rust-lang.org/rustc/lints/levels.html)
- **[fact]** The `warnings` group covers all lints at warn level; `#[deny(warnings)]` lifts every warning to an error. The Reference's own example shows `unsafe_code` is `allow` by default, so it is only caught after a `#[warn(unsafe_code)]` is raised into the `warnings` group. (Rust Reference, "Diagnostic attributes" — https://doc.rust-lang.org/reference/attributes/diagnostics.html)
- **[fact]** `#[expect(C)]` suppresses a lint but emits `unfulfilled_lint_expectations` if the lint does not fire — a mechanism that turns stale suppressions into errors. (Rust Reference, "Diagnostic attributes"; rustc book, "Lint Levels")
- **[fact]** "Future-incompatible" lints give a warned transition period before behavior becomes a hard error (policy in RFC 1589). (rustc book, "Lints")
- **[fact]** Cargo compiles dependencies with `--cap-lints allow` so dependency warnings do not pollute build output; `force-warn` lints are exempt from this cap. (rustc book, "Lint Levels")
- **[fact]** Official CI pattern: keep a project "warnings clean" on CI via `build.warnings = "deny"` (config) or `CARGO_BUILD_WARNINGS=deny` (env), then run `cargo clippy --all-targets --all-features --keep-going`. (Cargo Book, "Continuous Integration" — https://doc.rust-lang.org/cargo/guide/continuous-integration.html; Clippy README — https://raw.githubusercontent.com/rust-lang/rust-clippy/master/README.md)
- **[fact]** Official caveat: "CI can fail due to new toolchain versions because there are limited compatibility guarantees around warnings. Consider pinning the toolchain version with an automated job that creates a PR to upgrade the toolchain on new releases." (Cargo Book, "Continuous Integration")
- **[fact]** Since Cargo 1.97, `CARGO_BUILD_WARNINGS` is preferred over `-D warnings` because it does not invalidate build caches. (Clippy README)

### 2. Clippy

- **[fact]** Clippy is "a collection of lints to catch common mistakes"; >800 lints, organized by category with default levels: `correctness` = deny, `suspicious`/`style`/`complexity`/`perf` = warn (together `clippy::all`), while `pedantic`, `restriction`, `nursery`, `cargo` are allow by default. (Clippy README — https://raw.githubusercontent.com/rust-lang/rust-clippy/master/README.md)
- **[fact]** Official warning: `restriction` "should, *emphatically*, not be enabled as a whole" — it may lint perfectly reasonable code and lints may contradict each other; use case-by-case (e.g. `unwrap_used` to "prevent panicking in certain functions" on CI). (Clippy README)
- **[fact]** `pedantic` "contains some very aggressive lints prone to false positives." (Clippy README)
- **[fact]** CI integration is documented by the project itself: `rustup component add clippy`; `CARGO_BUILD_WARNINGS=deny cargo clippy --all-targets --all-features` fails the build on any clippy or rustc warning; `cargo clippy --fix` auto-applies suggestions. (Clippy README)

### 3. rustfmt

- **[fact]** rustfmt formats "Rust code according to style guidelines"; its default style "conforms to the Rust style guide that has been formalized through the style RFC process" (RFC 3338). (rustfmt README — https://raw.githubusercontent.com/rust-lang/rustfmt/master/README.md; links to https://doc.rust-lang.org/nightly/style-guide/)
- **[fact]** CI gate: `cargo fmt --all -- --check` exits non-zero if formatting changes would be made; the project documents this exact setup for CI. (rustfmt README)
- **[fact]** Post-1.0 formatting stability guarantees exist for whole programs, with listed carve-outs (macros, comments, non-ASCII, fragments). (rustfmt README)
- **[fact]** `edition` and `style_edition` config; `rustfmt.toml` options are split into stable and nightly-only. (rustfmt README)

### 4. Unsafe audit

- **[fact]** Unsafe grants exactly five capabilities (deref raw pointer, call unsafe fn, access/alter mutable statics, implement unsafe trait, access union fields); it does not disable the borrow checker. (The Rust Book, ch. 20 "Unsafe Rust" — https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html)
- **[opinion]** Official guidance: "Keep `unsafe` blocks small"; wrap unsafe code in a safe abstraction "prevents uses of `unsafe` from leaking out"; `SAFETY:` comments on unsafe fns and blocks are idiomatic. (The Rust Book, ch. 20)
- **[fact]** Since Rust 2024, `unsafe_op_in_unsafe_fn` warns by default — unsafe ops inside an `unsafe fn` need an explicit `unsafe {}` block. The rationale (RFC 2585): the `unsafe fn` keyword had two roles and the second (implicitly allowing unsafe ops in the body) was "determined to be too risky without explicit unsafe blocks." (Edition Guide — https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html)
- **[fact]** Miri is an official dynamic UB checker; it only flags UB in code paths actually executed ("you will need to use it in conjunction with good testing techniques") and "does not cover every possible way your code can be unsound." The book demonstrates it catching a real dangling-pointer UB (Listing 20-7). (The Rust Book, ch. 20)
- **[fact]** The official unsafe reference is The Rustonomicon. (The Rust Book, ch. 20, reference)

### 5. Panic / error-handling discipline

- **[opinion]** Book guidance: returning `Result` is the default for fallible functions; `panic!` for unrecoverable "bad state" / violated contracts; `expect`/`unwrap` appropriate in tests, prototypes, examples, and where the programmer has more information than the compiler; encode invalid states in types (e.g. a `Guess::new` newtype) so invalid values can't compile. (The Rust Book, "To panic! or Not to panic!" — https://doc.rust-lang.org/book/ch09-03-to-panic-or-not-to-panic.html)
- **[fact]** The panic-vs-Result choice is codifiable as lints: clippy `restriction` lints such as `unwrap_used` exist specifically to "prevent panicking in certain functions" on CI. (Clippy README)

### 6. API and trait design

- **[fact]** The Rust API Guidelines are authored "largely by the Rust library team" and are explicitly "only guidelines… not… a mandate"; they cover naming, interoperability, type safety, predictability, flexibility, dependability, debuggability, and future-proofing (including sealed traits, C-SEALED). (Rust API Guidelines — https://rust-lang.github.io/api-guidelines/)
- **[opinion]** The same source claims crates that conform "integrate better with the existing crate ecosystem" — a claim of benefit stated without measurement in that document. (Rust API Guidelines)
- **[fact]** Semver consequences of API design are catalogued in the Cargo Book: adding a non-defaulted trait item, changing trait signatures, tightening generic bounds, and adding enum variants without `non_exhaustive` are Major; adding a defaulted trait item or defaulted type parameter are Minor or "Possibly-breaking"; the guide cites sealed traits and `#[non_exhaustive]` as mitigation strategies. (Cargo Book, "SemVer Compatibility" — https://doc.rust-lang.org/cargo/reference/semver.html)
- **[fact]** The SemVer chapter opens by saying the rules "are only *guidelines*, and not necessarily hard-and-fast rules that all projects will obey." (Cargo Book, "SemVer Compatibility")

### 7. Unit / property / fuzz testing

- **[fact]** `cargo test` runs unit tests in `src`, integration tests in `tests/`, compiles examples, and runs doctests. (Cargo Book, "Tests" — https://doc.rust-lang.org/cargo/guide/tests.html)
- **[fact]** Proptest (QuickCheck family) defines property testing as checking properties over automatically generated inputs, with automatic shrinking to a minimal failing case; the project positions it as a complement to, not replacement for, hand-written unit tests. (Proptest docs — https://proptest-rs.github.io/proptest/intro.html)
- **[fact]** cargo-fuzz wraps libFuzzer, requires a nightly compiler, and provides `fuzz run`, input minification (`tmin`/`cmin`), and coverage; the project maintains a "trophy case" of real bugs found by fuzzing. (cargo-fuzz README — https://raw.githubusercontent.com/rust-fuzz/cargo-fuzz/master/README.md; https://github.com/rust-fuzz/trophy-case)
- **[opinion]** Fuzzing-efficacy is supported only by anecdata (the trophy-case list) in the sources I reached; no controlled measurement is cited there.

### 8. no_std vs std-only

- **[fact]** `#![no_std]` links `core` instead of `std`; `core` is a "platform-agnostic subset" with no libstd runtime. Trade-offs per the official table: no heap without the `alloc` crate, no stack-overflow protection, no init code before `main`, and no OS abstractions. (Embedded Rust Book, "A no_std Rust Environment" — https://docs.rust-embedded.org/book/intro/no-std.html; RFC 1184)
- **[fact]** Semver-wise, "switching from `no_std` support to requiring `std`" is classified as a **Major** change — i.e. making a library std-only is a breaking release. (Cargo Book, "SemVer Compatibility")

### 9. Semver and toolchain pinning

- **[fact]** The Cargo Book is the canonical classification of compatible/breaking changes (major / minor / possibly-breaking), and defines that 0.y.z bumps treat y as major; it explicitly depends on maintainer judgment for "Possibly-breaking" items. (Cargo Book, "SemVer Compatibility")
- **[fact]** cargo-semver-checks lints API diffs against the Cargo SemVer reference, supports deny/warn/allow lint levels and required-update (major/minor), has defined exit codes (0 clean, 100 violations, 101 error), and targets "not to have false positives." Its own FAQ states it does not catch every violation (e.g. breaking type changes, generics, subset-of-features breakage). (cargo-semver-checks README — https://raw.githubusercontent.com/obi1kenobi/cargo-semver-checks/main/README.md)
- **[fact]** `rust-toolchain.toml` pins channel, components, and targets per-project and "is suitable to check in to source control"; the rustup book notes that if the toolchain is pinned to a specific release, `Cargo.lock` "should probably be tracked too." (rustup book, "Overrides" — https://rust-lang.github.io/rustup/overrides.html)
- **[fact]** Cargo's official CI chapter shows testing across stable/beta/nightly channels, verifying the `rust-version` field with `cargo-hack`/`cargo-msrv`, and a separate "latest dependencies" job (with `continue-on-error`) to keep dependency ranges honest. (Cargo Book, "Continuous Integration")

### 10. CI gate design

- **[fact]** The Cargo Book's CI chapter is the closest thing to an official CI gate spec: build+test on a channel matrix; warnings gate via `CARGO_BUILD_WARNINGS=deny`; a "latest deps" job marked continue-on-error; a rust-version verification job; and explicit trade-off notes (exhaustiveness vs turnaround, CI cost, toolchain-upgrade churn). (Cargo Book, "Continuous Integration")
- **[fact]** The fmt and clippy gates are documented by those projects themselves (rustfmt `--check`; clippy with `CARGO_BUILD_WARNINGS=deny --all-targets --all-features`). Neither the Cargo Book nor clippy/rustfmt READMEs present measurements of these gates' effect on defect rates. (rustfmt README; Clippy README)

### Empirical support vs expert opinion

- **[fact]** The mechanisms (lint levels, `cap-lints`, `expect`, future-incompatible warnings, Miri, cargo-fuzz tooling) are fully specified in primary documentation.
- **[opinion]** Every *practice* above — warnings-deny in CI, small unsafe blocks, SAFETY comments, `Result` over `panic`, sealed traits, property-testing as a complement, toolchain pinning, the CI channel matrix — appears in primary sources as expert/official guidance supported by stated rationale, not by controlled or comparative measurement. None of the sources I reached cites a study measuring, e.g., whether `deny(warnings)` reduces shipped defects or whether clippy pedantic reduces bug density.
- The closest things to evidence in the reached sources are (a) Miri demonstrably catching a concrete UB in the Book's worked example, and (b) the cargo-fuzz trophy-case list of real-world bugs found — both are existence proofs, not efficacy comparisons.

## Sources

Primary documents fetched and quoted from (all first-party, retrieved 2026-08-07):

1. The Rust Book, ch. 20 "Unsafe Rust" — https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html
2. The Rust Book, ch. 9 "To panic! or Not to panic!" — https://doc.rust-lang.org/book/ch09-03-to-panic-or-not-to-panic.html
3. The Rust Reference, "Diagnostic attributes" — https://doc.rust-lang.org/reference/attributes/diagnostics.html
4. The rustc book, "Lints" — https://doc.rust-lang.org/rustc/lints/index.html
5. The rustc book, "Lint Levels" — https://doc.rust-lang.org/rustc/lints/levels.html
6. The Rust Edition Guide, "unsafe_op_in_unsafe_fn warning" — https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html
7. The Cargo Book, "Continuous Integration" — https://doc.rust-lang.org/cargo/guide/continuous-integration.html
8. The Cargo Book, "Tests" — https://doc.rust-lang.org/cargo/guide/tests.html
9. The Cargo Book, "SemVer Compatibility" — https://doc.rust-lang.org/cargo/reference/semver.html (page is long; the classification list, no_std→std major, "only guidelines" disclaimer, and sealed-trait/non_exhaustive mitigations were verified in the fetched portion)
10. The rustup book, "Overrides" — https://rust-lang.github.io/rustup/overrides.html
11. The Embedded Rust Book, "A no_std Rust Environment" — https://docs.rust-embedded.org/book/intro/no-std.html (cites RFC 1184 — https://github.com/rust-lang/rfcs/blob/master/text/1184-stabilize-no_std.md)
12. Rust API Guidelines — https://rust-lang.github.io/api-guidelines/
13. Clippy README (rust-lang/rust-clippy) — https://raw.githubusercontent.com/rust-lang/rust-clippy/master/README.md
14. rustfmt README (rust-lang/rustfmt) — https://raw.githubusercontent.com/rust-lang/rustfmt/master/README.md
15. Proptest docs — https://proptest-rs.github.io/proptest/intro.html
16. cargo-fuzz README (rust-fuzz/cargo-fuzz) — https://raw.githubusercontent.com/rust-fuzz/cargo-fuzz/master/README.md
17. cargo-semver-checks README (obi1kenobi/cargo-semver-checks) — https://raw.githubusercontent.com/obi1kenobi/cargo-semver-checks/main/README.md

All were readable via text fetch; no PDFs were involved.

## Verdict

**Established (fact, verified in primary sources):** Rust has a fully specified lint system (`allow/expect/warn/force-warn/deny/forbid`, `--cap-lints`, `warnings` group, `expect`/`unfulfilled_lint_expectations`, future-incompatible lints per RFC 1589). Cargo officially documents a warnings-deny CI gate (`build.warnings`/`CARGO_BUILD_WARNINGS`, preferred over `-D warnings` since Cargo 1.97 for cache reasons), the explicit risk that toolchain upgrades can break warnings-deny CI (pinning recommended), a channel-matrix + rust-version + latest-deps CI design, the SemVer compatibility classification (including no_std→std = Major), `rust-toolchain.toml` pinning, and no_std semantics. Clippy (800+ lints, category defaults, `restriction` not-en-block), rustfmt (`--check` gate), Miri (dynamic UB checking, only executed paths), cargo-fuzz (libFuzzer wrapper, nightly), and cargo-semver-checks (API diff vs Cargo SemVer reference) all document their own capabilities and CI usage.

**Uncertain:** Whether these practices measurably improve code quality. Every behavioral recommendation is expert/official guidance backed by rationale, not by empirical studies, in all sources I reached. Fuzzing's "trophy case" and Miri's worked UB catch are existence proofs only. I found no primary source that measures the defect-reducing effect of warnings-deny, clippy categories, unsafe-block scoping, `Result`-over-panic, property-testing, or toolchain pinning.

**What would settle it:** Controlled or large-scale observational studies — e.g. lint fires/`deny`-CI adoption vs post-release defect or vulnerability rates, fuzzing coverage contribution per project, unsafe-block size and bug rate correlation, or semver-check adoption vs downstream breakage. No such measurements were present in the primary sources I reached; this gap is itself the main finding for the "empirical vs expert" part of the question.
