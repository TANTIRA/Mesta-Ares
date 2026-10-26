# HTTP API specifications

OpenAPI 3.0.3 documents for every HTTP surface in the repo.

| File | Service | Default port |
| - | - | - |
| `api-gateway.yaml` | Edge: auth, rate limits, cache, `/aggregate/dashboard`, pass-through | 8082 |
| `api-server.yaml` | Console read models, kill switch, vulns, CVSS, A2A, ACP | 8080 |
| `tool-server.yaml` | Policy-gated tool invocation + approval queue | 8090 |
| `ingestion-gateway.yaml` | Alert webhooks | 8091 |

Render locally:

```shell
npx --yes @redocly/cli preview-docs docs/openapi/api-gateway.yaml
# or
npx --yes @redocly/cli lint docs/openapi/*.yaml
```

`crates/api-gateway` has a test (`specs_are_valid_openapi`) that parses every
file and resolves each internal `$ref`, so a broken spec fails `cargo test`.

## Conventions in force

- **Resources are nouns, methods carry the verb** — `GET /vulns/{cve_id}`,
  `POST /approvals/{id}/approve` (the action sub-resource is the one place a
  verb appears; there is no clean noun for "decide an approval").
- **Status codes** — `200` success, `202` accepted (async publish, ACP `async`
  runs), `400` caller error, `404` absent, `409` conflicting state transition,
  `429` rate limit, `502` upstream dependency failed, `503` gateway auth not
  configured.
- **Pagination** — offset/limit (`?startIndex=`, `?limit=`) for list endpoints;
  `limit` is capped per endpoint (see each parameter's `maximum`).
- **Lists return a wrapper object** when the payload is not a bare array
  (`{"rows": [...]}`, `{"agents": [...]}`), so fields can be added later.
- **`POST /invoke` is HTTP 200 even for denials** — the `decision` field is the
  contract. Do not infer success from the status code on that endpoint.

## Deviations from the house standard

These are known, deliberate gaps against the API-documentation standard. Each
is a behaviour change, not a documentation gap:

| Gap | Current state | Cost to fix |
| - | - | - |
| API versioning | No `/v1` prefix anywhere. | Breaking for the console and any integration; needs a dual-mount window. |
| Response envelopes | Mixed: some endpoints return bare arrays (`/a2a/agents`, `/vulns/advisories`, `/approvals`), others wrappers. | Low per endpoint, but changes client parsing. |

Resolved: **structured error envelope**. All HTTP services now answer failures
as `{"error": {"code", "message", "details"?}}` through
`http_limits::error::ApiError`. The ACP routes keep their protocol-mandated
`{code, message, data}` shape.

The envelope every non-ACP service now emits:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid input",
    "details": [{ "field": "vector", "message": "unrecognised CVSS prefix" }]
  }
}
```
