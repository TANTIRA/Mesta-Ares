# BAS — breach & attack simulation
> **Note:** `bas-004` (HTTP beacon, T1071.001) is expected-red: no network-traffic telemetry exists yet (no Suricata/Zeek source). It stays as a tracking canary for the NTA gap.

`bas-runner` replays Atomic Red Team-style scenarios against sacrificial lab
endpoints and scores whether the pipeline *detected* the behavior. It closes
the loop: simulations only mean anything if a SIEM alert fires and triage
agrees.

## Scenario manifest

One YAML file per scenario in `scenarios/`:

| Field | Meaning |
|-------|---------|
| `id` | Stable identifier (`bas-NNN`). Referenced in coverage reports. |
| `name` | Human-readable title. |
| `techniques` | MITRE ATT&CK technique IDs the scenario exercises. |
| `target` | Host class/pool the runner picks a victim from (e.g. `windows-endpoint`). |
| `executor.platform` | `windows` or `linux` — selects the runner's shell channel. |
| `executor.command` | The command line. Must be self-cleaning or lab-safe. |

Current coverage: T1003.001, T1059.001, T1547.001, T1071.001, T1027.

## Flow

1. `bas-runner` loads `scenarios/` (mounted read-only at `/bas` in compose).
2. Selects a target host by `target` class, executes via SSH/WinRM (or a
   Wazuh agent command channel — whichever lands first).
3. Watches OpenSearch for the alert and the triage-agent verdict within a
   time budget, then writes a coverage record to Postgres
   (scenario → detected? → which rule/monitor → triage verdict).
4. Gaps become detection-agent tasks: draft a Sigma rule into
   `detection-content/pending/`.

## Rules of engagement

- Scenarios run only against dedicated lab hosts — never against assets in
  `policies/protected-assets.yml` (enforced by the same policy gate as
  every other tool call).
- Commands must be idempotent/self-cleaning; assume they will be re-run.
- No real malware. Harmless payloads with the right *behavioral shape*
  (encoded commands, run-key writes, fixed-interval egress).
