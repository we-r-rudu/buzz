# Desktop Contributor Rules

Scope: all files under `desktop/`. Root [AGENTS.md](../AGENTS.md) rules also
apply. More specific guides override this file inside their directories.

The desktop app is Tauri 2, React 19, Vite, Tailwind CSS, and Biome. UI features
belong under `src/features/`; native commands and process integration belong
under `src-tauri/`.

## Checks

```bash
just desktop-check
just desktop-test
just desktop-build
just desktop-tauri-check
just desktop-tauri-test
```

The Tauri crate is excluded from the root Cargo workspace. Use the `just
desktop-tauri-*` recipes or `desktop/src-tauri/Cargo.toml` explicitly.

## Text sizing and zoom

Desktop zoom changes the root HTML font size, so readable text must use named,
rem-based Tailwind tokens.

- Chat body and author text use `text-base`.
- Metadata uses `text-2xs` or `text-3xs`.
- Do not add arbitrary px, rem, or em text-size literals.
- If the existing scale cannot express a real design requirement, add a named
  rem-based token in `tailwind.config.js`.
- `scripts/check-px-text.mjs` enforces the rule. Decorative glyph exceptions
  must be explicitly allowlisted there.

## Community switching

`<AppReady key={communityKey}>` remounts React state when communities change,
but module-level caches, Maps, class instances, and promises survive.

Every module-level singleton containing community-scoped data must expose a
reset function and register it in `src/features/communities/useCommunityInit.ts`
inside `resetCommunityState()`. Inspect that function for the authoritative list;
do not duplicate the list in documentation.

Key files:

- `src/app/App.tsx` — community remount boundary
- `src/features/communities/useCommunityInit.ts` — reset and backend config
- `src/main.tsx` — provider hierarchy

## React reference stability

Do not depend on complete React Query result objects in memoized paths. Depend on
stable methods such as `mutateAsync`. Use
`src/shared/hooks/useStableReference.ts` when a derived Map or array needs a
content-stable reference.

Measure interaction performance with DevTools closed and without per-interaction
logging.

## E2E and screenshots

- Desktop rendering requires the mock Tauri bridge; a plain browser is not a
  valid app environment.
- Every E2E spec calls `installMockBridge(page)`.
- Register `page.addInitScript` state before installing the bridge.
- Wait for `waitForMockLiveSubscription` before emitting mock live messages.
- Call the shared `waitForAnimations(page)` helper before every screenshot.
- Scope screenshots to their subject. If capturing multiple states, verify their
  hashes differ before posting.
- `general` has pre-seeded unread messages; use `engineering` for a no-unread
  state.
- `reuseExistingServer: true` can serve a stale build; rebuild before rerunning a
  changed screenshot spec.

Read
[`src-tauri/src/managed_agents/screenshot_skill.md`](src-tauri/src/managed_agents/screenshot_skill.md)
before capturing or posting PR screenshots.
