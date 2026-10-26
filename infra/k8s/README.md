# Plain Kubernetes manifests

Raw `Deployment`/`Service`/`PVC` YAML for the Ares stack — the same objects
the Helm chart renders, without Helm. Use whichever fits your workflow:

| | `infra/k8s` (this directory) | `infra/helm/ares` |
| - | - | - |
| Templating | none, plain YAML | Helm values + templates |
| Env per service | explicit `env:` blocks | `services.<name>.env` |
| Content ConfigMaps | created by `apply.sh` from the repo | `configVolumes` → ConfigMaps you create |
| Upgrades | `kubectl apply -k` | `helm upgrade` |

## Deploy

```shell
./infra/k8s/apply.sh              # namespace, content ConfigMaps, manifests
./infra/k8s/apply.sh --secrets    # also creates ares-secrets from .env
```

`ares-secrets` must exist before pods start. Required keys: `DATABASE_URL`,
`ANTHROPIC_API_KEY`, provider keys you use, and `POSTGRES_PASSWORD` for the
bundled Postgres.

Images are `ghcr.io/your-org/ares-<service>:0.1.0`. Build them with
`infra/Dockerfile.rust-service` (`--build-arg BIN=<service>`) and
`infra/Dockerfile.console`, or change the registry/tag in `apps.yaml`.

## What runs

- **HTTP services** (`tool-server` :8090, `api-server` :8080,
  `ingestion-gateway` :8091, `console` :3000) get a Service plus
  readiness/liveness probes against `/health` (console: `/`).
- **NATS workers** (`orchestrator`, `triage-agent`, `response-agent`,
  `detection-agent`, `hunt-agent`, `vuln-agent`, `bas-runner`) are Deployments
  with no ports and no probes — they only consume subjects.
- **Datastores** (`postgres`, `nats`, `redis`) are single-replica
  Deployments with PVCs. Dev-grade: use managed Postgres and the NATS
  operator for production.

## Content mounts

The binaries read content from paths relative to the working directory, so
the manifests set `workingDir: /app` and mount ConfigMaps at the exact
subdirectory each one scans:

| Binary | Reads | Mount |
| - | - | - |
| `tool-server` | `POLICIES_DIR=/policies` | `ares-policies` → `/policies` |
| `vuln-agent` | `VULN_WATCHLIST=/policies/vuln-watchlist.yml` | `ares-policies` → `/policies` |
| `api-server` | `detection-content/active` | `ares-detection-active` → `/app/detection-content/active` |
| `bas-runner` | `bas/scenarios` | `ares-bas-scenarios` → `/app/bas/scenarios` |

`detection-agent` **writes** `detection-content/{pending,active}`, and
ConfigMap mounts are read-only. It therefore gets a PVC seeded once by an
init container (`cp -rn`, so promoted rules are never overwritten).

Kustomize refuses to read files outside its own directory, which is why
`apply.sh` creates the four content ConfigMaps with
`kubectl create configmap --from-file=<dir>` and then applies the manifests.
Re-run the script after editing policies or detection content: it re-creates
the ConfigMaps and rolls the five Deployments that mount them.

## Known gaps

- `console` bakes `NEXT_PUBLIC_API_URL` into its bundle at **build** time; the
  env var here only affects server-side rendering. Build the image with the
  browser-reachable API URL.
- `BAS_SCENARIOS_DIR` (used by the Helm chart) is dead code — `bas-runner`
  hardcodes `bas/scenarios`, hence the mount path above.
- No Ingress, HPA, PodDisruptionBudget or NetworkPolicy; add per environment.
