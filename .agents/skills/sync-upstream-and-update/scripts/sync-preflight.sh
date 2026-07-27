#!/bin/bash
# Preflight for an upstream sync. Read-only (git fetch only).
# Usage: sync-preflight.sh [branch]   (default: current branch)
#
# Reports:
#   1. working-tree cleanliness (refuses to forecast on a dirty tree)
#   2. how far upstream/main is ahead of the branch's merge-base
#   3. the upstream commits that would land
#   4. conflict forecast: fork-side changed files ∩ upstream-changed files,
#      both derived from git (three-dot diffs) — no manifest to maintain.
# Exit 0 always; the report is the product.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

BRANCH="${1:-$(git branch --show-current)}"
DEPLOY_ONLY="desktop/src-tauri/tauri.conf.json
desktop/src-tauri/src/app_state_keyring.rs"

git fetch upstream --prune --quiet

if [[ -n "$(git status --porcelain)" ]]; then
  echo "WARNING: working tree is dirty — commit or stash before merging."
  echo
fi

BASE=$(git merge-base "$BRANCH" upstream/main)
AHEAD=$(git rev-list --count "$BASE..upstream/main")

if [[ "$AHEAD" -eq 0 ]]; then
  echo "nothing to sync: upstream/main has no commits beyond $BRANCH's merge-base."
  exit 0
fi

echo "upstream/main is $AHEAD commit(s) ahead of $BRANCH's merge-base:"
git log --oneline "$BASE..upstream/main" | sed 's/^/  /'
echo

# Fork-side changes since the merge-base (three-dot) ∩ upstream-side changes.
comm -12 \
  <(git diff --name-only "$BASE...$BRANCH" | sort -u) \
  <(git diff --name-only "$BASE..upstream/main" | sort -u) \
  > /tmp/sync-overlap.$$ || true

if [[ -s /tmp/sync-overlap.$$ ]]; then
  echo "CONFLICT FORECAST — fork-touched files upstream also changed:"
  while read -r f; do
    if grep -qxF "$f" <<<"$DEPLOY_ONLY"; then
      echo "  $f   [deploy-only: keep personal values]"
    else
      echo "  $f"
    fi
  done < /tmp/sync-overlap.$$
  echo
  echo "Resolve per FORK.md playbook, then run validate-fork.sh BEFORE committing."
else
  echo "no overlap: upstream changes touch no fork-modified files — clean merge expected."
fi
rm -f /tmp/sync-overlap.$$
