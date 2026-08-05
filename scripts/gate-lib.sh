#!/bin/sh
# Shared helpers for the deterministic verification gate.

step() {
    label="$1"
    shift
    out="$("$@" 2>&1)" || {
        echo "[FAIL] $label:"
        printf '%s\n' "$out" | awk '{ printf "line-%d %s\n", NR, $0 }'
        return 1
    }
    [ -n "$out" ] || { echo "[FAIL] $label: silent target (produced no output)"; return 1; }
    echo "[ok] $label"
    return 0
}

skip() {
    echo "[skip] $1: $2"
}
