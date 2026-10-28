# Keycloak (identity plane)

`infra/keycloak/Dockerfile` packages a Keycloak 26.8.1 image with eight
third-party providers and the `ares` keycloakify theme. `build-providers.sh`
downloads release jars where upstream publishes them (adaptive-authn 1.0.4,
cap-captcha 26.8.0.r1, GeoAware v1.1.0, scim-server 1.8.0, multi-tenancy
26.1.1, keycloak-pow v2.0.1), builds the rest from source
(secrets-vault — no release assets), and builds the theme via the
keycloakify starter. `infra/docker-compose.yml` runs it as `keycloak` +
`keycloak-db` (Postgres). Admin console: http://localhost:8180
(`KEYCLOAK_ADMIN` / `KEYCLOAK_ADMIN_PASSWORD` from `.env`).

Boot-time SPI config in compose: the Nordix vault SPIs are disabled until
`VAULT_ADDR`/`VAULT_TOKEN` are set; GeoAware runs the `mock` geoip provider
(dummy MaxMind creds keep its autodownload init happy) until a real mmdb or
license is provided; `SCIM_AUTHENTICATION_MODE` defaults to `KEYCLOAK`.

## Providers

| Extension | Built from | What it adds | Configure |
|---|---|---|---|
| adaptive-authn | mabartos/keycloak-adaptive-authn | Risk/adaptive authn policies (device, location, velocity signals) | Authentication → Flows: add the adaptive executions/policies per upstream README |
| cap-captcha | schwebke/cap-captcha-keycloak | Friendly-CAPTCHA authenticator for login/registration | Set site key + secret in the authenticator config of your flow |
| GeoAware | B2-Code/Keycloak-GeoAware | Geo-velocity/impossible-travel checks via MaxMind | Drop `GeoLite2-City.mmdb` into `infra/keycloak/geoip/`, set `GEOAWARE_DB_PATH` |
| keycloakify theme | keycloakify-starter (keycloakify toolchain) | React `ares` login/account theme | Overrides in `infra/keycloak/theme-overrides/`; realm already selects `ares` |
| secrets-vault | Nordix/keycloak-secrets-vault-provider | Vault SPI — realm secrets resolved from HashiCorp Vault | Set `VAULT_ADDR`/`VAULT_TOKEN` env; map secrets per upstream docs |
| scim-server | Metatavu/keycloak-scim-server | SCIM 2.0 endpoints (`/realms/<r>/scim/v2/*`) for user/group provisioning | Enable per-realm; issue a service token |
| keycloak-pow | gitlab.com/mrbuch/keycloak/keycloak-pow | Proof-of-work authenticator (anti-bot, CPU puzzle before login) | Add the PoW execution to the browser flow |
| multi-tenancy | anarsultanov/keycloak-multi-tenancy | Realm-level multi-tenancy: orgs, invites, per-org IdP | Enable in realm settings; manage under the "Organizations"-style UI added by the jar |

AuthN wiring for the stack: `realm-ares.json` ships client `ares-console`
(public, PKCE, redirect `http://localhost:3000`) and `ares-service`
(service account). Console/api-server OIDC enforcement is the follow-up step —
point them at `http://localhost:8180/realms/ares` when implemented.

## Rebuild after bumping a ref

```sh
docker compose -f infra/docker-compose.yml build keycloak
docker compose -f infra/docker-compose.yml up -d keycloak
```

## QA checklist

- `docker compose -f infra/docker-compose.yml config` renders without error.
- Built image contains one jar per provider: `docker run --rm ares/keycloak ls /opt/keycloak/providers`.
- `kc.sh build` step succeeds inside the Dockerfile (it validates provider manifests).
- Realm `ares` imports: http://localhost:8180 → `ares` realm exists with theme `ares`.
