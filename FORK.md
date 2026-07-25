# Rudu fork of Buzz

This repository is the Rudu-maintained fork of
[`block/buzz`](https://github.com/block/buzz). It keeps Buzz's protocol and
technical naming unless a Rudu product requirement needs a deliberate change.

Read this before: syncing upstream, solving merge conflicts, adding a
fork-owned change, or cutting a local/personal build.

## Baseline

- Upstream: `https://github.com/block/buzz.git` (`upstream`, push disabled)
- Fork: `https://github.com/we-r-rudu/buzz.git` (`origin`)
- Initial fork base: `acfbb1bb6af54cb29cb152496ff43b8285dcb8cf`

The commit graph is the source of truth for the current fork delta:

```bash
git fetch upstream --prune
git log --oneline upstream/main..origin/main
git diff --stat upstream/main...origin/main
```

All `gh` commands in this checkout need `--repo we-r-rudu/buzz` — gh resolves
to upstream `block/buzz` by default and will file PRs/issues in the wrong repo.

## Branch policy

- `origin/main` is the shippable Rudu fork.
- `upstream/main` is read-only and tracks `block/buzz`.
- Fork work uses short-lived topic branches **off `origin/main`**.
- Upstream updates use `sync/upstream-YYYY-MM-DD` branches and pull requests.
- Feature pull requests may be squashed. Upstream sync pull requests must use a
  merge commit so Git retains upstream ancestry.
- Force pushes and rebases of `main` are not allowed.
- This checkout's local `main` may mirror `upstream/main` directly (bleeding
  edge, ahead of the fork's last sync). Do not build fork work on it — branch
  from `origin/main`, or verify `git log origin/main..main` is empty first.
- Personal deploy branches (e.g. `rudu`) carry the local-build identity edits
  from the deploy skill. They are never pushed for review and never merged
  anywhere; rebase/rebuild is their update mechanism.

## Upstream sync

Sync deliberately — never mid-task on an unrelated branch. Start clean:

```bash
git fetch upstream --prune
git switch -c "sync/upstream-$(date +%Y-%m-%d)" origin/main
git merge --no-ff upstream/main
just ci
git push -u origin HEAD
```

Run `just test` as well when the update touches `buzz-relay`, `buzz-db`, or
`buzz-auth` and Postgres and Redis are available. Then re-validate every
fork-owned change per the table below **before** opening the sync PR.

## Change placement

Use the least invasive extension point that works:

1. Existing workflows, configuration, Nostr events, `buzz-sdk`, or
   `buzz-ws-client`.
2. Fork-owned modules such as `crates/rudu-*`,
   `desktop/src/features/rudu-*`, or `mobile/lib/features/rudu_*`.
3. Small, **additive** wiring changes in shared Buzz registries and entry
   points only when the feature cannot remain additive.

Do not add a speculative plugin framework or globally rename Buzz crates,
protocol identifiers, or `BUZZ_*` environment variables. Generic fixes should
be contributed upstream and removed from the fork delta after they return in
an upstream sync.

## Fork-owned changes and how to validate them

Every entry lists what we added, where it lives, and the exact check that
fails if a conflict resolution dropped it. Run these after any sync merge.

| Change | Files | Validation |
|---|---|---|
| oh-my-pi ACP runtime (`omp acp` as a managed-agent harness) | `desktop/src-tauri/src/managed_agents/discovery.rs` (catalog entry, avatar, `default_agent_args` arm), `…/discovery/tests.rs` | `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::` — pins: `omp_args_default_to_acp_subcommand`, `resolves_omp_avatar_and_runtime`, `omp_install_commands_are_runtime_free_shell_installers` |
| `thought_level` effort category + dynamic config-id writes (omp thinking vs claude `effort`) | `…/managed_agents/config_bridge/reader.rs`, `…/reader_tests.rs` | same suite — pin: `post_spawn_thought_level_config_option_surfaces_effort_with_native_write` |
| omp effort field model (deferred, configOption id `thinking`) | `desktop/src/features/agents/lib/agentConfigCore.ts`, `agentConfigCore.test.mjs`, rule 3 in `desktop/src/features/agents/AGENTS.md` | `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test 'src/features/agents/**/*.test.mjs'` and `pnpm typecheck` |
| `~/.bun/bin` on discovery search path and spawn/probe PATH (bun-installed runtimes work from the app) | `…/managed_agents/discovery.rs` (`common_binary_paths`), `…/managed_agents/runtime/path.rs` (`build_augmented_path`) | same Rust suite — discovery + `runtime::path` compose tests; live check: Doctor shows the runtime available with only a bun install present |
| Agent pool footprint cap (`DEFAULT_AGENT_PARALLELISM` = 1, matching buzz-acp's own default) | `…/managed_agents/types.rs` | same Rust suite; live check: spawn one agent, count its `omp`/harness children — exactly 1 per running agent (24× blew up a 16 GB machine with ~13 auto-started agents) |
| Orphan sweep knows omp/bun (`omp` in `KNOWN_AGENT_BINARIES`, `bun` in `KNOWN_SCRIPT_INTERPRETERS`) | `…/managed_agents/runtime.rs`, `…/runtime/tests.rs` | same Rust suite — pins: `name_matches_known_binary_accepts_omp`, `name_matches_interpreter_accepts_bun` |
| Local-deploy skill | `.agents/skills/buzz-local-deploy/SKILL.md` (default slug `rudu`) | follow it end-to-end; verify `Info.plist` identifier is `xyz.block.buzz.app.dev.<slug>` and sidecars exist in `Contents/MacOS` |
| Personal build identity (deploy branch only) | `desktop/src-tauri/tauri.conf.json` (`productName`, `identifier` with `.dev.` infix), `desktop/src-tauri/src/app_state_keyring.rs` (release keychain service `buzz-desktop-<slug>`) | present only on personal deploy branches; if a merge/rebase shows them on `main` or a PR branch, the branch is wrong — drop those hunks |
| Slim root agent guide + scoped guides | root `AGENTS.md` (intentionally slim, commit `e8ad3dcbe`), `desktop/AGENTS.md`, `desktop/src/features/agents/AGENTS.md`, `docs/agents/` | when upstream's former monolithic `AGENTS.md` conflicts, keep the slim Rudu guide; verify any genuinely-new upstream intent against code and rehome it in the matching scoped guide or `RELEASING.md` |

General gates for any fork change: `just ci` before a PR; `just test` for
relay/db/auth changes. Desktop Rust tests need the explicit manifest path
(above) — root `cargo test` does not include the desktop crate.

Live smoke for the omp harness (no app needed):

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{}}}' | omp acp
BUZZ_ACP_AGENT_COMMAND="$(command -v omp)" BUZZ_ACP_AGENT_ARGS=acp \
  ./target/release/buzz-acp auth-methods --json   # must print AND exit
