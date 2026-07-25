---
name: buzz-local-deploy
description: Use when building, updating, or reinstalling a personal locally-installed Buzz desktop app from the we-r-rudu/buzz fork that must coexist with a company-installed Buzz — triggers include "deploy local buzz", "my own buzz build", "personal release", "install my branch", or running a custom unsigned Buzz.app without the upstream release pipeline.
disable-model-invocation: true
---

# Buzz Local Deploy

Build a personal, isolated Buzz desktop app from a branch of this fork and install it locally. The upstream release lane (`just release-desktop`, canary workflow) is **unavailable** — it needs block/buzz signing secrets and publishes publicly. Local `tauri build` is the only path.

## Input

- `<slug>` — instance name (default `rudu`). Drives bundle id, app name, keychain service.

## Isolation model (3 collision surfaces with the company install)

| Surface | Keyed by | Fix |
|---|---|---|
| App-data dir, Launch Services | bundle identifier | custom identifier |
| Nest dir + CLI symlink (`~/.buzz`/`~/.buzz-dev`, `buzz`/`buzz-dev`) | identifier prefix at **runtime** (`is_dev_data_dir_name`, migration.rs) | **dev-prefixed** identifier |
| Keychain service (identity + agent keys) | **compile-time** literal in release builds | code edit — mandatory |

## Procedure

0. **Toolchain.** Run `. ./bin/activate-hermit` first, and prefix any async/background build command with it too — fresh shells do not inherit a previous call's env. All `just`/`cargo`/`pnpm` steps below require it.
1. **Branch.** Ensure a personal branch exists off latest `main` (e.g. `git checkout -b <slug>`). Never clobber uncommitted work; if the branch already has these edits, skip them.
2. **Edit `desktop/src-tauri/tauri.conf.json`:**
   - `"productName": "Buzz <Slug>"`
   - `"identifier": "xyz.block.buzz.app.dev.<slug>"` — the `.dev.` infix is required, not cosmetic. It routes nest → `~/.buzz-dev` and CLI → `buzz-dev`. A plain `xyz.block.buzz.app.<slug>` identifier lands the app in the company install's `~/.buzz` nest and `buzz` CLI symlink.
3. **Edit `desktop/src-tauri/src/app_state_keyring.rs`** — release arm of `keyring_service()`: `"buzz-desktop"` → `"buzz-desktop-<slug>"`. This is the only collision that corrupts state: without it the app adopts the company identity and can clobber its keychain on sign-out/import. Identifier changes do NOT fix this — the service is compile-time hardcoded per build profile.
4. **Optional — real agent sidecars** (skip = agent features dead in the app):
   ```bash
   cargo build --release -p buzz-acp -p buzz-agent -p buzz-dev-mcp -p buzz-cli -p git-credential-nostr
   # copy each into desktop/src-tauri/binaries/<name>-aarch64-apple-darwin
   ```
   Then build via `cd desktop && pnpm tauri build --features mesh-llm --target aarch64-apple-darwin` directly — `just desktop-release-build` re-stubs sidecars with empty files.
5. **Build:** `just desktop-release-build` (long; run async).
   Output: `desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Buzz <Slug>.app`
6. **Install:** copy to `/Applications`. If macOS calls it "damaged" after moving/copying: `xattr -cr "/Applications/Buzz <Slug>.app"`.
7. **Report:** app path, keychain service `buzz-desktop-<slug>`, nest `~/.buzz-dev`, data dir `~/Library/Application Support/xyz.block.buzz.app.dev.<slug>`.

## Updating an existing personal build

Rebase/merge the branch onto `main`, repeat step 5. The two edits rarely conflict with upstream. No auto-updater exists in local builds (`buzz_updater_enabled` is CI-only) — rebuilds are the update mechanism, and the app will never self-update back to stock.

## Zero-code-edit alternative

Debug build: `pnpm tauri build --debug --config <override.json>`. Auto-isolates keychain (`buzz-desktop-dev`, runtime-overridable via `BUZZ_DEV_KEYRING_SERVICE=buzz-desktop-dev.<slug>`) and nest. Cost: debug-profile binary (slower startup/crypto).

## Known safe sharings

- `~/.buzz-dev` nest with disposable `just dev` instances (agents keyed by pubkey inside — contention, not corruption).
- `~/.buzz/models` — hardcoded ML-model cache, shared by all builds.

## Never do

- `just release-desktop` / signed canary / tag-triggered `release.yml` — upstream-only, needs secrets, publishes publicly.
- Identifier without the `.dev.` infix (see step 2).
- Skip step 3 "because the identifier changed" — keychain is compile-time, identifier-independent.
- `gh pr create` without `--repo we-r-rudu/buzz` in this checkout — gh resolves to upstream block/buzz.
