# Mobile Contributor Rules

Scope: all files under `mobile/`. Root [AGENTS.md](../AGENTS.md) rules also apply.

The mobile app uses Flutter, Riverpod, and `flutter_hooks`. Features live under
`lib/features/`; reusable code lives under `lib/shared/`.

## Architecture

- Use `HookConsumerWidget` or `ConsumerWidget`; do not add `StatefulWidget`.
- Feature modules may import from `shared/`, not from other feature modules.
- Keep Nostr kinds in `lib/shared/relay/nostr_models.dart` synchronized with
  `desktop/src/shared/constants/kinds.ts`.
- Use `context.colors` and `context.textTheme` instead of raw
  `Theme.of(context)` calls.
- Use `Grid` spacing tokens and `Radii` border-radius tokens.
- Use `debugPrint()` or structured logging, never `print()`.

## File structure

- Keep one public widget per file.
- Put private sub-widgets in sibling `part` files under the page directory.
- Files must remain below 1,000 lines. If
  `scripts/check-file-sizes.mjs` fails, split the file; do not raise or bypass
  the limit.

## Safe commands

Agents may run:

```bash
just mobile-fmt
just mobile-check
just mobile-test
```

Do not run `flutter run`, `flutter build`, `flutter clean`, `flutter upgrade`, or
`just mobile-dev` from an agent session.

## Testing

- Prefer widget tests for UI behavior.
- Use `ProviderScope(overrides: [...])` to inject fakes.
- Fake notifiers extend the real notifier and override `build()`.
- Use `WidgetHelpers.testable()` for simple tests; use an explicit
  `ProviderScope` and `MaterialApp` when additional overrides are required.
