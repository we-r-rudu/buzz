#!/bin/bash
# Check whether the VPS buzz-acp binary is stale relative to a local ref.
# Usage: check-vps-update.sh [ref]   (default ref: current branch)
#
# The VPS runs only buzz-acp, so staleness = changes in its workspace
# dependency closure (computed dynamically via cargo metadata) plus
# Cargo.lock / rust-toolchain.toml. The VPS is stamped at build time with
# the exact commit it compiled (/opt/buzz-src/.built-commit).
# Exit 0 = up to date, 1 = stale or unknown baseline, 2 = cannot determine.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
REF="${1:-$(git branch --show-current)}"

STAMP=$(ssh dn 'cat /opt/buzz-src/.built-commit' 2>/dev/null) \
  || { echo "cannot read VPS build stamp — is dn reachable and was it ever deployed?"; exit 2; }
STAMP=${STAMP%%[^0-9a-f]*}   # trim any trailing whitespace/newline

if ! git cat-file -e "$STAMP" 2>/dev/null; then
  echo "VPS build commit $STAMP is unknown to this repo — redeploy to establish a baseline."
  exit 1
fi
if ! git merge-base --is-ancestor "$STAMP" "$REF" 2>/dev/null; then
  echo "VPS build commit $STAMP is not an ancestor of $REF (history rewritten since build) — redeploy to be safe."
  exit 1
fi

# Dependency closure of buzz-acp across workspace path deps (dynamic).
. ./bin/activate-hermit
CLOSURE=$(cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c "
import json, sys, os
meta = json.load(sys.stdin)
pkgs = {p['name']: p for p in meta['packages'] if p['source'] is None}
seen, frontier = set(), {'buzz-acp'}
while frontier:
    seen |= frontier
    frontier = {d['name'] for f in frontier for d in pkgs[f]['dependencies'] if d.get('path')} - seen
for name in sorted(seen):
    print(os.path.relpath(os.path.dirname(pkgs[name]['manifest_path']), '.'))
" 2>/dev/null) || CLOSURE=""
# Fallback: last-known closure if metadata fails.
CLOSURE=${CLOSURE:-"crates/buzz-acp
crates/buzz-core
crates/buzz-persona
crates/buzz-sdk"}

CHANGES=$(git log --oneline "$STAMP..$REF" -- $CLOSURE Cargo.lock rust-toolchain.toml)
if [[ -z "$CHANGES" ]]; then
  echo "VPS is up to date with $REF (build: ${STAMP:0:12})."
  exit 0
else
  echo "VPS is STALE vs $REF (build: ${STAMP:0:12}). Relevant commits:"
  echo "$CHANGES" | sed 's/^/  /'
  echo
  echo "Run redeploy-vps.sh to rebuild and restart the agents."
  exit 1
fi
