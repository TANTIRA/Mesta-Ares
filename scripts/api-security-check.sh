#!/usr/bin/env bash
# api-security-check.sh — OWASP API Security Top 10 (2023) probes for the
# Mesta-Ares dev stack. Run while `docker compose -f infra/docker-compose.yml`
# is up. READ-ONLY BY DEFAULT: GETs, malformed inputs, method switching,
# oversized bodies (rejected before handlers run), CORS preflight, header
# review. Nothing here authenticates — there is no auth to bypass; a 2xx on a
# sensitive route IS the finding.
#
# Port note: briefs that say "ingestion on :8090" are stale — :8090 is
# tool-server (the policy-gated exec sink); ingestion-gateway is :8091.
#
# Opt-in probes with real downstream side effects (off unless set to 1):
#   PROBE_KILL_SWITCH=1   toggles /controls/kill-switch twice, restores state
#                         (compose caveat: writes only api-server's local
#                         policies/kill — the propagation bug is itself the
#                         finding; on a single-host cargo run it flips the
#                         real shared kill file briefly)
#   PROBE_A2A_SEND=1      one tasks/send to triage-agent -> NATS -> one LLM
#                         verdict call (real token spend, possible TheHive case)
#   PROBE_INGEST=1        one POST /webhook/generic -> real envelope -> triage
#                         cycle (LLM spend, possible case/handoff)
#   PROBE_TOOL_INVOKE=1   POST /invoke with a NONEXISTENT tool — deny-by-default
#                         means nothing executes; writes one audit_log row.
#                         Proves the exec sink is reachable unauthenticated.
#
# Destructive probes intentionally NOT included (would need authz controls +
# a maintenance window): invoking real tools (wazuh.active_response.*,
# entra.user.*, exec.sandbox, blocklist.*), response-agent tasks/send with an
# action-oriented goal, tasks/cancel on a real task id, and sustained load.
#
# Usage: ./scripts/api-security-check.sh  (env: API_HOST, INGEST_HOST, TOOL_HOST)

set -u

API_HOST="${API_HOST:-http://localhost:8080}"
INGEST_HOST="${INGEST_HOST:-http://localhost:8091}"
TOOL_HOST="${TOOL_HOST:-http://localhost:8090}"

CURL="curl -sS -m 10"
FINDINGS=0

section() { printf '\n=== %s ===\n' "$*"; }
info()    { printf '  [info] %s\n' "$*"; }
ok()      { printf '  [ok]   %s\n' "$*"; }
finding() { printf '  [FINDING] %s\n' "$*"; FINDINGS=$((FINDINGS + 1)); }

# code <method> <url> [curl args...] -> prints status code
code() { local m="$1" u="$2"; shift 2; $CURL -o /dev/null -w '%{http_code}' -X "$m" "$u" "$@" 2>/dev/null; }

# unauth <method> <url> <what> [curl args...] -> 2xx on a sensitive route = finding
unauth() {
  local m="$1" u="$2" what="$3"; shift 3
  local c; c="$(code "$m" "$u" "$@")"
  case "$c" in
    2*) finding "$what: $m $u -> $c with no credentials" ;;
    401|403) ok "$what: $m $u -> $c (auth enforced)" ;;
    *) info "$what: $m $u -> $c" ;;
  esac
}

command -v curl >/dev/null || { echo "curl required"; exit 1; }
echo "api-security-check — api=$API_HOST ingest=$INGEST_HOST tool=$TOOL_HOST"
echo "opt-in side-effect probes: PROBE_KILL_SWITCH PROBE_A2A_SEND PROBE_INGEST PROBE_TOOL_INVOKE"

# ---------------------------------------------------------------- 0. liveness
section "0. Liveness / recon"
unauth GET "$API_HOST/health" "api-server health"
unauth GET "$INGEST_HOST/health" "ingestion health"
unauth GET "$TOOL_HOST/health" "tool-server health"
unauth GET "$TOOL_HOST/tools" "tool-server allowlist disclosure"

