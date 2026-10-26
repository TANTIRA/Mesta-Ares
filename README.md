# Mesta-Ares

Agentic SIEM+SOAR. Wazuh + OpenSearch collect and correlate; TheHive, Cortex,
MISP, and Shuffle handle cases, analysis, intel, and workflows; a Rust
control plane (`tool-server`, `orchestrator`, and per-role agents on a NATS
JetStream bus) triages, hunts, and responds **autonomously** inside a strict
policy envelope.

## Architecture

```mermaid
flowchart LR
    subgraph Telemetry
        W[Wazuh agents] -->|alerts| WM[wazuh-manager]
        WM -->|integratord webhook| IG[ingestion-gateway]
    end
    IG --> N[(NATS JetStream)]
    N --> ORCH[orchestrator]
    ORCH --> TA[triage-agent]
    ORCH --> RA[response-agent]
    ORCH --> HA[hunt-agent]
    DA[detection-agent] --> OS[(OpenSearch)]
    BAS[bas-runner] --> N
    TA & RA & HA --> TS[tool-server]
    TS --> PG[(Postgres + pgvector)]
    TS --> R[(Redis)]
    TA & RA & HA --> OS
    TS -->|gated tools| SOAR[TheHive / Cortex / MISP / Shuffle]
    TS -->|gated tools| WM
    API[api-server] --> N
    API --> PG
    CON[console] --> API
```

Every side effect goes through `tool-server`: response actions, case creation
(triage and hunt), and detection deployments all invoke allowlisted tools, so
the policy gate, idempotency store and audit chain see them.

## Security model

- **Autonomous by default, gated by policy.** `policies/tools.yml` is a
  deny-by-default allowlist: each tool has an `impact` class, an `autonomy`
  level, arg schemas, and blast-radius caps (rate limits + cooldowns).
- **Hard boundary.** `policies/protected-assets.yml` lists assets (domain
  controllers, Tier-0 accounts, RFC1918 ranges, the control plane itself)
  that can never be the target of a write/high-impact call — not
  runtime-overridable; changes require a commit.
- **Kill switch.** `ARES_KILL_SWITCH=1` or a non-empty `$POLICIES_DIR/kill`
  file halts every agent and the policy gate (one shared predicate in
  `agent-protocol::kill`); `POST /controls/kill-switch` writes that file.
- **Detection-as-code.** Rules live in `detection-content/`, are drafted by
  the agent into `pending/`, backtested, and promoted to `active/` only by
  commit — then deployed as OpenSearch monitors.
- **Audit.** Tool calls, denials, and verdicts are persisted for replay
  (see `evals/`).

## Quickstart

```sh
cp .env.example .env            # fill in API keys
docker compose -f infra/docker-compose.yml up -d
```

Then:

| Console | URL |
|---|---|
| Ares console | http://localhost:3000 |
| api-server | http://localhost:8080 |
| TheHive | http://localhost:9000 |
| Cortex | http://localhost:9001 |
| Shuffle | http://localhost:3001 |
| MISP | https://localhost:8443 |
| Wazuh dashboard | http://localhost:5601 (`--profile wazuh-ui`) |
| OpenSearch | http://localhost:9200 |

To receive Wazuh alerts, add the block in `infra/wazuh/integratord.xml` to
`ossec.conf` inside the manager container (the forwarder script is already
bind-mounted at `/var/ossec/integrations/custom-ares-webhook`).

Production skeleton: `helm install ares infra/helm/ares` after creating an
`ares-secrets` Secret and setting `global.imageRegistry`. Plain Kubernetes
manifests for the same stack live in `infra/k8s` — deploy with
`./infra/k8s/apply.sh` (see `infra/k8s/README.md`).

## Repo layout

| Path | Contents |
|---|---|
| `crates/` | Rust workspace (see `Cargo.toml`): control plane (`tool-server`, `policy-engine`, `audit`, `idempotency`), edge (`api-server`, `ingestion-gateway`), agents (`orchestrator`, triage/response/detection/hunt/vuln), shared libs (`agent-protocol`, `agent-core`, `llm-client`, `connectors`, `enrichment-lib`, `cvss`, `http-limits`, `a2a`, `acp`), and `bas-runner` |
| `apps/console/` | Next.js console |
| `infra/` | docker-compose dev stack, Dockerfiles, Helm chart, plain k8s manifests, Wazuh integration |
| `docs/openapi/` | OpenAPI 3.0.3 specs for every HTTP surface (gateway, api-server, tool-server, ingestion-gateway) |
| `policies/` | Tool allowlist + protected assets (policy gate inputs) |
| `detection-content/` | Sigma rules — `active/` deployed, `pending/` agent drafts |
| `evals/` | Golden sets + policy negative-test matrix for offline eval |
| `bas/` | Breach-and-attack-simulation scenario manifests |
| `.github/workflows/` | CI: rust fmt/clippy/test, sigma check, evals, console, compose validation |