```

## Conflict resolution playbook

The goal is always **preserve both** — upstream's change and the fork's.
Never resolve shared files with wholesale `ours`/`theirs`.

1. **Classify the file first.** Fork-owned files (this file, slim root
   `AGENTS.md`, scoped guides, `.agents/skills/`, `docs/agents/`,
   `.github/CODEOWNERS`) are kept in the Rudu shape; upstream edits there are
   mined for intent, not taken literally. Shared upstream files with fork
   additions take upstream's structure plus our hunks re-applied.
2. **Our shared-file changes are additive by design** — a new catalog entry,
   a new match arm, a new path-list entry, a new `else if` branch. Resolve by
   taking upstream's refactored surroundings and re-adding the fork's lines,
   not by reverting upstream to make our old hunk fit.
3. **Re-validate per the table** immediately after resolving, before
   committing the merge. Each validation entry is a test that fails loudly if
   our half was lost — that is the preservation guarantee.
4. **Personal-deploy hunks** (`tauri.conf.json` identity, keychain service)
   conflict on every rebase of a deploy branch; resolution is always "keep
   the personal value". They must never appear outside a deploy branch.
5. When upstream lands an equivalent of a fork change (e.g. their own omp
   runtime entry), drop ours and take theirs — shrinking the delta is a win.
   Note it in the sync PR.

## Releases

Rudu releases must use Rudu-owned application identifiers, updater endpoints
and keys, signing credentials, release tags, container images, and chart
repositories. Upstream publishing workflows (`just release-desktop`, canary,
tag-triggered `release.yml`) are not a Rudu release pipeline and must not be
run — they need block/buzz secrets and publish publicly. Personal/local
builds use `.agents/skills/buzz-local-deploy` (local `tauri build` only).

Keep the Apache 2.0 license and upstream attribution. Fork documentation
should identify Rudu changes without rewriting upstream history or authorship.

## Keep this file true

When you add a fork-owned change, add its row to the validation table in the
same PR. When a validation command changes, update it here. A row that no
longer matches reality is worse than no row.
