#!/bin/bash
# Build the five agent sidecars and stage them for tauri bundling.
# Skipping this when crates/ changed ships a stale buzz-acp inside the app.
# Safe to re-run: cargo is incremental, no-op when fresh.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
. ./bin/activate-hermit
[[ $(uname -m) == arm64 ]] || { echo "this script targets aarch64-apple-darwin only"; exit 1; }

SIDECARS="buzz-acp buzz-agent buzz-dev-mcp buzz-cli git-credential-nostr"
cargo build --release $(printf -- '-p %s ' $SIDECARS)

for b in buzz buzz-acp buzz-agent buzz-dev-mcp git-credential-nostr; do
  # install -m0755: cp over an existing file keeps the destination's old
  # mode, and a 644 sidecar cannot be spawned by the app (EACCES).
  install -m 0755 "target/release/$b" "desktop/src-tauri/binaries/$b-aarch64-apple-darwin"
done
ls -l desktop/src-tauri/binaries/ | awk 'NR>1 && NF {printf "  %s %10d  %s\n", $1, $5, $9}'
echo "SIDECARS_OK"
