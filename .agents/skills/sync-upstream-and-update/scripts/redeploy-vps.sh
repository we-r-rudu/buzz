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
  echo $SHA > /opt/buzz-src/.built-commit"

# Fleet-idle gate: don't kill in-flight turns. Mid-turn agents emit
# turn_liveness (kind 24200) every ~10s; the relay's /query requires Nostr
# auth, so journal activity is the proxy — no buzz-agent log lines in the
# last 15s means idle. Poll every 5s, hard budget 120s, then proceed anyway.
ssh dn 'bash -s' <<'IDLE_GATE'
budget=120
while [ "$budget" -gt 0 ]; do
  n=$(journalctl -u 'buzz-agent@*' --since '-15s' --no-pager -o cat | grep -c . || true)
  if [ "$n" -eq 0 ]; then
    echo FLEET_IDLE
    exit 0
  fi
  echo "fleet busy ($n log lines in last 15s); waiting"
  sleep 5
  budget=$((budget - 5))
done
echo "IDLE_WAIT_TIMEOUT (proceeding)"
IDLE_GATE

ssh dn "systemctl restart 'buzz-agent@*' && echo REDEPLOY_OK"
