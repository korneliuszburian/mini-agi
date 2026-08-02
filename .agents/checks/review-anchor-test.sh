#!/bin/sh
# Self-test for the review skill's memory-anchor gate (ADR-0003).
# Structural checks: canonical memory always carries 16-hex fact ids, and
# the review rubric enforces the anchor rule.
set -eu

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

fail=0

if ! grep -qE '[a-f0-9]{16}' "$ROOT"/memory/canonical/entries/*/*.md; then
    echo "[FAIL] canonical memory carries no 16-hex fact ids"
    fail=1
fi

if ! grep -q 'Anchors:' "$ROOT/.agents/checks/review-rubric.md"; then
    echo "[FAIL] review rubric does not enforce the Anchors line"
    fail=1
fi

if ! grep -q 'memory-anchor\|Anchors' "$ROOT/.agents/skills/review/SKILL.md"; then
    echo "[FAIL] review skill does not state the memory-anchor gate"
    fail=1
fi

[ "$fail" -eq 0 ] || exit 1
echo "review-anchor: ALL GREEN"
