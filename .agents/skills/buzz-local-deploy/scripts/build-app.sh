#!/bin/bash
# Build the personal app bundle. Long (~5 min) — invoke async.
# NEVER `just desktop-release-build`: it re-stubs sidecars with empty files.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
. ./bin/activate-hermit
cd desktop
CI=true pnpm tauri build --features mesh-llm --target aarch64-apple-darwin 2>&1 | tail -5
ls -d src-tauri/target/aarch64-apple-darwin/release/bundle/macos/*.app
