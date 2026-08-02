#!/bin/sh
# verify.sh — the deterministic gate of mini-agi-rs.
# Sensor contract (PoC Makefile semantics): every target must exit 0 AND
# produce output; a silent target is a failing target.
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

step "fmt-check"    sh -c 'cargo fmt --check && echo "fmt-check: clean"' || fail=1
step "clippy"       cargo clippy --all-targets -- -D warnings || fail=1
step "tests"        cargo test --all || fail=1
step "eval-gate"    ./target/debug/mini-agi eval gate || fail=1
step "provenance"   ./target/debug/mini-agi provenance || fail=1

if [ "$fail" -eq 0 ]; then
    echo "verify: ALL GREEN"
else
    echo "verify: FAILED"
    exit 1
fi
