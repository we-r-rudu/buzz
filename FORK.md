# Rudu fork of Buzz

This repository is the Rudu-maintained fork of
[`block/buzz`](https://github.com/block/buzz). It keeps Buzz's protocol and
technical naming unless a Rudu product requirement needs a deliberate change.

## Baseline

- Upstream: `https://github.com/block/buzz.git`
- Fork: `https://github.com/we-r-rudu/buzz.git`
- Initial fork base: `acfbb1bb6af54cb29cb152496ff43b8285dcb8cf`
- Baseline verified against upstream `main`: 2026-07-23

The commit graph is the source of truth for the current fork delta:

```bash
git fetch upstream --prune
git log --oneline upstream/main..main
git diff --stat upstream/main...main
```

## Branch policy

- `main` is the shippable Rudu fork.
- `upstream/main` is read-only and tracks `block/buzz`.
- Fork work uses short-lived topic branches.
- Upstream updates use `sync/upstream-YYYY-MM-DD` branches and pull requests.
- Feature pull requests may be squashed. Upstream sync pull requests must use a
  merge commit so Git retains upstream ancestry.
- Force pushes and rebases of `main` are not allowed.

## Upstream sync

Sync daily when upstream is active. Start from a clean worktree:

```bash
git fetch upstream --prune
git switch -c "sync/upstream-$(date +%Y-%m-%d)" origin/main
git merge --no-ff upstream/main
just ci
git push -u origin HEAD
```

Run `just test` as well when the update touches `buzz-relay`, `buzz-db`, or
`buzz-auth` and Postgres and Redis are available. Resolve conflicts narrowly;
do not take `ours` or `theirs` across whole upstream-owned files.

## Change placement

Use the least invasive extension point that works:

1. Existing workflows, configuration, Nostr events, `buzz-sdk`, or
   `buzz-ws-client`.
2. Fork-owned modules such as `crates/rudu-*`,
   `desktop/src/features/rudu-*`, or `mobile/lib/features/rudu_*`.
3. Small wiring changes in shared Buzz registries and entry points only when the
   feature cannot remain additive.

Do not add a speculative plugin framework or globally rename Buzz crates,
protocol identifiers, or `BUZZ_*` environment variables. Generic fixes should
be contributed upstream and removed from the fork delta after they return in an
upstream sync.

Current fork-owned changes include this file, the fork notices in `README.md`
and `AGENTS.md`, `.github/CODEOWNERS`, the OMP deployment guide, and removal of
legacy agent-client compatibility symlinks. Git history remains authoritative as
this list evolves.

## Releases and attribution

Rudu releases must use Rudu-owned application identifiers, updater endpoints and
keys, signing credentials, release tags, container images, and chart
repositories. Upstream publishing workflows are not a Rudu release pipeline.

Keep the Apache 2.0 license and upstream attribution. Fork documentation should
identify Rudu changes without rewriting upstream history or authorship.
