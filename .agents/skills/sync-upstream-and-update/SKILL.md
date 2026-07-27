---
name: sync-upstream-and-update
description: Sync we-r-rudu/buzz branches with block/buzz upstream without losing fork changes, then update what runs the code — the local Ruduzz app and the VPS agent fleet. Covers upstream syncs, merging/rebasing personal branches, conflict resolution, and checking/redeploying VPS-hosted buzz-acp agents.
disable-model-invocation: true
---

# Sync Upstream and Update

Sync this fork with `block/buzz` upstream, then bring every runtime that
executes the code up to date. The policy and conflict playbook live in
`FORK.md` — this skill owns the runnable gates; scripts do every check that
can be deterministic, judgment enters only at conflict resolution.

Scripts live in `scripts/` beside this file. All run from the repo root and
are safe to re-run (idempotent or read-only as noted).

## Step 1 — Preflight (read-only)

```bash
bash .agents/skills/sync-upstream-and-update/scripts/sync-preflight.sh [branch]
```

Fetches upstream, reports drift, and predicts conflict zones by intersecting
the upstream diff with the branch's own fork-side diff (derived from git —
no manifest to maintain). Completion: output is either `nothing to sync`
(skip to step 6 to verify the VPS anyway) or a forecast listing each
fork-touched file upstream also changed. The forecast decides how careful
step 2 must be.

## Step 2 — Merge and resolve

Merge `upstream/main` into the working branch (or rebase, for personal
branches — force-push becomes necessary). Resolve per the **conflict
resolution playbook in `FORK.md`**: classify fork-owned vs shared first,
preserve both sides, deploy-only hunks (`tauri.conf.json` identity,
`app_state_keyring.rs` service) always keep the personal value.

Completion: `git status` shows no unmerged paths.

## Step 3 — Validate before committing the merge

```bash
bash .agents/skills/sync-upstream-and-update/scripts/validate-fork.sh
```

Runs the FORK.md validation table as one gate. This is the step that catches
what textual merges cannot: renamed/changed struct fields (upstream
`install_instructions_url` → `cli_/adapter_` split broke the omp entry while
showing zero conflicts). **Never commit a sync merge on a red gate.**
Completion: exit 0.

## Step 4 — Commit and push

Commit the merge. Push notes: `gh` commands need `--repo we-r-rudu/buzz`
(checkout resolves upstream by default); after a rebase, push needs
`--force-with-lease` — confirm the old branch tip is what you expect before
forcing.

## Step 5 — Update the local app

Skip when the sync touched nothing under `desktop/` (preflight forecast says
so). Otherwise follow `skill://buzz-local-deploy` — rebuild + reinstall.
Completion: installed bundle id and version match expectations.

## Step 6 — Update the VPS agent fleet

The VPS (`ssh dn`) runs exactly one binary, `buzz-acp`, so it goes stale only
when its dependency closure changes. The closure is computed dynamically —
never assume.

```bash
bash .agents/skills/sync-upstream-and-update/scripts/check-vps-update.sh
```

- Exit 0 → up to date, done.
- Exit 1 → stale (or history rewritten since the build): run
  `bash .agents/skills/sync-upstream-and-update/scripts/redeploy-vps.sh`
  (~3 min; archives the branch, rebuilds on the VPS, waits for the fleet to
  go idle — max 2 min — before restarting all units, restamps).

Completion (both required):

```bash
bash .agents/skills/sync-upstream-and-update/scripts/check-vps-update.sh   # exit 0
ssh dn 'journalctl -u "buzz-agent@*" --since "-3min" --no-pager | grep -c "presence set to online"'   # == fleet size (10)
```

## Scripts reference

| Script | Guarantee | Mutates? |
|---|---|---|
| `sync-preflight.sh` | drift count, upstream commit list, conflict forecast from three-dot diffs | no (fetch only) |
| `validate-fork.sh` | full FORK.md validation table green (Rust suite, frontend tests, typecheck) | no |
| `check-vps-update.sh` | VPS build stamp vs branch, filtered to the buzz-acp closure + `Cargo.lock` + `rust-toolchain.toml`; detects rebased-away stamps | no |
| `redeploy-vps.sh` | rebuilds `buzz-acp` from the branch tip on the VPS, waits for fleet idle (≤2 min) before restarting `buzz-agent@*`, restamps | VPS only |

## Maintenance

- VPS topology changes (host alias, unit names, fleet size) → update the
  two VPS scripts + step 6's online count.
- FORK.md policy/playbook changes → this skill points at them; nothing to
  duplicate here.
