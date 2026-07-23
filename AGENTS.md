# AGENTS.md — AI Agent Contributor Guide

This file contains rules that apply to every change in this repository. More
specific instructions live in scoped guides linked below.

**Fork context:** this checkout is `we-r-rudu/buzz`, maintained separately from
upstream `block/buzz`. Follow [FORK.md](FORK.md) for fork-owned changes and
upstream sync rules. References to `block/buzz` in upstream documentation do not
define Rudu release or deployment behavior.

## Essential workflow

Activate the repository toolchain before running Git, hooks, or project commands:

```bash
. ./bin/activate-hermit
```

Use the repository recipes rather than reconstructing their commands:

```bash
just setup   # first-time dependencies and local infrastructure
just ci      # required before a PR
just test    # integration tests; required after relay, database, or auth changes
```

`just test` requires Postgres and Redis. Hooks may format and restage files, so
check `git status` again after a commit attempt. Run `just hooks` after environment
changes if hooks stop working.

Repository-wide constraints:

- Do not add `unsafe` Rust.
- Do not add `unwrap()` or `expect()` in production paths; propagate typed errors.
- Add doc comments to new public interfaces.
- Leave the working tree free of unrelated edits.

## Where changes go

| Change | Primary location |
|---|---|
| Event types, verification, filters, kind registry | `crates/buzz-core` |
| WebSocket/HTTP ingest, subscriptions, side effects | `crates/buzz-relay` |
| Postgres access and migrations | `crates/buzz-db`, `migrations/` |
| Authentication and authorization | `crates/buzz-auth` |
| Search, pub/sub, audit, media | Matching `crates/buzz-*` module |
| Agent-facing commands | `crates/buzz-cli` |
| Typed event builders and clients | `crates/buzz-sdk`, `crates/buzz-ws-client` |
| Desktop UI or native shell | `desktop/` |
| Browser client | `web/` |
| Mobile app | `mobile/` |

For Rudu-only work, use the least invasive seam that works: existing protocol or
configuration first, then fork-owned modules such as `crates/rudu-*`, and only
then small shared wiring changes. Send generic fixes upstream when practical.
Do not add a speculative plugin framework or globally rename Buzz protocol,
crate, or `BUZZ_*` environment identifiers.

## Architecture contracts

- **Prefer Nostr events over new endpoint-specific HTTP APIs.** HTTP is reserved
  for NIP-11/NIP-05 metadata, generic `/events`, `/query`, and `/count` bridges,
  workflow webhooks, Blossom media, git smart HTTP and policy hooks, and health
  probes.
- **Register every event kind in `crates/buzz-core/src/kind.rs`.** Add relay
  handling only after the kind is registered; `ALL_KINDS` checks collisions.
- **Scope channels with `h` tags, not `e` tags.** Filters and queries operating
  inside a channel must include the channel's `h` tag.
- **Preserve host-derived community isolation.** Relay, HTTP, media, git, search,
  workflow, and pub/sub paths must use the community resolved from the request
  host, never a client-controlled tag as the tenant authority.
- **Put agent-facing operations in `buzz-cli`.** Add the subcommand first, then
  wire transport behavior in its client module. `buzz-dev-mcp` is the separate
  shell and file-tool surface used by `buzz-agent`.
- **Maintain thread counters.** Any reply insertion path must update the
  materialized `reply_count` and `descendant_count` on the thread root.
- **Keep workflow conditions compatible with `evalexpr`.** Prefer existing
  condition and action shapes over a parallel evaluator.

Reference the Nostr specifications at <https://github.com/nostr-protocol/nips>.

## Agent CLI

Managed agent subprocesses receive `BUZZ_RELAY_URL`, `BUZZ_PRIVATE_KEY`, and
`BUZZ_AUTH_TAG`. Local development must provide the required relay URL and key.

Read a `buzz://message` deep link with:

```bash
buzz messages thread --channel <uuid> --event <hex> --format compact
```

The optional deep-link `thread` parameter is not required. `--format compact` is
a global flag and therefore precedes the subcommand. See
[`crates/buzz-cli/TESTING.md`](crates/buzz-cli/TESTING.md) for the complete CLI
contract and live-testing runbook.

## Common gotchas

1. Channel metadata is kind `39000`; NIP-01 kind `41` is unused by Buzz.
2. Relay queries must specify `kinds`; kindless queries are rejected by the
   p-gate. `messages search` should normally use `--kinds 9,45001,45003`.
3. The desktop Tauri crate is excluded from the root Cargo workspace. Run its
   checks through `just desktop-tauri-*` or its own manifest.
4. Desktop Tauri formatting can fail from a Git worktree. Run
   `just desktop-tauri-fmt` from the main checkout, then restage the result.
5. Do not depend on complete React Query result objects in memoized desktop
   paths. Depend on stable methods such as `mutateAsync`; use
   `desktop/src/shared/hooks/useStableReference.ts` for derived collections.

## Agent skills

### Issue tracker

Issues and PRDs are tracked in GitHub Issues for `we-r-rudu/buzz`; external pull
requests are not a triage surface. See `docs/agents/issue-tracker.md`.

### Triage labels

Triage uses the canonical `needs-triage`, `needs-info`, `ready-for-agent`,
`ready-for-human`, and `wontfix` labels. See `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repository; read root `CONTEXT.md` and `docs/adr/` when
present. See `docs/agents/domain.md`.

## Scoped guides

Read the guide covering the area you change:

- [Desktop rules](desktop/AGENTS.md)
- [Mobile rules](mobile/AGENTS.md)
- [Agent configuration rules](desktop/src/features/agents/AGENTS.md)
- [Desktop screenshot skill](desktop/src-tauri/src/managed_agents/screenshot_skill.md)

For PR screenshots, never use `buzz upload`, relay media URLs, or third-party
image hosts. The current posting script still targets `block/buzz`; do not run it
for a Rudu PR until it is made fork-aware.

## Detailed documentation

- [CONTRIBUTING.md](CONTRIBUTING.md) — setup, code style, and contribution flow
- [TESTING.md](TESTING.md) — unit, integration, and live relay testing
- [ARCHITECTURE.md](ARCHITECTURE.md) — protocol and module architecture
- [RELEASING.md](RELEASING.md) — upstream release implementation
- [FORK.md](FORK.md) — Rudu ownership and upstream synchronization
