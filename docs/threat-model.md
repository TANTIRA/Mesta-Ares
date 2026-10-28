# Mesta-Ares — STRIDE Threat Model

Scope: the system as deployed by `infra/docker-compose.yml` and `infra/k8s/` on `main`
(post-PR-#1 architecture: gateway, api-server, tool-server, agents, A2A/ACP, approvals).
Model date: 2026-10-28. Re-run triggers: new external dependency, new trust boundary,
new data class, any auth-boundary change.

## 1. System model

```
                        ┌─────────────── TB1: untrusted / public ───────────────┐
                        │                                                       │
  A1 Analyst (browser) ─┼─► P1 console (Next.js :3000)                          │
  A2 External A2A/ACP   ─┼─► P2 api-gateway (axum :8082)                         │
     agent client       │      │                                                │
  A3 Wazuh manager,     ─┼─► P4 ingestion-gateway (:8091)  ◄── direct bind      │
     webhook sources    │      │                                                │
  A4 Human approver     ─┼─► (direct :8080/:8090/:8091 binds bypass P2)         │
  A5 Network attacker   ─┼─────────────────────────────────────────────────────│
                        └──────┼────────────────────────────────────────────────┘
                               │ TB2: HTTP auth (ARES_API_KEYS — FAIL-OPEN,
                               │      not wired in any shipped manifest)
        ┌──────────────────────┼──────────────────────────────────────┐
        │ P3 api-server :8080  │  P5 tool-server :8090                │
        │  REST / A2A / ACP    │  /invoke /approvals /tools           │
        │  SSE streams         │  policy-engine · kill-switch         │
        │  approvals UI data   │  idempotency · audit writer          │
        └──────┬───────────────┘  │   ▲ holds ALL side-effect creds   │
               │                  │   │ (MISP, TheHive, Cortex, Wazuh, │
               │                  │   │  Shuffle, OpenSearch, TI, NVD) │
               │             ┌────┴───┴────────────────────┐          │
               │             │        TB5: side-effects    │          │
               │             ▼                             ▼          │
               │      D5 MISP :8443  D6 TheHive :9000 / Cortex :9001   │
               │      D7 Wazuh mgr :55000  D8 Shuffle :5001            │
               │      E5 OpenShell :8180 (docker.sock, root)           │
               │                                                       │
               │  TB3: message bus — NATS JetStream, NO AUTH           │
               ▼                                                       │
        ┌─────────────┐   ares.triage / respond / detection / hunt /   │
        │ D2 NATS     │──► vulns / bas / activity / dlq                │
        │ :4222 :8222 │                                              │
        └──────┬──────┘                                              │
               │                                                     │
   ┌───────────┼────────────────────────────────────────┐           │
   ▼           ▼            ▼           ▼        ▼        ▼           │
 P6 orch.   P7 triage   P8 response  P9 det.  P10 hunt  P11 vuln/    │
            agent       agent        agent    agent     P12 bas      │
              │           │           │         │                    │
              └── LLM calls ──► TB6 egress ──► E1 Anthropic          │
                              E2 Ollama :11434 (local)               │
                              E3 NVD · E4 VT/Shodan/AbuseIPDB/GreyNoise
               │                                                     │
               │  TB4: datastores — no or trivial auth, host ports    │
               ▼                                                     │
   D1 Postgres :5432 (ares:ares)  D3 Redis :6379  D4 OpenSearch :9200 │
   alerts·cases·audit_log·approvals·a2a_tasks·acp_runs·advisories     │

   TB7: host daemon — docker.sock mounted by Cortex, Shuffle, OpenShell
```

Element index — Actors: A1 analyst, A2 external agent, A3 webhook source,
A4 approver, A5 attacker. Processes: P1 console, P2 api-gateway, P3 api-server,
P4 ingestion-gateway, P5 tool-server, P6 orchestrator, P7–P12 agents.
Stores: D1 Postgres, D2 NATS, D3 Redis, D4 OpenSearch, D5–D8 SOAR platform stores.
External: E1 Anthropic, E2 Ollama, E3 NVD, E4 TI providers, E5 OpenShell.

## 2. Element × STRIDE grid

Only applicable cells listed; "—" = not applicable or no realistic threat.

| # | Element | S | T | R | I | D | E |
|---|---------|---|---|---|---|---|---|
| A5 | Network attacker | Forge webhooks, A2A tasks, NATS envelopes | Rewrite NATS msgs, DB rows | — | Read everything when auth unset | Flood ingest/LLM/SSE | Invoke tools when auth unset → policy-bounded side effects |
| A2 | External A2A/ACP agent | Stolen/spoofed agent card, forged task claims | Task state in D1 | Deny task submission | Read task results of other agents | Task spam → agent/LLM burn | Scope-bypass: agent-scoped tasks exist, verify enforcement |
| A3 | Webhook source | Spoofed source IP/payload | — | Deny alert provenance | — | Alert flood → triage/LLM cost DoS | — |
| P1 | console | — | — | — | Serves API URL baked in image; key handling in browser | — | — |
| P2 | api-gateway | — | Cached responses shared across keys | — | Cache cross-key leak; upstream errors leak URLs | Per-IP in-process limits; cache unbounded per prefix | Anonymous dev mode = full bypass |
| P3 | api-server | Key spoof (static keys, no rotation) | — | — | /audit, /alerts, /plans read; SSE activity stream | Large queries, SSE fan-out, A2A task spam | — |
| P4 | ingestion-gateway | Webhook forgery | — | — | — | Flood → D2/D1/LLM downstream | — |
| P5 | tool-server | Actor header spoof only if auth unset | — | Approver = any key holder (no SoD) | /tools reveals capability surface | Idempotency store exhaustion | **The jackpot**: holds all side-effect creds; policy is the only gate |
| P6 | orchestrator | — | Dedupe bypass via D3 flush | — | — | Dedupe flood | — |
| P7 | triage-agent | Forged ares.triage envelopes | — | — | Alert content → LLM prompt (egress) | LLM token burn | **Prompt injection → tool-arg steering** |
| P8 | response-agent | Forged ares.respond envelopes | — | Denies action provenance | Plan/result data → LLM | LLM + tool-loop burn | Prompt injection → tool calls (policy bounds) |
| P9 | detection-agent | Forged detection envelopes | Detection rules in mounted dir | — | — | — | Malicious rule = code-adjacent content |
| P10 | hunt-agent | Forged ares.hunt | — | — | Hunt results | Scheduled hunt resource burn | — |
| P11 | vuln-agent | — | — | — | — | NVD poll loop failure | — |
| P12 | bas-runner | — | BAS scenarios in mounted dir | — | Containment tool results | — | Containment tools = real side effects |
| D1 | Postgres | — | ares:ares, host :5432; rewrite alerts/cases/**approvals rows** | Rewrite + rehash audit_log | All alert/case/approval data | Connection exhaustion | DB write = forge approval, bypass API+audit entirely |
| D2 | NATS | **Any network peer publishes** forged envelopes | Rewrite in-flight msgs | — | Subscribe = read all traffic | Queue flood, DLQ poison | Publish ares.respond → side effects via policy-bounded tools |
| D3 | Redis | — | Flush dedupe → duplicate actions | — | Idempotency keys | — | — |
| D4 | OpenSearch | — | **Rewrite telemetry → detection blindness** | — | All telemetry | — | — |
| D5–D8 | SOAR platforms | Cred theft from P5 env | MISP intel, TheHive cases | — | Full intel/case data | — | Cortex/Shuffle docker.sock → host |
| E1–E4 | Third-party APIs | — | MITM if TLS weak (MISP skip-verify in dev) | — | Alert/IOC content egress to Anthropic/TI | Provider outage cascade | — |
| TB7 | docker.sock ×3 | — | — | — | — | — | Container escape → host root → everything |

## 3. Ranked risk register

Severity = impact × likelihood. Exposure assumes LAN-reachable dev (all ports
bind 0.0.0.0) and cluster-internal prod for NATS/datastores.

| Rank | ID | Threat | Cat | Impact | Likelihood | Sev |
|---|---|---|---|---|---|---|
| 1 | R1 | **All HTTP surfaces unauthenticated in shipped deploys** — `ARES_API_KEYS` never passed in compose/k8s/helm; `require_auth` fails open; gateway `ALLOW_ANONYMOUS=1` in dev. Anyone reachable can `/invoke` tools, approve approvals, read alerts/audit. | S,E,I,R | Full policy-bounded action execution + intel read | Trivial — no exploit needed | **Critical** |
| 2 | R2 | **NATS unauthenticated publish** — forged `ares.respond`/`ares.triage` envelopes drive agents to invoke tools. Host port 4222 exposed in compose; lateral-movement vector in k8s. | S,T,E,D | Forged response actions, message eavesdrop | Low complexity | **Critical** |
| 3 | R3 | **docker.sock mounts ×3** (Cortex, Shuffle, OpenShell as root) — any RCE in those services = host root. | E | Total host compromise | Needs service RCE | High |
| 4 | R4 | **Postgres ares:ares on host :5432** — direct SQL: forge approval rows (bypasses API + audit), rewrite alerts/cases, rehash audit_log chain. | T,R,E | Silent history/approval forgery | Trivial | High |
| 5 | R5 | **No separation of duty on approvals** — approver is any `ARES_API_KEYS` holder; requester can self-approve; combined with R1 anyone approves. | E,R | Approval gate bypass | Trivial | High |
| 6 | R6 | **LLM prompt injection via alert content** — attacker-controlled event text reaches triage/response prompts; steers tool args within policy allowlist (scope creep on read tools, action targeting). | T,E | Bounded side-effect steering | Medium — needs alert-path control | High |
| 7 | R7 | **OpenSearch/Redis unauthenticated** — rewrite telemetry (detection blindness), flush dedupe → duplicate side effects. | T,I | Detection evasion + duplicate actions | Trivial when reachable | High |
| 8 | R8 | **Audit chain not anchored** — `audit_log` verifies sequential hash but a DB writer recomputes the chain; `/audit/verify` reads the same DB it checks. | R | Undetectable history rewrite | Needs D1 write | Medium |
| 9 | R9 | **Alert content egress to Anthropic** — internal IOCs/PI leave boundary in prompts; TI lookups leak investigated indicators. | I | Confidentiality breach of investigation data | Certain — by design | Medium |
| 10 | R10 | **MISP TLS skip-verify (dev)** — creds + intel MITM-able; documented pattern may leak to prod. | I,T | Cred theft, poisoned intel | Needs path position | Medium |
| 11 | R11 | **Secrets as env/.env everywhere** — `.env` on disk, `docker inspect` leaks, k8s env-var secrets; single service RCE reads every cred it carries. | I | Cred compromise | High | Medium |
| 12 | R12 | **Webhook/ingest flood → LLM cost DoS** — junk alerts burn Anthropic tokens and fill DLQ. | D | Financial + triage blindness | Trivial under R1 | Medium |
| 13 | R13 | **Per-IP in-process rate limits** — N replicas = N× limit; bypassable behind shared NAT / spoofed source. | D | Weakened DoS controls | Medium | Medium |
| 14 | R14 | **Gateway cache shared across keys** — cached GET responses served to any authenticated key. | I | Low (same data class today) | Certain | Low |
| 15 | R15 | **Supply chain** — compose header admits image tags "pinned from memory"; no digest verification. | T | Vulnerable/wrong images | Medium | Medium |
| 16 | R16 | **k8s manifests: no NetworkPolicy, env-var secrets, PG data on default PVC** — flat intra-cluster trust. | I,T | Lateral movement in cluster | Medium | Medium |
| 17 | R17 | **DLQ unbounded / no replay policy** — poisoned or accumulating dead letters. | D | Backlog, storage exhaustion | Low | Low |
| 18 | R18 | **Detection content mount writable** — detection rules / BAS scenarios on writable PVC; modifying detection = evasion. | T | Silent detection evasion | Needs file access | Medium |

## 4. Mitigation tickets

| ID | Risk | Strategy | Ticket (acceptance criteria) |
|---|---|---|---|
| T-01 | R1 | Reduce | **Wire auth into every deploy manifest.** Pass `ARES_API_KEYS` in compose `x-ares-secrets`, k8s Secret, helm values. Remove `GATEWAY_ALLOW_ANONYMOUS` from shipped compose (doc-only opt-in). AC: fresh `docker compose up` → 401 on `/alerts`, `/invoke`, `/webhooks/*`, `/api/*` without key; with key → 200. |
| T-02 | R1,R5 | Reduce | **Fail-closed auth option.** `ARES_REQUIRE_AUTH=1` makes `ApiKeys::from_env` fatal when unset; set it in shipped manifests. AC: service refuses to boot with flag set and no keys. |
| T-03 | R2 | Reduce | **NATS accounts.** Dedicated user per service: agents subscribe-only on their subject, only orchestrator/ingress may publish `ares.*`, DLQ reader is separate. AC: publish to `ares.respond` with agent creds → permission violation logged. |
| T-04 | R3 | Reduce/Avoid | **Remove docker.sock or gate it.** Cortex/Shuffle via `docker-socket-proxy` exposing only `containers/images` APIs; OpenShell on a dedicated node/VM. AC: `curl --unix-socket` from inside each service can list containers but not create/exec. |
| T-05 | R4,R7,R16 | Reduce | **Datastore hardening.** Generated DB password via secret (not `ares:ares`); drop host port bindings for Postgres/Redis/OpenSearch in a `prod` compose profile; add k8s NetworkPolicy limiting D1–D4 to known pods; OpenSearch security plugin or TLS+auth. AC: from an unlabeled pod / off-host, connect to :5432/:6379/:9200 fails. |
| T-06 | R5 | Reduce | **Approver SoD.** `ARES_API_KEYS` spec gains role tag (`key:actor:role`); `/approve`/`/reject` require `role=approver` and `actor != requesting actor`. AC: invoke with key A, approve with key A → 403; approve with role-less key → 403. |
| T-07 | R6 | Reduce | **Prompt-injection defenses + residual doc.** Delimit/label untrusted event text in prompts; record which tool args derived from alert content (taint flag in audit). Document residual: policy allowlist is the hard bound. AC: injection fixture in test alert cannot place args outside schema; audit event marks tainted args. |
| T-08 | R8 | Reduce | **Anchor audit chain externally.** Nightly job exports chain head hash to a write-once target outside D1 (separate creds: S3 object-lock, or signed file). AC: `/audit/verify` plus `audit-anchor` file detect a rewritten middle row. |
| T-09 | R9 | Accept+Reduce | **LLM egress policy.** Classify fields permitted in prompts; prefer `DECISION_MODEL` (local Ollama) for sensitive deployments; optional redaction pass before provider call. AC: doc lists sent fields; `LLM_EGRESS=ollama` routes all decision calls locally. |
| T-10 | R10 | Reduce | **MISP TLS.** `MISP_TLS_INSECURE=1` explicit opt-in, default verify; ship a CA mount path. AC: default client fails against self-signed MISP; succeeds with proper CA. |
| T-11 | R11,R16 | Reduce | **Secrets management.** k8s ExternalSecrets/SealedSecrets or mounted secret files instead of env; document `.env` perms (600) and never bake keys into images (incl. `NEXT_PUBLIC_*`). AC: `kubectl describe pod` shows no key material; image scan shows none. |
| T-12 | R12,R17 | Reduce | **Ingest flood controls + DLQ policy.** Per-source quotas before NATS publish; DLQ max-length + alert on growth + documented replay procedure. AC: 10k junk webhooks → capped at quota, DLQ alert fires. |
| T-13 | R13 | Accept (doc) | **Rate-limit scaling note.** Document that limits are per-replica per-IP; add per-key limit tier using authenticated actor. AC: docs state effective limit = replicas × configured; per-key limit test passes. |
| T-14 | R14 | Reduce | **Cache key scope.** Include actor identity in gateway cache key, or restrict `GATEWAY_CACHE_PREFIXES` to non-actor-scoped endpoints. AC: two keys → independent cache entries. |
| T-15 | R15 | Reduce | **Pin image digests.** Replace mutable tags with `image@sha256:` in compose + helm; CI check that manifests resolve. AC: `docker compose config` resolves every image; CI fails on missing digest. |
| T-16 | R18 | Reduce | **Detection content integrity.** Ship detection-as-code/BAS read-only at runtime; promotion path = PR → CI-signed artifact → PVC sync. AC: in-cluster write to content dir is denied by mount mode. |
| T-17 | R2 | Reduce | **Close NatsSink bypass.** Route all `ares.*` producers (activity, bas, vuln) through `agent_core::NatsSink` so JetStream durable publish applies uniformly. AC: kill JetStream → producers get publish errors, not silent core-NATS sends. |

Accepted without tickets (documented): R9 residual (egress is intrinsic to cloud LLM
use — owner: deployment), R14 residual if T-14 lands.

## 5. Validation triggers

Re-run this model when: a new connector/tool executor lands (new creds or trust
boundary); a new data class enters Postgres/OpenSearch; auth model changes
(JWT/OAuth replaces keys); a new inbound listener is added; an incident occurs —
feed the miss back into the grid.