# ------------------------------------------------- API2: authn / method matrix
section "API2: authentication & method switching (all requests unauthenticated)"
unauth GET  "$API_HOST/alerts"        "alert search"
unauth GET  "$API_HOST/audit"         "audit log read"
unauth GET  "$API_HOST/metrics"       "metrics"
unauth GET  "$API_HOST/cases"         "case list"
unauth GET  "$API_HOST/coverage"      "ATT&CK coverage"
unauth GET  "$API_HOST/playbooks"     "playbook list"
unauth GET  "$API_HOST/controls/kill-switch" "kill-switch state read"
for m in PUT PATCH DELETE POST; do
  c="$(code "$m" "$API_HOST/alerts" -H 'content-type: application/json' -d '{}')"
  info "method switch $m /alerts -> $c (405 expected)"
done
c="$(code GET "$API_HOST/cvss/score")";            info "GET /cvss/score -> $c (405 expected)"
c="$(code PUT "$API_HOST/controls/kill-switch" -H 'content-type: application/json' -d '{}')"
info "PUT /controls/kill-switch -> $c (405 expected)"
c="$(code POST "$INGEST_HOST/webhook/generic" -X POST -H 'content-type: text/plain' -d 'not json')"
info "POST /webhook/generic malformed -> $c (400/415 expected; anything else publishes)"

# ------------------------------------------------------------- API1: BOLA fuzz
section "API1: BOLA — direct object lookups, arbitrary ids"
for id in 1 test admin "00000000-0000-0000-0000-000000000000"; do
  unauth GET "$API_HOST/alerts/$id" "alert by id '$id'"
  unauth GET "$API_HOST/cases/$id"  "case by id '$id'"
  unauth GET "$API_HOST/plans/$id"  "response plan for case '$id'"
done
# A2A task read/cancel by arbitrary id (JSON-RPC, POST /a2a/{agent})
RPC_GET='{"jsonrpc":"2.0","id":"secprobe","method":"tasks/get","params":{"id":"1"}}'
RPC_CANCEL='{"jsonrpc":"2.0","id":"secprobe","method":"tasks/cancel","params":{"id":"no-such-task-secprobe"}}'
for agent in triage-agent response-agent detection-agent hunt-agent; do
  unauth POST "$API_HOST/a2a/$agent" "a2a tasks/get on $agent" \
    -H 'content-type: application/json' -d "$RPC_GET"
done
unauth POST "$API_HOST/a2a/triage-agent" "a2a tasks/cancel (random id)" \
  -H 'content-type: application/json' -d "$RPC_CANCEL"
info "tasks/cancel on a random id mutates a2a_tasks if it exists — unauthenticated task cancellation"
# Per-task SSE (bounded read; times out by design on a live task)
c="$($CURL -o /dev/null -w '%{http_code}' --max-time 3 -N "$API_HOST/a2a/tasks/1/events" 2>/dev/null)"
info "GET /a2a/tasks/1/events -> ${c:-timeout/stream} (404 for unknown id; 200 = readable stream)"
# Playbook name is matched against doc names, not paths — probe traversal anyway
c="$(code GET "$API_HOST/playbooks/..%2F..%2Fetc%2Fpasswd")"
info "GET /playbooks/..%2F..%2Fetc%2Fpasswd -> $c (404 expected — name lookup, no fs read of param)"
c="$(code GET "$API_HOST/playbooks/..%2F..%2Fpolicies%2Ftools")"
info "GET /playbooks traversal to policies -> $c (404 expected)"

# --------------------------------------- API3: property-level / shape abuse
section "API3: mass-assignment / property abuse"
unauth POST "$API_HOST/a2a/triage-agent" "tasks/send missing message (should 4xx)" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tasks/send","params":{"id":"secprobe-nomsg"}}'
unauth POST "$API_HOST/cvss/score" "cvss non-string vector" \
  -H 'content-type: application/json' -d '{"vector":{"x":1}}'
unauth POST "$API_HOST/cvss/score" "cvss malformed vector" \
  -H 'content-type: application/json' -d '{"vector":"CVSS:3.1/AV:N/AC:L/../../../etc/passwd"}'
unauth POST "$API_HOST/cvss/score" "cvss unknown top-level props ignored?" \
  -H 'content-type: application/json' -d '{"vector":"CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:H/SI:H/SA:H","admin":true,"role":"ops"}'

