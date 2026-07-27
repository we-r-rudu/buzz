#!/bin/bash
# Rename the personal build (or set its identity for a fresh slug).
# Usage: rename-app.sh <product-name> [identifier]
#
# Edits exactly the display-name surfaces and NOTHING else:
#   - desktop/src-tauri/tauri.conf.json  productName (+ identifier when given)
#   - desktop/src-tauri/Info.plist       CFBundleName, CFBundleDisplayName,
#                                        and the 3 TCC permission descriptions
# Deliberately untouched: identifier (unless passed) and
# app_state_keyring.rs — a rename changes the display, never the identity.
# Idempotent. Commit the result as a deploy-only hunk (FORK.md).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
NAME="${1:?usage: rename-app.sh <product-name> [identifier]}"
CONF=desktop/src-tauri/tauri.conf.json
PLIST=desktop/src-tauri/Info.plist
PB=/usr/libexec/PlistBuddy

tmp=$(mktemp)
if [[ $# -ge 2 ]]; then
  jq --arg n "$NAME" --arg id "$2" '.productName=$n | .identifier=$id' "$CONF" > "$tmp"
else
  jq --arg n "$NAME" '.productName=$n' "$CONF" > "$tmp"
fi
mv "$tmp" "$CONF"

$PB -c "Set :CFBundleName $NAME" "$PLIST"
$PB -c "Set :CFBundleDisplayName $NAME" "$PLIST"
for key in NSMicrophoneUsageDescription NSCameraUsageDescription NSLocalNetworkUsageDescription; do
  cur=$($PB -c "Print :$key" "$PLIST")
  # Permission sentences lead with the app name — swap the first word.
  new=$(sed "s/^[^ ]* /$NAME /" <<<"$cur")
  $PB -c "Set :$key $new" "$PLIST"
done

if git diff --quiet -- "$CONF" "$PLIST"; then
  echo "no changes — already named '$NAME'."
else
  git diff --stat -- "$CONF" "$PLIST"
  echo "renamed to '$NAME'. Identifier and keychain service untouched."
  echo "Commit as a deploy-only hunk (never lands on main/PRs — FORK.md)."
fi
