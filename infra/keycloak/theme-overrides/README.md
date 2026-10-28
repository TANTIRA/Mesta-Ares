# keycloakify theme overrides

Files in this directory are copied over the pinned `keycloakify-starter`
checkout at image build time (see `../Dockerfile`, `theme` stage).

To customize the theme:

- React login/account pages go under `src/` (follows keycloakify-starter layout).
- The theme name is `ares` — set in the starter's `vite.config.ts`
  (`keycloakify({ themeName: "ares" })`). The override below already does that.
- Anything placed here wins over the starter file of the same path.

Built jar lands in `dist_keycloak/` and is installed as
`/opt/keycloak/providers/ares-theme.jar`, so the `ares` theme appears under
Realm settings → Themes.
