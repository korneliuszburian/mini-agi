#!/bin/sh
# demo.sh — the killer demo: init a repo, give an agent a verified brain.
#
#   1. mini-agi init  -> scaffold (memory, gate scripts, MCP config)
#   2. memory write   -> one fact, provenance-gated
#   3. checkpoint     -> begin/audit cycle
#   4. gates          -> eval gate, skill verify, budget
#   5. MCP            -> show the config any agent plugs into
set -eu

BIN="${1:-$(cd "$(dirname "$0")/.." && pwd)/target/release/mini-agi}"
DEMO_DIR="${DEMO_DIR:-/tmp/mini-agi-demo}"

if [ ! -x "$BIN" ]; then
  echo "building $BIN..."
  cargo build --release
fi

rm -rf "$DEMO_DIR"
mkdir -p "$DEMO_DIR"
export AGENTIC_ROOT="$DEMO_DIR"

echo "==> init: scaffold a repo with a verified brain"
"$BIN" init

echo
echo "==> write an enforced fact (canonical memory, provenance)"
printf 'FACT: this demo proves any agent gets a verified brain via MCP.\n' > "$DEMO_DIR/demo-buffer.md"
"$BIN" mem consolidate "$DEMO_DIR/demo-buffer.md" --domain demo
"$BIN" derive
"$BIN" provenance

echo
echo "==> checkpoint journal (begin + audit)"
cd "$DEMO_DIR"
git init -q
git config user.email demo@mini-agi
git config user.name demo
git add -A && git commit -qm seed
"$DEMO_DIR/scripts/checkpoint.sh" begin demo-run
"$BIN" checkpoint audit

echo
echo "==> gates"
"$BIN" eval gate || true
"$BIN" stats
"$BIN" budget

echo
echo "==> MCP: any agent connects over stdio"
cat "$DEMO_DIR/opencode.json"
echo
echo "demo done. try:  npx @openai/codex --mcp '{\"command\":\"$BIN\",\"args\":[\"mcp\"]}'"
