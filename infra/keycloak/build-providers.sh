#!/usr/bin/env bash
# Populate infra/keycloak/providers/ with every third-party Keycloak
# provider jar + the keycloakify "ares" theme jar.
#
# Where an upstream publishes a release jar we download it (fast,
# deterministic); where it doesn't we build from source (mvn/gradlew).
# Requires curl; for source builds: git, mvn (Java 17+), node/npm.
set -euo pipefail
cd "$(dirname "$0")"

OUT=providers
BUILD=.provider-build
rm -rf "$OUT"
mkdir -p "$BUILD" "$OUT"

KC_VERSION="${KC_VERSION:-26.8.1}"   # must match KEYCLOAK_VERSION in Dockerfile

# ---- released jars -----------------------------------------------------------
dl() { # url dest
  echo "==> $2"
  curl -fsSL "$1" -o "$OUT/$2"
}

# mabartos/keycloak-adaptive-authn — main extension + bundled theme + a
# ready-made adaptive realm (kept next to realm-ares.json as a reference;
# import it by mounting like the other realm file if wanted).
dl https://github.com/mabartos/keycloak-adaptive-authn/releases/download/"${ADAPTIVE_AUTHN_REF:-1.0.4}"/keycloak-adaptive-authn-"${ADAPTIVE_AUTHN_REF:-1.0.4}".jar keycloak-adaptive-authn.jar
dl https://github.com/mabartos/keycloak-adaptive-authn/releases/download/"${ADAPTIVE_AUTHN_REF:-1.0.4}"/adaptive-authn-theme-"${ADAPTIVE_AUTHN_REF:-1.0.4}".jar adaptive-authn-theme.jar
curl -fsSL "https://github.com/mabartos/keycloak-adaptive-authn/releases/download/${ADAPTIVE_AUTHN_REF:-1.0.4}/adaptive-realm-${ADAPTIVE_AUTHN_REF:-1.0.4}.json" -o adaptive-realm.json

dl https://github.com/schwebke/cap-captcha-keycloak/releases/download/"${CAP_CAPTCHA_REF:-26.8.0.r1}"/cap-captcha-keycloak.jar cap-captcha-keycloak.jar
dl https://github.com/schwebke/cap-captcha-keycloak/releases/download/"${CAP_CAPTCHA_REF:-26.8.0.r1}"/cap-captcha-theme.jar cap-captcha-theme.jar
dl https://github.com/B2-Code/Keycloak-GeoAware/releases/download/"${GEOAWARE_REF:-v1.1.0}"/keycloak-geoaware.jar keycloak-geoaware.jar
dl https://github.com/Metatavu/keycloak-scim-server/releases/download/"${SCIM_REF:-v1.8.0}"/keycloak-scim-server-"${SCIM_REF#v}".jar keycloak-scim-server.jar
dl https://github.com/anarsultanov/keycloak-multi-tenancy/releases/download/"${MULTI_TENANCY_REF:-26.1.1}"/keycloak-multi-tenancy-"${MULTI_TENANCY_REF:-26.1.1}".jar keycloak-multi-tenancy.jar
# GitLab generic-package release asset.
dl "https://gitlab.com/api/v4/projects/81869473/packages/generic/keycloak-pow/${POW_REF:-v2.0.1}/keycloak-pow.jar" keycloak-pow.jar

# ---- source builds (no published artifacts) ----------------------------------
fetch() { # repo ref dest — tarball, no history needed
  local repo="$1" ref="$2" dest="$3" path url
  path="${repo#*://*/}"; path="${path%.git}"
  case "$repo" in
    *github.com*) url="https://codeload.github.com/$path/tar.gz/$ref" ;;
    *gitlab.com*) url="https://gitlab.com/$path/-/archive/$ref/$(basename "$path")-$ref.tar.gz" ;;
    *) return 1 ;;
  esac
  mkdir -p "$dest"
  curl -fsSL "$url" | tar xz --strip-components=1 -C "$dest"
}

build_src() { # name repo ref
  local name="$1" repo="$2" ref="$3"
  echo "==> $name @ $ref (source build)"
  [ -f "$BUILD/$name/pom.xml" ] || [ -f "$BUILD/$name/build.gradle.kts" ] || [ -f "$BUILD/$name/build.gradle" ] \
    || fetch "$repo" "$ref" "$BUILD/$name"
  (cd "$BUILD/$name" && {
    if [ -f build.gradle.kts ] || [ -f build.gradle ]; then
      ./gradlew -q build -x test
    else
      mvn -q -B -Dmaven.test.skip=true -Dkeycloak.version="$KC_VERSION" -Dversion.keycloak="$KC_VERSION" package
    fi
  })
  find "$BUILD/$name" \( -path "*/target/*.jar" -o -path "*/build/libs/*.jar" \) \
    ! -name "*-sources.jar" ! -name "*-javadoc.jar" \
    ! -name "original-*.jar" ! -name "*-tests.jar" \
    -exec cp {} "$OUT/" \;
}

# Nordix publishes no jar assets — always source.
build_src keycloak-secrets-vault-provider \
  https://github.com/Nordix/keycloak-secrets-vault-provider.git "${SECRETS_VAULT_REF:-main}"

# Optional: force source builds for released extensions too (e.g. to test a
# branch) — run e.g. `BUILD_FROM_SOURCE="scim multi-tenancy" ./build-providers.sh`
for name in ${BUILD_FROM_SOURCE:-}; do :; done

# ---- keycloakify theme -------------------------------------------------------
# keycloakify itself is the build toolchain (keycloakify/keycloakify); the
# consumption path is a starter checkout + our overrides -> theme jar.
echo "==> keycloakify theme @ ${KEYCLOAKIFY_STARTER_REF:-main}"
[ -f "$BUILD/theme/package.json" ] \
  || fetch "https://github.com/keycloakify/keycloakify-starter.git" "${KEYCLOAKIFY_STARTER_REF:-main}" "$BUILD/theme"
cp -rf theme-overrides/. "$BUILD/theme/"
(cd "$BUILD/theme" && npm install --no-audit --no-fund && npm run build-keycloak-theme)
# keycloakify emits one jar per Keycloak line; 26.x uses the catch-all one.
cp "$BUILD/theme/dist_keycloak/keycloak-theme-for-kc-all-other-versions.jar" "$OUT/ares-theme.jar"

echo "==> providers/:"
ls -1 "$OUT"
