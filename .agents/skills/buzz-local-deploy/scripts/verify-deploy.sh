#!/bin/bash
# Verify the installed app matches the source config. The completion gate
# for every deploy — exit 0 (VERIFY_OK) or name each mismatch.
#
# Checks, all against desktop/src-tauri/tauri.conf.json:
#   app exists at /Applications/<productName>.app
#   CFBundleName / CFBundleDisplayName == productName (and != "Buzz")
#   CFBundleIdentifier == identifier (identity continuity)
#   CFBundleShortVersionString == version
#   all five sidecars present and executable in Contents/MacOS
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
CONF=desktop/src-tauri/tauri.conf.json
NAME=$(jq -r .productName "$CONF")
ID=$(jq -r .identifier "$CONF")
VER=$(jq -r .version "$CONF")
APP="/Applications/$NAME.app"
PB=/usr/libexec/PlistBuddy
fail=0
check() { # label expected actual
  if [[ "$2" == "$3" ]]; then
    printf '  ok   %-28s %s\n' "$1" "$3"
  else
    printf '  FAIL %-28s want %s, got %s\n' "$1" "$2" "$3"
    fail=1
  fi
}

[[ -d "$APP" ]] || { echo "MISSING: $APP — run install-app.sh"; exit 1; }
P="$APP/Contents/Info.plist"

check CFBundleName "$NAME" "$($PB -c 'Print :CFBundleName' "$P")"
check CFBundleDisplayName "$NAME" "$($PB -c 'Print :CFBundleDisplayName' "$P")"
check CFBundleIdentifier "$ID" "$($PB -c 'Print :CFBundleIdentifier' "$P")"
check CFBundleShortVersionString "$VER" "$($PB -c 'Print :CFBundleShortVersionString' "$P")"
[[ "$NAME" != "Buzz" ]] || { echo "  FAIL productName is stock 'Buzz' — run rename-app.sh"; fail=1; }

for b in buzz buzz-acp buzz-agent buzz-dev-mcp git-credential-nostr; do
  [[ -x "$APP/Contents/MacOS/$b" ]] && printf '  ok   sidecar %-22s\n' "$b" \
    || { printf '  FAIL sidecar %-22s missing\n' "$b"; fail=1; }
done

[[ $fail -eq 0 ]] && echo "VERIFY_OK" || echo "VERIFY_FAILED"
exit $fail
