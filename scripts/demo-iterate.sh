#!/bin/bash
# Demo: the verified-iteration breakthrough pattern (EXP-012/013).
# Builds a below-bar task (hidden suite), runs a blind single-shot
# (plain) vs the kernel's verified-iteration loop (--iterate +
# --blind-worker), and prints both outcomes.
set -e
BIN=${BIN:-$(dirname "$0")/../target/debug/mini-agi}
TASK=${TASK:-e1}
WORK=/tmp/mini-agi-demo
HIDDEN=/tmp/mini-agi-demo-hidden
rm -rf "$WORK" "$HIDDEN" && mkdir -p "$WORK" "$HIDDEN"

# Hidden suite: config-line parser with quoted/comment/whitespace cases.
cat > "$WORK/README-spec.md" <<'SPEC'
# Demo task - config-line parser
Implement parse_config(line) -> (key, value) in config.py. A line has
the form `key = value`. Everything left of the first '=' is the key
(trimmed); right is the value (trimmed). Lines with no '=' return None.
The verifier (make verify) runs a hidden test suite and must pass.
SPEC
cat > "$WORK/config.py" <<'PY'
def parse_config(line: str):
    # TODO: implement
    raise NotImplementedError
PY
cat > "$WORK/Makefile" <<'MK'
verify:
	PYTHONPATH=. python3 $(HIDDEN)/test_hidden.py
MK
cat > "$HIDDEN/test_hidden.py" <<'PY'
import sys, unittest
sys.path.insert(0, '.')
from config import parse_config
class Hidden(unittest.TestCase):
    def test_simple(self): self.assertEqual(parse_config('a = b'), ('a','b'))
    def test_quoted(self): self.assertEqual(parse_config('k = "hi there"'), ('k','"hi there"'))
    def test_comment(self): self.assertEqual(parse_config('k = v  # c'), ('k','v'))
    def test_no_eq(self): self.assertIsNone(parse_config('no equals'))
if __name__ == '__main__': unittest.main()
PY

echo "== plain (blind single-shot) =="
cd "$WORK" && codex exec -s workspace-write --skip-git-repo-check \
  "$(cat README-spec.md)

You CANNOT run the hidden test suite; reason from the spec and write the implementation." \
  > /dev/null 2>&1 || true
PYTHONPATH=. python3 "$HIDDEN/test_hidden.py" > /tmp/demo-plain.log 2>&1 && echo "plain: PASS" || echo "plain: FAIL"

echo "== kernel verified-iteration (--iterate --blind-worker) =="
cd "$WORK" && rm -f config.py && cat > config.py <<'PY'
def parse_config(line: str):
    # TODO: implement
    raise NotImplementedError
PY
"$BIN" codex README-spec.md . --verify "PYTHONPATH=. python3 $HIDDEN/test_hidden.py" \
  --target . --iterate 3 --blind-worker --hidden-dir "$HIDDEN" --no-sandbox 2>&1 \
  | grep -E "attempt|verifier|attempts" || true
echo "demo done — run.json: $(python3 -c "import json; d=json.load(open('$WORK/run.json')); print('attempts', d.get('attempts'), 'verifier_passed', d.get('verifier_passed'))")"
