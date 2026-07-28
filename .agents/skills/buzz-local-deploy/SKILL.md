---
name: buzz-local-deploy
description: Use when building, updating, restoring, or reinstalling the locally-installed Ruduzz desktop app from the we-r-rudu/buzz fork that must coexist with a company-installed Buzz — triggers include "deploy Ruduzz", "deploy local buzz", "my own buzz build", "personal release", "install my branch", or running the unsigned fork app without the upstream release pipeline.
disable-model-invocation: true
---

# Ruduzz Local Deploy

Build the personal, isolated Ruduzz desktop app from a branch of this fork and install it locally. The upstream release lane (`just release-desktop`, canary workflow) is **unavailable** — it needs block/buzz signing secrets and publishes publicly. Local `tauri build` is the only path.

Every routine step is a script in `scripts/` — invoke them in the order below and watch the results; do not re-derive the commands. Each script self-activates the hermit toolchain, is idempotent, and prints a completion marker. Scripts never edit `app_state_keyring.rs` and never touch the company-installed Buzz (different bundle identifier).

## Isolation model (3 collision surfaces with the company install)

| Surface | Keyed by | Fix |
|---|---|---|
| App-data dir, Launch Services | bundle identifier | custom identifier |
| Nest dir + CLI symlink (`~/.buzz`/`~/.buzz-dev`, `buzz`/`buzz-dev`) | identifier prefix at **runtime** (`is_dev_data_dir_name`, migration.rs) | **dev-prefixed** identifier |
| Keychain service (identity + agent keys) | **compile-time** literal in release builds | code edit — mandatory, once per slug |

## Procedure

0. **Branch.** Personal branch off latest `main` (e.g. `<slug>`). Never clobber uncommitted work.
1. **Name/identity — enforce Ruduzz on every deploy (idempotent):**
   ```bash
   bash .agents/skills/buzz-local-deploy/scripts/rename-app.sh [identifier]
   ```
   Sets `productName` and the `Info.plist` display names to `Ruduzz` (these override `productName` in the menu bar, Dock, and Spotlight). Omit the identifier for an existing slug; pass it only for a fresh slug: `xyz.block.buzz.app.dev.<slug>` — the `.dev.` infix is required, not cosmetic; it routes nest → `~/.buzz-dev` and CLI → `buzz-dev`. **Restoring the Ruduzz name never changes the identifier or keychain service** — changing those orphans app-data and keys. Completion: `NO_CHANGE` or `RENAMED`; commit changed files as a deploy-only hunk (FORK.md).
2. **Keychain service — once per slug, manual code edit** (scripts refuse this one): release arm of `keyring_service()` in `desktop/src-tauri/src/app_state_keyring.rs` → `"buzz-desktop-<slug>"`. Without it the app adopts the company identity and can clobber its keychain on sign-out/import. Identifier changes do NOT fix this — the service is compile-time hardcoded.
3. **Sidecars** (skip only when nothing under `crates/` changed since the last build):
   ```bash
   bash .agents/skills/buzz-local-deploy/scripts/build-sidecars.sh
   ```
   Completion: prints `SIDECARS_OK` with five staged binaries.
4. **Build** (~5 min, run async):
   ```bash
   bash .agents/skills/buzz-local-deploy/scripts/build-app.sh
   ```
   Completion: prints the `Ruduzz.app` bundle path.
5. **Install** — removes every `/Applications` app with our bundle identifier (including old-named ones), copies the fresh bundle to `/Applications/Ruduzz.app`, and strips quarantine:
   ```bash
   bash .agents/skills/buzz-local-deploy/scripts/install-app.sh
   ```
6. **Verify** — the deploy is not done until this passes:
   ```bash
   bash .agents/skills/buzz-local-deploy/scripts/verify-deploy.sh
   ```
   Completion: exit 0, prints `VERIFY_OK` — configured `productName` is exactly `Ruduzz`, bundle/display names match it, identifier and version match `tauri.conf.json`, and all five sidecars are present. Then launch the app and confirm the menu bar shows `Ruduzz` (the check `Info.plist` hardcoding once hid).

## Known safe sharings

- `~/.buzz-dev` nest with disposable `just dev` instances (agents keyed by pubkey inside — contention, not corruption).
- `~/.buzz/models` — hardcoded ML-model cache, shared by all builds.

## Never do

- `just release-desktop` / signed canary / tag-triggered `release.yml` — upstream-only, needs secrets, publishes publicly.
- `just desktop-release-build` — re-stubs sidecars with empty files; `build-app.sh` exists precisely to avoid it.
- Identifier without the `.dev.` infix (see isolation table).
- Skip step 2 "because the identifier changed" — keychain is compile-time, identifier-independent.
- `gh pr create` without `--repo we-r-rudu/buzz` in this checkout — gh resolves to upstream block/buzz.
