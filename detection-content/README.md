# Detection-as-code

Sigma rules live in this directory and are deployed to OpenSearch as
per-document monitors by the `detection-agent`.

## Flow

```
agent drafts rule → pending/ → backtest vs. indexed telemetry → active/ → deployed monitor
```

1. **Draft** — `detection-agent` (or a human) writes a Sigma rule into
   `pending/`. Every rule needs `id`, `status`, `level`, `logsource`, a
   `detection` block, and MITRE ATT&CK `tags`.
2. **Backtest** — the agent replays the rule over recent telemetry in
   OpenSearch and estimates the false-positive rate. Promotion requires the
   FP count to stay under the `backtest_fp_max` budget passed to
   `detection.sigma.deploy` (see `policies/tools.yml`).
3. **Active** — passing rules move to `active/` via a repo commit. The agent
   cannot edit `active/` in place; promotion is always a git change.
4. **Deploy** — `detection.sigma.deploy` converts the Sigma rule to an
   OpenSearch query and installs it as a monitor. Alerts raised by the
   monitor flow back through `ingestion-gateway` like any other source.

## CI

GitHub Actions runs `sigma check detection-content/` (see
`.github/workflows/ci.yml`) on every push — malformed rules fail the build.

## Directory layout

| Dir        | Contents                                        |
|------------|-------------------------------------------------|
| `active/`  | Deployed rules. Change only via reviewed commit. |
| `pending/` | Drafts awaiting backtest/promotion.             |
