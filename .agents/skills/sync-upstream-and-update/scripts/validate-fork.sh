#!/bin/bash
# The FORK.md validation table as a single gate. Run BEFORE committing any
# sync merge — catches compile breaks textual merges can't see (renamed
# struct fields, moved modules).
# Exit 0 = all green. Output names the failing leg on non-zero.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
. ./bin/activate-hermit

echo "== [1/3] Rust managed_agents suite (omp pins, PATH compose, sweep tables)"
cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents:: 2>&1 \
  | grep -E "^error|test result" | tail -2

echo "== [2/3] Frontend agents tests (agentConfigCore field model)"
(cd desktop && node --import ./test-loader.mjs --experimental-strip-types \
  --test 'src/features/agents/**/*.test.mjs' 2>&1 | grep -E "^ℹ (tests|pass|fail)")

echo "== [3/3] Typecheck"
(cd desktop && pnpm typecheck >/dev/null 2>&1)

echo "VALIDATE_FORK_OK"
