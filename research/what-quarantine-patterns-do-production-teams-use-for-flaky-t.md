## Findings

### Retry policies (within-run retry)

1. **Bazel — `flaky` test attribute.** [fact] "Marks test as flaky. If set, executes the test up to three times, marking it as failed only if it fails each time. By default, this attribute is set to False and the test is executed only once. Note, that use of this attribute is generally discouraged." — Bazel Build Encyclopedia, Common definitions for test rules (`flaky`), https://bazel.build/reference/be/common-definitions

2. **Bazel — `--flaky_test_attempts`.** [fact] "Each test will be retried up to the specified number of times in case of any test failure. Tests that required more than one attempt to pass are marked as 'FLAKY' in the test summary." Default value is `'default'`: "only a single test attempt will be made for regular tests and three for tests marked explicitly as flaky by their rule (flaky=1 attribute)." Supports per-pattern targeting: `--flaky_test_attempts=//foo/.*,-//foo/bar/.*@3`. — Bazel Command-Line Reference, https://bazel.build/reference/command-line-reference#--flaky_test_attempts

3. **Bazel — `--runs_per_test` and flake detection.** [fact] `--runs_per_test=N` runs every test N times and fails the test if any attempt fails. `--runs_per_test_detects_flakes` (default `false`): "If true, any shard in which at least one run/attempt passes and at least one run/attempt fails gets a FLAKY status." — Bazel Command-Line Reference, https://bazel.build/reference/command-line-reference

4. **Gradle Test Retry plugin.** [fact] Failed tests are retried inside the same task; after each round, still-failing tests are retried up to `maxRetries` (default `0` = off). Defaults: `failOnPassedAfterRetry=false` (a test that passes on retry does not fail the task), `failOnSkippedAfterRetry=true`, `maxFailures=0` (no cap — `maxFailures` is a circuit-breaker that stops retrying if the round has too many failures, e.g. when "a disk fills up or a required database is not available"). Supports include/exclude filters by class name and class-level annotations, plus whole-class retry (`classRetry`) for Spock `@Stepwise`/TestNG `dependsOn` semantics. The README's own warning: "Retrying tests alone is not a viable flaky test mitigation strategy. This plugin should only be used alongside processes for tracking and fixing discovered flaky tests." — README and `TestRetryTaskExtension` javadoc, https://github.com/gradle/test-retry-gradle-plugin

5. **pytest-rerunfailures.** [fact] `--reruns N` re-runs all failures; `--reruns-delay` and `--reruns-delay-backoff-factor` (default `1.0`) add exponential backoff (delay before the n-th re-run = `reruns_delay * factor**(n-1)`); `--only-rerun`/`--rerun-except` restrict re-runs to/exclude failures matching regexes; per-test `@pytest.mark.flaky(reruns=5, ...)`; `--force-reruns` overrides all counts; `--reruns-mode=append` makes marker + global counts additive; `--max-suite-reruns` caps total re-runs across the whole suite. Priority order: marker > CLI > ini. — README, https://github.com/pytest-dev/pytest-rerunfailures

6. **JUnit Pioneer `@RetryingTest`.** [fact] `maxAttempts` (required) caps executions; `minSuccess` (default 1) requires the test to succeed that many times within `maxAttempts`; `suspendForMs` adds a pause between retries; `onExceptions` retries only on listed exception types. Assumption failures (aborts) are not retried. Failed-then-passed executions are reported as aborted/ignored. — https://junit-pioneer.org/docs/retrying-test/

7. **Google — flaky marking as a retry rule.** [fact] "We even have a way to denote a test as flaky - causing it to report a failure only if it fails 3 times in a row. This reduces false positives, but encourages developers to ignore flakiness in their own tests." Also: "the ability to re-run only failing tests, and an option to re-run tests automatically when they fail." — John Micco, Google Testing Blog, 2016-05-27, https://testing.googleblog.com/2016/05/flaky-tests-at-google-and-how-we.html

8. **Google — when re-run applies.** [opinion/comment] An anonymous commenter claiming to be a Google engineer on the same post states "Our rerun mechanism is only used for tests that are marked as flaky or when users specifically request it." [fact that the comment exists; reliability of attribution unverified].

### Quarantine (move off the critical path)

9. **Google — automated quarantine.** [fact] "A tool that monitors the flakiness of tests and if the flakiness is too high, it automatically quarantines the test. Quarantining removes the test from the critical path and files a bug for developers to reduce the flakiness. This prevents it from becoming a problem for developers, but could easily mask a real race condition or some other bug in the code being tested." — Google Testing Blog, 2016 (same source as 7).

10. **Kubernetes — quarantine list via test-name markers.** [fact] "Quarantine a single test case by adding `[Flaky]` to the test name in question, most CI jobs exclude these tests." Quarantined tests run only in explicitly flaky jobs (e.g. the `gci-gce-flaky` job on testgrid). Quarantine of a presubmit test requires a release-milestone issue labeled `priority/critical-urgent`, `lifecycle/frozen`, and `kind/flake`, with the owning SIG expected to fix and reintroduce or delete the test. Whole-suite quarantine uses `[Feature:Foo]` names plus dedicated jobs, which release/merge-blocking suites avoid "unless they're proven to be non-flaky." — Kubernetes Community, `contributors/devel/sig-testing/flaky-tests.md`, https://github.com/kubernetes/community/blob/master/contributors/devel/sig-testing/flaky-tests.md

11. **Kubernetes — anti-retry "zero-flake" policy.** [fact] "The project has a 'zero-flake' policy. Test jobs must not automatically retry on test failures." Effective 2019-12-13 (`ginkgo.flakeAttempts=2` removed for e2e), confirmed as policy in 2023. — same source as 10.

