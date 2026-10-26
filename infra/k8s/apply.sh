#!/usr/bin/env bash
# Deploys the Ares stack to the current kubectl context.
#
#   ./infra/k8s/apply.sh            # namespace + content ConfigMaps + manifests
#   ./infra/k8s/apply.sh --secrets  # also creates ares-secrets from .env
#
# Content ConfigMaps are created here rather than by kustomize: kustomize
# refuses to read files outside its own directory, and policies/rules must
# stay single-sourced in the repo.
set -euo pipefail

cd "$(dirname "$0")/../.."
NS=ares

kubectl create namespace "$NS" --dry-run=client -o yaml | kubectl apply -f -

if [[ "${1:-}" == "--secrets" ]]; then
  [[ -f .env ]] || { echo "no .env to read secrets from" >&2; exit 1; }
  kubectl -n "$NS" create secret generic ares-secrets --from-env-file=.env \
    --dry-run=client -o yaml | kubectl apply -f -
fi

# One ConfigMap per source directory: kustomize/kubectl flatten --from-file to
# basenames, so the manifests mount each at the path its binary reads.
for pair in \
  "ares-policies:policies" \
  "ares-detection-active:detection-content/active" \
  "ares-detection-pending:detection-content/pending" \
  "ares-bas-scenarios:bas/scenarios"; do
  name="${pair%%:*}"
  dir="${pair#*:}"
  kubectl -n "$NS" create configmap "$name" --from-file="$dir" \
    --dry-run=client -o yaml | kubectl apply -f -
done

kubectl apply -k infra/k8s

# Pods mount ConfigMaps as files at start; roll the consumers so edited
# policies/rules take effect.
kubectl -n "$NS" rollout restart \
  deploy/ares-tool-server deploy/ares-vuln-agent deploy/ares-detection-agent \
  deploy/ares-api-server deploy/ares-bas-runner
