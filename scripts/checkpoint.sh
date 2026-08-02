#!/bin/sh
# checkpoint.sh — Edit-Checkpoint Cascade (ECC). Port of the PoC script
# (scripts/checkpoint.sh), adapted to the Rust product's gate: `make verify`
# becomes `scripts/verify.sh`, and the checkpoint journal lives in
# memory/episodic/checkpoints.log.
#
# Invoke BEFORE a new edit step and AFTER a verification step. Each call:
#   1. snapshots current state as a git commit,
#   2. records it in the checkpoint journal,
#   3. on the NEXT call, if the verifier (scripts/verify.sh) failed since the
#      last checkpoint, rolls back to the last green checkpoint
#      (git reset --hard).
#
# Usage:
#   scripts/checkpoint.sh begin <label>   # commit current state, journal it
#   scripts/checkpoint.sh ack <label> <reason>  # acknowledge a journal violation
#   scripts/checkpoint.sh verify <label>  # run verifier; green -> keep; red -> rollback
#   scripts/checkpoint.sh status          # show journal tail
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
JOURNAL="$ROOT/memory/episodic/checkpoints.log"
mkdir -p "$ROOT/memory/episodic"

ts() { date -u +%Y-%m-%dT%H:%M:%SZ; }

case "${1:-}" in
  begin)
    label="${2:-step}"
    rev="$(git -C "$ROOT" rev-parse --short HEAD)"
    if [ -n "$(git -C "$ROOT" status --porcelain)" ]; then
      dirty_paths="$(git -C "$ROOT" status --porcelain | sed 's/^...//')"
      disallowed_paths="$(printf '%s\n' "$dirty_paths" | awk '!/^(tickets\/|scripts\/|tests\/|memory\/|evals\/|docs\/|adr\/|artifacts\/|knowledge\/|\.agents\/|crates\/|Makefile$|AGENTS\.md$|CLAUDE\.md$|Cargo\.toml$|Cargo\.lock$|opencode\.json$|\.gitignore$)/')"
      if [ -n "$disallowed_paths" ]; then
        echo "$(ts) CHECKPOINT-ABORT $label disallowed dirty paths: $disallowed_paths" >> "$JOURNAL"
        echo "checkpoint aborted: dirty paths outside the allowlist:" >&2
        printf '%s\n' "$disallowed_paths" >&2
        exit 1
      fi
      git -C "$ROOT" add -A
      git -C "$ROOT" commit -q -m "checkpoint: $label"
      rev="$(git -C "$ROOT" rev-parse --short HEAD)"
      echo "$(ts) BEGIN $label -> $rev" >> "$JOURNAL"
      echo "checkpoint $rev: $label"
    else
      echo "$(ts) BEGIN $label -> $rev (clean)" >> "$JOURNAL"
      echo "checkpoint $rev: $label (clean)"
    fi
    ;;
  ack)
    label="${2:-}"
    reason="${3:-}"
    if [ -z "$label" ] || [ -z "$reason" ]; then
      echo "usage: $0 ack <label> <reason>"; exit 1
    fi
    echo "$(ts) STATUS $label $reason" >> "$JOURNAL"
    echo "acknowledged: $label"
    ;;
  verify)
    label="${2:-step}"
    prev="$(git -C "$ROOT" rev-parse --short HEAD)"
    if scripts/verify.sh >/dev/null 2>&1; then
      echo "$(ts) VERIFY-PASS $label @ $prev" >> "$JOURNAL"
      echo "green: $label @ $prev"
    else
      last_green="$(awk '/BEGIN/{for (i = 1; i <= NF; i++) if ($i == "->") r = $(i + 1)} END{print r}' "$JOURNAL" 2>/dev/null || true)"
      if [ -n "$last_green" ] && [ "$last_green" != "$prev" ]; then
        echo "$(ts) VERIFY-FAIL $label @ $prev -> ROLLBACK to $last_green" >> "$JOURNAL"
        git -C "$ROOT" reset -q --hard "$last_green"
        git -C "$ROOT" clean -fdq -e memory/episodic/checkpoints.log
        mkdir -p "$ROOT/memory/episodic"
        echo "RED: $label @ $prev — rolled back to green checkpoint $last_green"
      else
        echo "$(ts) VERIFY-FAIL $label @ $prev -> NO-ROLLBACK" >> "$JOURNAL"
        echo "RED: $label @ $prev — no earlier green checkpoint, leaving tree as-is"
      fi
      exit 1
    fi
    ;;
  status)
    tail -n 20 "$JOURNAL" 2>/dev/null || echo "no checkpoint journal yet"
    ;;
  *)
    echo "usage: $0 {begin|ack|verify|status} [label]"; exit 1;;
esac
