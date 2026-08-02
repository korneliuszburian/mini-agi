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
export RUSTFLAGS="-D warnings"

fail=0

step() {
    label="$1"
    shift
    out="$("$@" 2>&1)" || { echo "[FAIL] $label:"; echo "$out" | head -20; return 1; }
    [ -n "$out" ] || { echo "[FAIL] $label: silent target (produced no output)"; return 1; }
    echo "[ok] $label"
    return 0
}

skip() {
    echo "[skip] $1: $2"
}

BIN="./target/debug/mini-agi"
has_cargo=0
if [ -f Cargo.toml ]; then
    has_cargo=1
    step "build"         cargo build || fail=1
    step "fmt-check"    sh -c 'cargo fmt --check && echo "fmt-check: clean"' || fail=1
    step "clippy"       cargo clippy --all-targets -- -D warnings || fail=1
    step "tests"        cargo test --all || fail=1
else
    skip "build" "no Cargo.toml (not a Rust workspace)"
    skip "fmt-check" "no Cargo.toml"
    skip "clippy" "no Cargo.toml"
    skip "tests" "no Cargo.toml"
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
    step "stats"        "$BIN" stats || fail=1
    step "budget"       "$BIN" budget || fail=1
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

if [ "$fail" -eq 0 ]; then
    echo "verify: ALL GREEN"
else
    echo "verify: FAILED"
    exit 1
fi
