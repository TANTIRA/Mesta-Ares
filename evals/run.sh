#!/usr/bin/env bash
# Eval harness — currently a format check; real scoring is TODO (see README).
set -euo pipefail
cd "$(dirname "$0")"

echo "== evals: validating golden set format =="
python3 - <<'PY'
import json, pathlib, sys

files = sorted(pathlib.Path("golden_sets").glob("*.jsonl"))
if not files:
    sys.exit("no golden sets found under evals/golden_sets/")

total = 0
labels = {"tp": 0, "fp": 0}
for f in files:
    for i, line in enumerate(f.read_text().splitlines(), 1):
        if not line.strip():
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError as e:
            sys.exit(f"{f}:{i} invalid JSON: {e}")
        if not isinstance(obj.get("alert"), dict):
            sys.exit(f"{f}:{i} missing 'alert' object")
        if obj.get("label") not in ("tp", "fp"):
            sys.exit(f"{f}:{i} label must be 'tp' or 'fp', got {obj.get('label')!r}")
        labels[obj["label"]] += 1
        total += 1

print(f"ok: {total} labeled alerts ({labels['tp']} tp / {labels['fp']} fp) across {len(files)} set(s)")
PY

echo "TODO: replay golden sets through triage-agent and score precision/recall (needs ANTHROPIC_API_KEY)"
