# Mesta-Ares Keycloak image.
# Provider + theme jars are built on the host by build-providers.sh
# (keeps the image build off Docker Hub — only the quay.io base is pulled).
#
#   ./infra/keycloak/build-providers.sh
#   docker build -f infra/keycloak/Dockerfile -t ares/keycloak:local infra/keycloak

ARG KEYCLOAK_VERSION=26.8.1
FROM quay.io/keycloak/keycloak:${KEYCLOAK_VERSION}
COPY providers/ /opt/keycloak/providers/
RUN /opt/keycloak/bin/kc.sh build
ENTRYPOINT ["/opt/keycloak/bin/kc.sh"]
