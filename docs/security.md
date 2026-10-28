# Security posture — Mesta-Ares

Companion to `docs/threat-model.md` (risks, grid, tickets). This file is the
operator-facing control sheet.

## Credential egress map

| Destination | What leaves | Control |
|---|---|---|
| Anthropic (`ANTHROPIC_BASE_URL`) | Alert/IOC text in triage+planner prompts | `ANTHROPIC_BASE_URL=http://localhost:11434` keeps all LLM traffic on local Ollama; `DECISION_MODEL` covers triage verdicts only — planner/drafting calls follow the base URL |
| NVD / VirusTotal / Shodan / AbuseIPDB / GreyNoise / urlscan | The queried IOC only | Keys server-side; Shodan key errors are URL-stripped before logs |
| TheHive / Cortex / MISP / Wazuh / Shuffle | Tool-call payloads | Only reachable via tool-server; writes are `autonomy: approval` |
| MISP TLS | intel | Verifies by default; `MISP_TLS_INSECURE=1` is the explicit dev opt-in |

## Key model

- `ARES_API_KEYS=key:actor[:role],...` gates api-server, tool-server,
  ingestion-gateway; role `approver` decides `/approvals/*`; self-approval
  is denied. `ARES_REQUIRE_AUTH=1` refuses boot without keys ≥32 chars.
- `ARES_TOOL_KEY` is the service-to-service key: agents → tool-server,
  api-gateway → upstreams (client credentials never propagate inward).
- `ARES_CONSOLE_KEY` is injected by the console's server-side BFF proxy —
  nothing secret reaches the browser.
- `GATEWAY_API_KEYS` gates the edge; `GATEWAY_ALLOW_ANONYMOUS=1` is dev-only.
- NATS accounts (`infra/nats/nats.conf`): each service publishes/subscribes
  only its subjects; `ares.>` publish is restricted to orchestrator + api.
- Rotation = edit the secret + restart (`kubectl rollout restart` /
  `docker compose up -d`). Keys are env-resident by design today.

## Audit integrity

`audit_log` is a hash chain (`/audit/verify` re-walks it). Additionally,
api-server appends the chain tail to `AUDIT_ANCHOR_PATH`
(default `audit-anchor.log`) every 60s — mount that file on storage outside
Postgres so a DB writer cannot rewrite both. Verify reports `anchored`.

## Data plane

- JetStream persists `ares.>`; `ARES_DLQ` caps dead letters at 100k
  (discard-oldest).
- Compose publishes datastore/internal ports on `127.0.0.1` only; the
  gateway (`:8082`) and console (`:3000`) are the published edge.
- Postgres/Redis take env-provided passwords; k8s ships
  `networkpolicy.yaml` (default-deny + datastore/edge allowlists).
- Detection content is mounted read-only for readers; only detection-agent
  writes (promotion = pending → active).
