---
name: desktop-screenshot
description: >
  Capture desktop app screenshots and post them to GitHub PRs with immutable URLs.
version: 1
---

# Desktop Screenshot Skill

## CRITICAL: How to Host Screenshots for PRs

**NEVER use `buzz upload`, the relay media endpoint, or any third-party image
host (imgur, imgbb, etc.) for PR screenshots.** Relay media URLs fail through
GitHub's camo proxy (`Non-Image content-type returned`). External hosts are
unreliable and may expose content.

**ALWAYS use `scripts/post-screenshots.sh`** — it hosts PNGs on a per-developer
git branch with immutable commit-SHA URLs that render correctly on GitHub.
If you manually compose or edit PR markdown, run
`scripts/check-pr-image-urls.sh <markdown-file>` before posting. The checker
fails on Buzz/relay media URLs so broken images are caught locally.

This hosting rule applies to any PNG you want in a PR, including mobile
simulator screenshots captured outside the desktop Playwright helper.

## Fork Status

`scripts/post-screenshots.sh` currently sets `REPO=block/buzz`. Do not run it
for a `we-r-rudu/buzz` PR until the script is made fork-aware. Screenshot
capture remains safe; only the posting step is blocked for Rudu PRs.

## Step 1 — Capture Screenshots

`just desktop-screenshot` builds the frontend, starts a preview server, and
runs Playwright with the mock bridge (no relay needed).

```bash
just desktop-screenshot --name home
just desktop-screenshot --name channel --route /channels/general
just desktop-screenshot --name ctx-menu --right-click channel-random --clip 0,200,320,300
just desktop-screenshot --name sidebar --active-channel general --messages /tmp/msgs.json --clip 0,0,256,720
```

**Flags:** `--name` (filename, required), `--route` (client route), `--active-channel`
(channel to view), `--click`/`--right-click`/`--hover` (interact before capture),
`--clip` (crop as `x,y,w,h`), `--messages` (JSON file), `--wait` (ms, default 2000),
`--viewport` (WxH, default 1280x720), `--outdir` (default `test-results/screenshots`).

Output: PNG path on stdout.

### Injecting Messages

Write a JSON array to a temp file. `channelName` and `content` are required:

```json
[
  {"channelName": "random", "content": "Hey check this out", "kind": 40002},
  {"channelName": "random", "content": "Another message"}
]
```

Without `--active-channel`, the helper navigates to the message channel (for
showing content). With `--active-channel`, messages can target other channels
while the camera stays put (for unread indicators, badges).

### Available Mock Channels

`general`, `random`, `design`, `sales`, `engineering`, `agents`, `watercooler`,
`announcements`, `alice-tyler`, `bob-tyler`.

`general` has pre-seeded messages (always shows `hasUnread`). Use `engineering`
for "no unread" visual states.

## Step 2 — Post to a PR

```bash
./scripts/post-screenshots.sh <PR-number> test-results/screenshots
./scripts/post-screenshots.sh <PR-number> test-results/screenshots body.md
```

The script pushes images to `agent-screenshots/<github-username>` and posts a
new PR comment. Re-runs overwrite the image blobs but append another comment;
delete the superseded comment so reviewers do not see stale screenshots.

### Body Templates

The optional third argument is a markdown file with `{{filename}}` placeholders
(without `.png`). Images not referenced by a placeholder are appended at the end.

```markdown
### Context menu
Right-click shows "Star channel".

{{01-ctx-menu}}

### Starred section
`engineering` appears under Starred.

{{02-starred}}
```

## E2E Screenshot Specs

- Call `installMockBridge(page)` in every desktop E2E spec.
- Register `page.addInitScript` state before installing the bridge.
- Wait for `waitForMockLiveSubscription` before emitting mock live messages.
- Call the shared `waitForAnimations(page)` helper before every
  `page.screenshot()` or `locator.screenshot()`.
- Scope screenshots to the state being demonstrated. When capturing multiple
  states, verify their file hashes differ before posting.

## Gotchas

1. **Stale server** — `reuseExistingServer: true` means a prior build serves old
   code. Kill port 4173 and rebuild (`cd desktop && pnpm run build`) after code changes.
2. **Clip for readability** — full 1280x720 screenshots are hard to read for sidebar
   features. Sidebar = 256px wide; context menus ~450px.
3. **`post-screenshots.sh` requires `gh` auth** — the script uses `gh api` and
   `gh pr comment`. Ensure `gh auth status` succeeds.
