#!/bin/bash
# Install the freshly built Ruduzz bundle, replacing any previous install of
# THIS identity — including old-named ones (a rename installs alongside, not
# over). Only apps whose bundle identifier matches ours are removed; the
# company-installed Buzz (different identifier) is never touched.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
CONF=desktop/src-tauri/tauri.conf.json
NAME=$(jq -r .productName "$CONF")
ID=$(jq -r .identifier "$CONF")
BUILT="desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/$NAME.app"
[[ -d "$BUILT" ]] || { echo "no built bundle at $BUILT — run build-app.sh first"; exit 1; }

PB=/usr/libexec/PlistBuddy
shopt -s nullglob
for app in /Applications/*.app; do
  bid=$($PB -c "Print :CFBundleIdentifier" "$app/Contents/Info.plist" 2>/dev/null || true)
  [[ "$bid" == "$ID" ]] || continue
  echo "removing previous install: $app"
  rm -rf "$app"
done

cp -R "$BUILT" /Applications/
xattr -cr "/Applications/$NAME.app"
echo "installed /Applications/$NAME.app"