# --------------------------------------------- API4: resource consumption
section "API4: resource consumption"
BIG="$(mktemp -t ares-big-body.XXXXXX)"
{ printf '{"pad":"'; head -c 3000000 /dev/zero | tr '\0' 'A'; printf '"}'; } > "$BIG"
c="$(code POST "$INGEST_HOST/webhook/generic" -H 'content-type: application/json' --data-binary "@$BIG")"
info "3MB body -> ingestion /webhook/generic: $c (413 = axum 2MiB default cap; rejected before publish)"
c="$(code POST "$API_HOST/cvss/score" -H 'content-type: application/json' --data-binary "@$BIG")"
info "3MB body -> /cvss/score: $c (413 expected)"
c="$(code POST "$API_HOST/a2a/triage-agent" -H 'content-type: application/json' --data-binary "@$BIG")"
info "3MB body -> /a2a/triage-agent: $c (413 expected)"
rm -f "$BIG"
info "pagination caps: /alerts limit<=500, /audit limit<=1000, /vulns days<=120 — verify below"
unauth GET "$API_HOST/alerts?limit=999999" "alerts limit=999999 (server caps at 500)"
unauth GET "$API_HOST/audit?limit=999999"  "audit limit=999999 (server caps at 1000)"
# NVD fan-out: serialized behind >=6s upstream mutex — one request parks connections
t0=$(date +%s); c="$(code GET "$API_HOST/vulns?days=1")"; t1=$(date +%s)
info "GET /vulns?days=1 -> $c in $((t1-t0))s (slow = NVD mutex; N parallel requests queue => conn exhaustion)"
info "rate limiting: none implemented — burst of 10 to /metrics should NOT 429:"
for i in $(seq 1 10); do printf '  %s' "$(code GET "$API_HOST/metrics")"; done; echo
unauth GET "$API_HOST/vulns/NOT-A-CVE" "malformed CVE id (400 = input validation present)"
unauth GET "$API_HOST/vulns/CVE-2021-44228/history" "cve history (NVD-backed)"

# --------------------------------------- API5: function-level authorization
section "API5: function-level authz — control & command surfaces"
info "GET /controls/kill-switch already probed above (unauth state read = finding)"
if [ "${PROBE_KILL_SWITCH:-0}" = "1" ]; then
  before="$($CURL "$API_HOST/controls/kill-switch")"
  info "kill-switch before: $before"
  unauth POST "$API_HOST/controls/kill-switch" "engage (no body needed — defaults engaged)" -d ''
  unauth POST "$API_HOST/controls/kill-switch" "toggle via JSON" \
    -H 'content-type: application/json' -d '{"engaged":false}'
  unauth POST "$API_HOST/controls/kill-switch" "restore engaged" \
    -H 'content-type: application/json' -d '{"engaged":true}'
  # restore original
  case "$before" in *true*)  $CURL -o /dev/null -X POST "$API_HOST/controls/kill-switch" -H 'content-type: application/json' -d '{"engaged":true}' ;;
                   *false*) $CURL -o /dev/null -X POST "$API_HOST/controls/kill-switch" -H 'content-type: application/json' -d '{"engaged":false}' ;; esac
  info "kill-switch restored to prior state"
else
  info "skipping kill-switch POST (set PROBE_KILL_SWITCH=1; engages/disengages the real control)"
fi
if [ "${PROBE_A2A_SEND:-0}" = "1" ]; then
  unauth POST "$API_HOST/a2a/triage-agent" "tasks/send -> real agent task (LLM spend)" \
    -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":"secprobe","method":"tasks/send","params":{"message":{"role":"user","parts":[{"kind":"text","text":"security test task; no IOCs; do not escalate"}]}}}'
  info "NOT probed: tasks/send to response-agent — hands a goal to the Strong-tier"
  info "planner which emits real tool calls through /invoke (policy-gated but live)."
else
  info "skipping tasks/send (set PROBE_A2A_SEND=1; spends LLM tokens, may open cases)"
fi

