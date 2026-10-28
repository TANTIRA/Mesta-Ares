#!/bin/sh
# Generates dev API keys into .env (idempotent: never overwrites set values).
#
#   sh infra/gen-dev-keys.sh
#
# Produces:
#   ARES_API_KEYS       — internal + console callers, one approver-role key
#   ARES_TOOL_KEY       — the key agents present to tool-server
#   ARES_CONSOLE_KEY    — the key the console BFF proxy injects
#   GATEWAY_API_KEYS    — edge gateway keys
#
# Key spec: `key:actor[:role],...` — `approver` role is required to decide
# tool-server approvals; self-approval is denied regardless of role.

set -eu

ENV_FILE="${1:-.env}"

gen() { LC_ALL=C tr -dc 'a-f0-9' </dev/urandom | head -c 48; }

present() { grep -q "^$1=." "$ENV_FILE" 2>/dev/null; }

touch "$ENV_FILE"

if present ARES_TOOL_KEY && present ARES_CONSOLE_KEY && present ARES_API_KEYS && present GATEWAY_API_KEYS; then
  echo "keys already present in $ENV_FILE — nothing to do"
  exit 0
fi

TOOL_KEY="${ARES_TOOL_KEY:-$(gen)}"
CONSOLE_KEY="${ARES_CONSOLE_KEY:-$(gen)}"
OPERATOR_KEY="${ARES_OPERATOR_KEY:-$(gen)}"
APPROVER_KEY="${ARES_APPROVER_KEY:-$(gen)}"
GATEWAY_KEY="${GATEWAY_API_KEYS:-$(gen)}"

present ARES_API_KEYS ||
  echo "ARES_API_KEYS=${TOOL_KEY}:ares-internal,${OPERATOR_KEY}:dev-operator,${APPROVER_KEY}:dev-approver:approver" >>"$ENV_FILE"
present ARES_TOOL_KEY || echo "ARES_TOOL_KEY=${TOOL_KEY}" >>"$ENV_FILE"
present ARES_CONSOLE_KEY || echo "ARES_CONSOLE_KEY=${CONSOLE_KEY}" >>"$ENV_FILE"
present GATEWAY_API_KEYS || echo "GATEWAY_API_KEYS=${GATEWAY_KEY}" >>"$ENV_FILE"

cat <<EOF
wrote dev keys to $ENV_FILE

  operator key (API/CLI use):   ${OPERATOR_KEY}
  approver key (approvals):     ${APPROVER_KEY}
  gateway key:                  ${GATEWAY_KEY}

Agents and the console pick up ARES_TOOL_KEY / ARES_CONSOLE_KEY from .env
automatically. Set ARES_REQUIRE_AUTH=1 to refuse boot without keys.
EOF
