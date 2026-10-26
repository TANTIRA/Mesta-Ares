# Policy engine — negative test matrix

Every case below MUST be denied by the policy gate (`tool-server` +
`policies/tools.yml` + `policies/protected-assets.yml`). A "pass" for these
tests means the tool call is rejected before execution and the denial is
written to the audit log.

| # | Tool call | Input | Expected denial reason |
|---|-----------|-------|------------------------|
| 1 | `nonexistent.tool` | `{}` | Not on the allowlist (deny-by-default) |
| 2 | `host.shutdown` | `{agent_id:"001"}` | Explicit `autonomy: deny` |
| 3 | `user.delete` | `{user:"jsmith"}` | Explicit `autonomy: deny` |
| 4 | `siem.index.delete` | `{index:"alerts-*"}` | Explicit `autonomy: deny` |
| 5 | `blocklist.ip.add` | `{ip:"10.4.1.20"}` | `forbidden_args`: RFC1918 / protected CIDR |
| 6 | `blocklist.ip.add` | `{ip:"8.8.8.8"}` (no `duration_minutes` ok — optional) but `{ip:"bad"` | `args_schema` violation (not IPv4) |
| 7 | `blocklist.ip.add` | `{ip:"8.8.8.8", duration_minutes:99999}` | `args_schema` violation (> max 10080) |
| 8 | `firewall.rule.add` | `{direction:"inbound", remote:"10.0.0.0/8", action:"deny", duration_minutes:60}` | Protected CIDR in `protected-assets.yml` |
| 9 | `firewall.rule.add` | `{direction:"inbound", remote:"1.2.3.4", action:"allow", duration_minutes:60}` | `args_schema` enum violation (only `deny`) |
| 10 | `wazuh.active_response.isolate_host` | `{agent_id:"<domain-controller>", duration_minutes:60}` | Target in protected `host_classes` |
| 11 | `entra.user.disable` | `{user_id:"<Administrator uuid>"}` | Protected user pattern (`Administrator`) |
| 12 | `ad.user.disable` | `{sam_account_name:"krbtgt"}` | Protected user pattern |
| 13 | `wazuh.active_response.isolate_host` x6 in one hour | distinct unprotected hosts | Blast radius `max_per_hour: 5` exceeded |
| 14 | `wazuh.active_response.isolate_host` | same host twice within 10 min | `cooldown_minutes: 10` |
| 15 | any write/high tool | target = `tool-server`/`orchestrator`/`api-server`/`policy-engine`/`wazuh-manager` | Control-plane `self.services` protected |
| 16 | `wazuh.active_response.kill_process` | `{agent_id:"001"}` (missing `pid`) | `args_schema` required-field violation |
| 17 | `detection.sigma.deploy` | `{rule_path:"../etc/passwd"}` | `args_schema` pattern violation (`^detection-content/`) |
| 18 | `nvd.lookup_cve` | `{cve_id:"not-a-cve"}` | `args_schema` pattern violation (CVE id format) |
| 19 | `cvss.score` | `{vector:"AV:N/AC:L"}` | `args_schema` pattern violation (`^CVSS:(3.[01]|4.0)/`) |

## How to run (when the harness lands)

Policy tests will execute against a `tool-server` started with a test
policy dir. Each row is a JSON-RPC tool call; assert HTTP 403 + structured
denial reason + audit record. Until then this matrix is the spec.
