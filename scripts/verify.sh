#!/bin/sh
# verify.sh — the deterministic gate of mini-agi.
# Sensor contract (PoC Makefile semantics): every target must exit 0 AND
# produce output; a silent target is a failing target.
#
# Portable by design (`mini-agi init` runs this in ANY repo):
#   - cargo targets run only when the repo is a Cargo workspace
#   - kernel steps use the local debug build, else `mini-agi` from PATH;
#     without either they report [skip] (a fresh repo has no kernel yet)
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export PATH="$HOME/.cargo/bin:$PATH"
# Cycle-34 finding (Clippy README): prefer CARGO_BUILD_WARNINGS=deny over
# RUSTFLAGS="-D warnings" — the RUSTFLAGS form invalidates the build cache
# on every change and, since Cargo 1.97, the env form is the documented
# warnings-deny CI gate. `-D warnings` is still passed to clippy directly.
export CARGO_BUILD_WARNINGS="deny"

fail=0

. scripts/gate-lib.sh

BIN="./target/debug/mini-agi"
has_cargo=0
if [ -f Cargo.toml ]; then
    has_cargo=1
    step "build"         cargo build || fail=1
    step "fmt-check"    sh -c 'cargo fmt --check && echo "fmt-check: clean"' || fail=1
    step "clippy"       cargo clippy --all-targets --all-features -- -D warnings || fail=1
    step "tests"        cargo test --all --all-features || fail=1
    # Coverage gate (D2): line coverage is measured, not assumed. The
    # step is portable — it skips when cargo-llvm-cov (or a system
    # llvm-cov) is unavailable, and FAILS when total line coverage drops
    # below the floor. The gate needs `LLVM_COV`/`LLVM_PROFDATA` env
    # (system install) or the `llvm-tools-preview` rustup component.
    step "coverage" sh -c '
        if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
            echo "coverage: skipped (cargo-llvm-cov not installed)"
            exit 0
        fi
        export LLVM_COV="${LLVM_COV:-$(command -v llvm-cov 2>/dev/null || true)}"
        export LLVM_PROFDATA="${LLVM_PROFDATA:-$(command -v llvm-profdata 2>/dev/null || true)}"
        summary=$(cargo llvm-cov --workspace --summary-only 2>&1) || { echo "$summary"; exit 1; }
        # TOTAL row: last column is the line-coverage percent.
        pct=$(printf "%s\n" "$summary" | awk "/^TOTAL/{print \$NF}" | tr -d "%")
        [ -n "$pct" ] || { echo "coverage: no TOTAL row in summary"; printf "%s\n" "$summary" | tail -5; exit 1; }
        if [ "$pct" -lt 80 ]; then
            echo "coverage: FAIL — total line coverage ${pct}% below the 80% floor"
            exit 1
        fi
        echo "coverage: total line coverage ${pct}% (floor 80%)"
    ' || fail=1
    if [ -n "$BIN" ] && [ -x "$BIN" ]; then
        step "skills"       "$BIN" skill verify-all || fail=1
    else
        skip "skills" "no kernel binary"
    fi
else
    skip "build" "no Cargo.toml (not a Rust workspace)"
    skip "fmt-check" "no Cargo.toml"
    skip "clippy" "no Cargo.toml"
    skip "tests" "no Cargo.toml"
    skip "coverage" "no Cargo.toml"
fi

# Resolve the kernel binary AFTER the build step, so a just-built local
# debug binary is found. In a Rust repo a missing binary is a hard failure
# (the kernel gates are required there); in a non-Rust repo they skip.
if [ ! -x "$BIN" ]; then
    BIN="$(command -v mini-agi 2>/dev/null || true)"
fi
if [ -n "$BIN" ] && [ ! -x "$BIN" ]; then
    BIN=""
fi

if [ -n "$BIN" ]; then
    step "eval-gate"    "$BIN" eval gate || fail=1
    step "checkpoint"   "$BIN" checkpoint audit || fail=1
    step "provenance"   "$BIN" provenance || fail=1
    step "mem-dedup"     "$BIN" mem verify || fail=1
    step "stats"        "$BIN" stats || fail=1
    step "budget"       "$BIN" budget || fail=1
    step "insights"     "$BIN" insights || fail=1
    step "audit"        "$BIN" audit || fail=1
    # Determinism (cycle-34 finding): derivation is a pure function of
    # canonical memory — a second derive must not change the brief. A
    # fresh init'd repo has no facts yet; derive reports that cleanly
    # and stays gate-green (an init'd repo is gate-green by design).
    step "derive" sh -c '
        out1=$("$1" derive 2>&1); rc1=$?
        if [ "$rc1" -ne 0 ]; then
            echo "$out1" | grep -q "no canonical facts" || { echo "derive: failed"; echo "$out1"; exit 1; }
            # A stale brief after the canonical was wiped would mask
            # nondeterminism — refuse instead of hashing old output.
            [ -f memory/derived/context-brief.md ] && { echo "derive: canonical wiped but stale brief present — rerun after ingest"; exit 1; }
            echo "derive: deterministic (no facts)"; exit 0
        fi
        # Hash EVERY derived artifact (brief, per-domain fragments,
        # CLAUDE.md) — a nondeterministic fragment must not pass.
        derive_hashes() { sha256sum memory/derived/context-brief.md memory/derived/per-domain/AGENTS.*.md CLAUDE.md 2>/dev/null | cut -d" " -f1 | sha256sum | cut -d" " -f1; }
        h1=$(derive_hashes) || exit 1
        out2=$("$1" derive 2>&1); rc2=$?
        if [ "$rc2" -ne 0 ]; then
            echo "derive: failed on second run"; echo "$out2"; exit 1
        fi
        h2=$(derive_hashes) || exit 1
        if [ "$h1" = "$h2" ]; then echo "derive: deterministic ($h1)"; else echo "derive: NOT DETERMINISTIC ($h1 != $h2)"; exit 1; fi
    ' sh "$BIN" || fail=1
else
    if [ "$has_cargo" -eq 1 ]; then
        echo "[FAIL] build: kernel binary missing — expected target/debug/mini-agi or mini-agi on PATH"
        fail=1
    else
        skip "eval-gate" "mini-agi binary not found (install: cargo install mini-agi)"
        skip "checkpoint" "mini-agi binary not found"
        skip "provenance" "mini-agi binary not found"
        skip "stats" "mini-agi binary not found"
        skip "budget" "mini-agi binary not found"
    fi
fi

# Sandbox attestation (ADR-0009): outside CI this is skipped — the local
# gate stays portable by design. Inside CI the gate FAILS unless the
# runner attests isolation: non-root user and a runner identity marker.
# A workflow running the gate without isolation markers is therefore red.
if [ "${CI:-}" = "true" ]; then
    evidence="user=$(id -u) runner=${RUNNER_NAME:-<unset>} kernel=$(uname -sr) container=${container:-none}"
    if [ "$(id -u)" -eq 0 ] || [ -z "${RUNNER_NAME:-}" ]; then
        echo "[FAIL] sandbox: no isolation evidence ($evidence)"
        fail=1
    else
        echo "[ok] sandbox: $evidence"
    fi
else
    skip "sandbox" "CI-only isolation attestation (ADR-0009)"
fi

if [ "$fail" -eq 0 ]; then
    echo "verify: ALL GREEN"
else
    echo "verify: FAILED"
    exit 1
fi