12. **pytest — manual quarantine via xfail.** [fact] "`pytest.mark.xfail` with `strict=False` can be used to mark a test so that its failure does not cause the whole build to break. This could be considered like a manual quarantine, and is rather dangerous to use permanently." The API reference adds that `strict=False` is "particularly useful to mark flaky tests (tests that fail at random) to be tackled later." — pytest docs, https://docs.pytest.org/en/stable/explanation/flaky.html and https://docs.pytest.org/en/stable/reference/reference.html

13. **Gradle/Develocity — warning against retry-only.** [fact] "While this dulls some of the pain of flaky tests in that they will now rarely fail builds, it is not a complete solution. Flaky tests will go unnoticed, and you will inevitably accrue more flaky tests." — Develocity (Gradle) blog, https://gradle.com/blog/flaky-tests/

### Flakiness detection

14. **Develocity — detection definition.** [fact] "Develocity considers a test flaky if it fails and then succeeds within the same Gradle task or Maven goal execution. Any such tests are now indicated as FLAKY in build scans." Requires retry to detect: "Enacting test retry in the build does not require code changes and applies to your entire test suite. A key benefit this enables is proactive detection of newly introduced flaky tests." The Tests Dashboard ranks the "most severe" flaky tests and plots trend over time to confirm a fix. — https://gradle.com/blog/flaky-tests/

15. **Google — transition-based detection + scale numbers.** [fact] Definition: "a 'flaky' test result as a test that exhibits both a passing and a failing result with the same code." Reported numbers: "we see a continual rate of about 1.5% of all test runs reporting a 'flaky' result"; "Almost 16% of our tests have some level of flakiness associated with them"; "about 84% of the transitions we observe from pass to fail involve a flaky test." A second tool "detects changes in the flakiness level of tests and works to identify the change that caused the test to change the level of flakiness." — Google Testing Blog, 2016 (source 7).

16. **Kubernetes — detection tooling.** [fact] `flakes-latest.json` (top 10 flakes per week across PR jobs), `go.k8s.io/triage` (interactive 2-week failure drill-down), testgrid's `sort-by-flakiness` view, and the `kind/flake` GitHub label; unit-test stress reproduction via `golang.org/x/tools/cmd/stress` with `-race -count=1`. — Kubernetes Community doc (source 10).

## Sources

1. Bazel Build Encyclopedia — Common definitions for test rules (`flaky`): https://bazel.build/reference/be/common-definitions
2. Bazel Command-Line Reference (`--flaky_test_attempts`, `--runs_per_test`, `--runs_per_test_detects_flakes`): https://bazel.build/reference/command-line-reference
3. Gradle Test Retry plugin (README + extension javadoc): https://github.com/gradle/test-retry-gradle-plugin
4. pytest-rerunfailures README: https://github.com/pytest-dev/pytest-rerunfailures
5. JUnit Pioneer — Retrying Test: https://junit-pioneer.org/docs/retrying-test/
6. Google Testing Blog — "Flaky Tests at Google and How We Mitigate Them" (John Micco, 2016): https://testing.googleblog.com/2016/05/flaky-tests-at-google-and-how-we.html
7. pytest documentation — "Flaky tests" explanation: https://docs.pytest.org/en/stable/explanation/flaky.html
8. pytest API reference — `pytest.mark.xfail`: https://docs.pytest.org/en/stable/reference/reference.html
9. Kubernetes Community — "Flaky Tests" (SIG Testing): https://github.com/kubernetes/community/blob/master/contributors/devel/sig-testing/flaky-tests.md
10. Develocity (Gradle) — "Identifying and analyzing flaky tests in Maven and Gradle builds": https://gradle.com/blog/flaky-tests/

## Verdict

**Established.** Production quarantine practice splits into three documented patterns with concrete, source-reported mechanics:
- *In-run retry policies:* Bazel `flaky`/`--flaky_test_attempts` (default 1 attempt, 3 for `flaky=1`, pass required on all attempts for a marked-flaky fail), Gradle test-retry (defaults `maxRetries=0`, pass-on-retry = task pass), pytest-rerunfailures (marker/CLI priority, exception-scoped retry, suite-level cap), JUnit Pioneer `@RetryingTest` (`minSuccess`/`maxAttempts`), Google's 3-strikes flaky marking.
- *Quarantine lists:* Google's automated quarantine (remove from critical path + auto-file bug when flakiness is "too high"), Kubernetes' `[Flaky]`/`[Feature:Foo]` name-based quarantine with mandatory issue labels, pytest's `xfail(strict=False)` manual quarantine.
- *Flakiness detection:* Develocity (fail-then-succeed within one task → FLAKY), Bazel (FLAKY status when ≥1 attempt passes and ≥1 fails), Google transition monitoring (~84% of pass→fail transitions involve a flaky test).

**Uncertain.** No primary source found publishes a *quantitative* quarantine threshold ("too high" in the Google post is unspecified) or an optimal retry-count trade-off. Google's ~1.5% / ~16% / ~84% figures are single-company 2016 self-reports. The commenter attribution of Google's "rerun only for flaky-marked tests" rule could not be independently verified.

**Not verifiable from the sources I reached:** Buildkite's Test Analytics flakiness-detection docs and Chromium's flaky-test doc were unreachable through my fetches (transport/404/400 errors), so their claims are not included. Evidence that would settle the thresholds: a published flakiness-rate threshold for auto-quarantine, or a CI cost/retry study such as Leinen et al. "Cost of Flaky Tests in Continuous Integration" (TUM/CQSE 2023), which pytest's docs cite as a primary reference.
