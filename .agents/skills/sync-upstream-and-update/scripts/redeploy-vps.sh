#!/bin/bash
# Rebuild buzz-acp on the VPS from a local ref, restart the agent fleet,
# and restamp the build commit.
# Usage: redeploy-vps.sh [ref]   (default ref: current branch)
# ~3 min warm (VPS cargo cache), ~15 min cold.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
REF="${1:-$(git branch --show-current)}"
SHA=$(git rev-parse "$REF")
echo "redeploying $REF (${SHA:0:12})"

git archive --format=tar.gz "$REF" \
  Cargo.toml Cargo.lock rust-toolchain.toml crates examples \
  -o /tmp/buzz-vps-src.tar.gz
scp /tmp/buzz-vps-src.tar.gz dn:/tmp/ >/dev/null

ssh dn "rm -rf /opt/buzz-src && mkdir -p /opt/buzz-src && \
  tar xzf /tmp/buzz-vps-src.tar.gz -C /opt/buzz-src && \
  /root/build-buzz-acp.sh >> /var/log/buzz-build.log 2>&1 && \
  echo $SHA > /opt/buzz-src/.built-commit && \
  systemctl restart 'buzz-agent@*' && \
  echo REDEPLOY_OK"
