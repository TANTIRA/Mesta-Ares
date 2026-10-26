# Evals

Offline evaluation for the agentic layer. Two kinds today:

- **`golden_sets/`** — labeled alert corpora for scoring `triage-agent`
  precision/recall. One JSON object per line:
  `{"alert": {...}, "label": "tp" | "fp"}`. `triage_seed.jsonl` covers the
  four telemetry families (`endpoint_ad`, `network`, `m365`, `cloud`).
- **`policy_tests.md`** — negative test matrix for the policy gate: every
  tool call that must be denied, and why.

## Running

```sh
./evals/run.sh
```

Today `run.sh` validates that every golden-set line parses and carries a
legal label, then exits 0. It is wired into CI (`.github/workflows/ci.yml`).

The real harness (TODO): replay `triage_seed.jsonl` through `triage-agent`
with a stubbed `tool-server`, compare predicted vs. gold labels, report
precision/recall/F1 per telemetry family, and fail below thresholds.

## Adding alerts

- One JSON object per line, no pretty-printing.
- Keep `family` values stable: `endpoint_ad`, `network`, `m365`, `cloud`.
- Label what a human analyst would decide, not what the current agent says.
- Prefer FP examples that are *hard*: benign-but-scary (scanners, OneDrive
  run keys, sanctioned admin scripts).