# -------------------------------------------------------------- API7: SSRF-ish
section "API7: SSRF / arg-steered internal requests"
if [ "${PROBE_TOOL_INVOKE:-0}" = "1" ]; then
  # deny-by-default: a bogus tool can never execute; proves unauth reach of
  # the policy gate. Writes ONE audit_log row.
  unauth POST "$TOOL_HOST/invoke" "invoke nonexistent tool (deny proves reach)" \
    -H 'content-type: application/json' -d '{"id":"11111111-1111-1111-1111-111111111111","tool":"secprobe.nonexistent","args":{},"actor":"api-security-check","idempotency_key":"","correlation_id":"22222222-2222-2222-2222-222222222222"}'
  info "real-tool probes (need authz + window): opensearch.query index=_all q='*' reads any index;"
  info "  thehive.case.update case_id='../<path>' steers the URL path; exec.sandbox bash -c"
  info "  with newline/'>' args bypasses FORBIDDEN_SUBSTRINGS (openshell.rs:55)."
else
  info "skipping /invoke probe (set PROBE_TOOL_INVOKE=1; deny-only, writes one audit row)"
fi
info "connector base URLs are env-pinned — no arbitrary-origin SSRF surface found."

# ------------------------------------------------ API8: misconfiguration
section "API8: CORS, security headers, error verbosity"
hdrs="$($CURL -i -X OPTIONS "$API_HOST/controls/kill-switch" \
  -H 'Origin: https://evil.example' -H 'Access-Control-Request-Method: POST' 2>/dev/null)"
echo "$hdrs" | grep -qi 'access-control-allow-origin: \*' \
  && finding "CORS preflight reflects any origin (CorsLayer::permissive, main.rs:339)" \
  || info "CORS preflight: $(echo "$hdrs" | grep -i 'access-control' | head -3 | tr '\n' ' ')"
echo "$hdrs" | grep -qi 'access-control-allow-methods' && info "preflight methods: $(echo "$hdrs" | grep -i 'allow-methods')"
rhdrs="$($CURL -i "$API_HOST/health" 2>/dev/null | sed -n '1,/^$/p')"
for h in content-security-policy x-frame-options x-content-type-options strict-transport-security referrer-policy; do
  echo "$rhdrs" | grep -qi "^$h:" || finding "missing security header on api-server: $h"
done
info "error verbosity: trigger a backend failure and inspect bodies — handlers return"
info "  anyhow/reqwest/sqlx e.to_string() (e.g. GET /cases with TheHive down -> 502 + internals)"
body="$($CURL "$API_HOST/cases" 2>/dev/null)"
printf '  GET /cases body (first 300B): %.300s\n' "$body"
echo "$body" | grep -qiE 'refused|timeout|error|tcp|http://|postgres' \
  && finding "error body leaks upstream/transport detail" \
  || ok "GET /cases returned data (TheHive up) or clean error"

# ------------------------------------------------------- API9: inventory drift
section "API9: inventory — undocumented-but-live and documented-but-dead routes"
for r in /playbooks /vulns /vulns/advisories /metrics /coverage /stream/activity \
         /.well-known/agent.json /a2a/agents; do
  c="$(code GET "$API_HOST$r")"; info "GET $r -> $c"
done
c="$(code POST "$API_HOST/hunt" -H 'content-type: application/json' -d '{"prompt":"x"}')"
info "POST /hunt (console calls it, api.ts:373) -> $c (404 = client/server drift)"
c="$(code POST "$INGEST_HOST/webhook/wazuh" -H 'content-type: application/json' -d 'bad')"
info "ingestion /webhook/wazuh exists (malformed -> ${c}); /webhook/generic is undocumented in services.md"
info "also: no /ingest/* namespace exists despite older briefs; doc says 503 on NATS-down, code returns 502"

# -------------------------------------- API10: unsafe consumption / SSE
section "API10: untrusted data -> LLM & streams"
info "chain (static): POST /webhook/generic or a2a tasks/send text -> verbatim into"
info "  LLM context (triage main.rs:297, response main.rs:247) -> PlannedAction[] -> /invoke"
info "  => prompt injection becomes tool calls; exec.sandbox allows bash -c."
c="$($CURL -o /dev/null -w '%{http_code}' --max-time 3 -N "$API_HOST/stream/activity" 2>/dev/null)"
info "GET /stream/activity -> ${c:-stream-open} (200 = unauthenticated ops feed to any client)"

# ------------------------------------------------------------------- summary
section "SUMMARY"
echo "findings flagged: $FINDINGS"
echo "expected posture today: every 2xx above on a non-health route is unauthenticated access."
echo "re-run after auth lands: same script should report 401/403 for all unauth probes."
