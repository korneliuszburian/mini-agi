## Findings

**1. Identity of the package**
- `mcp-hub` is an npm package (author "Ravitemer"), MIT-licensed, with repository `git+https://github.com/ravitemer/mcp-hub.git`. Current `latest` dist-tag is **4.2.1**. Every published version in the registry is a plain `x.y.z` (no prerelease tags). Sources: npm registry metadata (`https://registry.npmjs.org/mcp-hub`), `package.json` at `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/package.json`.

**2. The declared version policy (statement of intent)**
- The project's `CHANGELOG.md` states, verbatim: "The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)." This is the only explicit, published version-policy statement I found. Source: `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/CHANGELOG.md` (header, first two lines). *Fact.*

**3. Mechanical enforcement of the policy**
- `package.json` defines `release:patch`, `release:minor`, `release:major` scripts, all invoking `bash scripts/release.sh patch|minor|major`.
- `scripts/release.sh` (primary source, fetched) enforces:
  - must run on `main` with a clean working tree;
  - performs the semver bump via `npm --no-git-tag-version version $VERSION_TYPE` (i.e., npm's patch/minor/major bump);
  - **gates the release** on a matching `## [new-version]` entry existing in `CHANGELOG.md` (aborts and reverts otherwise);
  - creates an annotated git tag `vX.Y.Z`, a single `chore(release): vX.Y.Z` commit, `npm publish`, and a GitHub release.
  Sources: `package.json` scripts section and `scripts/release.sh` at `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/scripts/release.sh`. *Fact.*

**4. Adherence in practice (observed behavior, consistent with the declared policy)**
- The registry version sequence (62 versions from `1.0.0` to `4.2.1`) and the changelog entries line up with semver semantics: breaking changes were released as majors — `2.0.0` (endpoints moved from URL params to `server_name` body), `3.0.0` (client registration endpoints removed), `4.0.0` (marketplace API schema/endpoint migration) — each with a `### Breaking Changes` section; features landed as minors and bug fixes as patches. Sources: npm registry version list + `CHANGELOG.md` sections `[2.0.0]`, `[3.0.0]`, `[4.0.0]`. *Fact* that practice matches the declared policy.
- `CONTRIBUTING.md` instructs PR authors to "Update the CHANGELOG.md with notes on your changes" as part of the pull-request process — the mechanism that feeds the changelog gate above. Source: `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/CONTRIBUTING.md`. *Fact.*

**5. What the policy does NOT specify (absent from the sources I reached)**
- No documented support/maintenance policy for older major versions (no LTS window, no deprecation/end-of-life statement for past majors).
- No documented policy for prereleases, release candidates, or nightly builds (and no such versions have been published — the registry shows only stable `x.y.z`).
- No stated minimum-version-support promise for Node beyond the README requirement `Node.js >= 18.0.0` (README.md, "Requirements"). *Estimate: the absence of a support-window policy is itself a finding, not a claim about what the policy is.*

## Sources
- npm registry metadata for `mcp-hub` (latest `4.2.1`, full version list, repo URL, description): `https://registry.npmjs.org/mcp-hub`
- `package.json` (version, scripts `release:patch/minor/major`, repository, license): `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/package.json`
- `CHANGELOG.md` (declaration of Keep a Changelog + Semantic Versioning; breaking-change sections): `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/CHANGELOG.md`
- `scripts/release.sh` (release-gate mechanics): `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/scripts/release.sh`
- `CONTRIBUTING.md` (changelog-update requirement in PR process): `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/CONTRIBUTING.md`
- `README.md` (Node >= 18 requirement): `https://raw.githubusercontent.com/ravitemer/mcp-hub/main/README.md`
- GitHub repo page (license, topics, structure): `https://github.com/ravitemer/mcp-hub`
- PyPI lookup returned 404 for `mcp-hub` (`https://pypi.org/pypi/mcp-hub/json`) — no Python distribution of this name found; the package in question is the npm one.

## Verdict
**Established:** The published version policy of the `mcp-hub` npm package is **Semantic Versioning 2.0.0** (with Keep-a-Changelog formatted changelog), declared in the repo's `CHANGELOG.md` and mechanically enforced by `scripts/release.sh` (npm semver bump, mandatory changelog entry for the new version, `vX.Y.Z` git tag, then npm/GitHub publish). All 62 published versions are stable `x.y.z` and the major-version bumps in the changelog match breaking-change releases, confirming the policy in practice.

**Uncertain:** There is no published policy for support windows of older majors, deprecation timelines, or prerelease/nightly distributions — the sources I reached simply document none. The same applies to minimum Node-version maintenance; only the runtime requirement `Node.js >= 18.0.0` is stated, not a support commitment.

**What would settle it:** Maintainer-authored documentation (e.g., a `SUPPORT.md`, a "versioning" section in README/CONTRIBUTING, or GitHub Discussions statements) covering LTS/deprecation and prerelease practices — none of which currently exists in the repo.
