#!/bin/bash
# Restore the personal build's Ruduzz display name (or set a fresh slug identity).
# Usage: rename-app.sh [identifier]
#
# Edits exactly the display-name surfaces and NOTHING else:
#   - desktop/src-tauri/tauri.conf.json  productName=Ruduzz (+ identifier when given)
#   - desktop/src-tauri/Info.plist       CFBundleName, CFBundleDisplayName,
#                                        and the 3 TCC permission descriptions
# Deliberately untouched: identifier (unless passed) and
# app_state_keyring.rs — a rename changes the display, never the identity.
# Idempotent. Commit the result as a deploy-only hunk (FORK.md).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
[[ $# -le 1 ]] || { echo "usage: rename-app.sh [identifier]"; exit 2; }
NAME=Ruduzz
CONF=desktop/src-tauri/tauri.conf.json
PLIST=desktop/src-tauri/Info.plist
PB=/usr/libexec/PlistBuddy
changed=0

if [[ "$(jq -r .productName "$CONF")" != "$NAME" ]] ||
  [[ $# -eq 1 && "$(jq -r .identifier "$CONF")" != "$1" ]]; then
  tmp=$(mktemp)
  if [[ $# -eq 1 ]]; then
    jq --arg n "$NAME" --arg id "$1" '.productName=$n | .identifier=$id' "$CONF" > "$tmp"
  else
    jq --arg n "$NAME" '.productName=$n' "$CONF" > "$tmp"
  fi
  mv "$tmp" "$CONF"
  changed=1
fi

for key in CFBundleName CFBundleDisplayName; do
  if [[ "$($PB -c "Print :$key" "$PLIST")" != "$NAME" ]]; then
    $PB -c "Set :$key $NAME" "$PLIST"
    changed=1
  fi
done
for key in NSMicrophoneUsageDescription NSCameraUsageDescription NSLocalNetworkUsageDescription; do
  cur=$($PB -c "Print :$key" "$PLIST")
  # Permission sentences lead with the app name — swap the first word.
  new=$(sed "s/^[^ ]* /$NAME /" <<<"$cur")
  if [[ "$cur" != "$new" ]]; then
    $PB -c "Set :$key $new" "$PLIST"
    changed=1
  fi
done

if [[ $changed -eq 0 ]]; then
  echo "NO_CHANGE — already named '$NAME'."
else
  git diff --stat -- "$CONF" "$PLIST"
  echo "RENAMED — app named '$NAME'. Identifier and keychain service untouched."
  echo "Commit as a deploy-only hunk (never lands on main/PRs — FORK.md)."
fi
