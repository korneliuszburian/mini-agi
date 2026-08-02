#!/bin/sh
# hitl-loop.template.sh — structured human-in-the-loop driver for the
# diagnosing-bugs skill when no agent-runnable feedback loop can be built.
# The human performs the steps; captured output feeds back to the agent.
#
# Usage: edit STEP_COMMAND and SYMPTOM, then run; or use it as a template
# for a repo-specific loop script.
set -eu

STEP_COMMAND="${1:-:}"
SYMPTOM="${2:-symptom not provided}"
OUT=/tmp/hitl-loop-output.log
RUN=0
FAIL=0

while [ "$RUN" -lt 10 ]; do
  RUN=$((RUN + 1))
  echo "--- run $RUN ---"
  echo "SYMPPT_CHECK: $SYMPTOM"
  if sh -c "$STEP_COMMAND" >"$OUT" 2>&1; then
    echo "run $RUN: PASS (symptom present?) — capture:"
    head -20 "$OUT"
    FAIL=$((FAIL + 1))
  else
    echo "run $RUN: FAIL (exit non-zero) — capture:"
    head -20 "$OUT"
  fi
  read -r -p "continue? [y/N] " answer || answer=N
  [ "$answer" = "y" ] || break
done

echo "symptom seen in $FAIL/$RUN runs — captured in $OUT"
