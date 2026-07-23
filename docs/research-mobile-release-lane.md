# Research: Mobile build/release lane without Buildkite

**Ticket:** wayfinder issue we-r-rudu/buzz#4 — "Mobile build/release lane without Buildkite"
**Date:** 2026-07-23 · **Branch:** `research/mobile-release-lane` (throwaway)

## Question

Upstream (block/buzz) ships the Flutter mobile app in `mobile/` via Block's private
Buildkite `buzz-releases` pipeline, fed manually by the `mobile-v*` tags that
`.github/workflows/auto-tag-on-release-pr-merge.yml` pushes when a `mobile-release/*` PR
merges (see `RELEASING.md`). That pipeline is not reusable. How should the Rudu fork
build and ship the app for iOS and Android?

## Starting conditions (verified)

- The app is a **plain Flutter (Dart) app** — `mobile/pubspec.yaml` (`name: buzz`,
  `version: 0.4.11+1`), `mobile/android/` (Gradle KTS), `mobile/ios/` (Xcode workspace).
  Not React Native, not Expo. No existing fastlane, Codemagic, or other mobile CD config
  in the repo.
- `we-r-rudu/buzz` is a **public** GitHub repository. This matters: standard
  GitHub-hosted runners (including macOS) are **free for public repositories**.
  [GitHub Actions billing docs](https://docs.github.com/en/billing/concepts/product-billing/github-actions)
  (accessed 2026-07-23).
- Rudu has **no Apple Developer Program membership** and **no Google Play Console
  account** yet, and no deadline.
- The lane must be executable by **one developer**, and it should reuse the existing
  release flow (`just release-mobile` → release PR → `mobile-v*` tag) where possible.

## Store accounts: the unavoidable gating costs

Every option below needs the same two store accounts; no CI tool removes these.

- **Apple Developer Program: $99/year.** Required for TestFlight and App Store
  distribution; also provides App Store Connect, which CI tools authenticate against
  via an API key. A free Apple account only covers sideloading to your own devices via
  Xcode. [developer.apple.com/programs](https://developer.apple.com/programs/)
  (accessed 2026-07-23).
- **Google Play Console: $25 one-time registration**, plus government-ID identity
  verification. [Play Console Help — Get started](https://support.google.com/googleplay/android-developer/answer/6112435)
  (accessed 2026-07-23).
- **Important Play gotcha for a fresh account:** *personal* developer accounts created
  after 2023-11-13 must run a **closed test with at least 12 opted-in testers for 14
  continuous days** and pass a questionnaire review (~7 days) before production access
  is granted. Internal testing works immediately; production does not. *Organization*
  accounts are exempt from the 12-tester rule but require D-U-N-S-style verification.
  [Play Console Help — App testing requirements for new personal developer accounts](https://support.google.com/googleplay/android-developer/answer/14151465)
  (accessed 2026-07-23). Given "no deadline," this gate is tolerable but should be
  started early — recruiting 12 testers is the long pole, not CI.

Signing assets each lane must manage:

- **iOS:** a distribution certificate + provisioning profile tied to the Apple Developer
  account, plus an App Store Connect API key for CI upload.
- **Android:** an upload keystore (kept by the developer; losing it before enrolling in
  Play App Signing is unrecoverable), and a Google Play Developer API service-account
  JSON key for CI upload.

## Option survey

### (a) GitHub Actions + fastlane

Flutter's own CD guide lists fastlane as the way to integrate with existing CI,
explicitly including GitHub Actions, and documents the full setup: `fastlane init` in
`mobile/android` and `mobile/ios`, `flutter build appbundle` / `flutter build ipa`,
`upload_to_play_store` (supply) for Play, `upload_to_testflight` (pilot) for iOS, and
**match** to sync iOS certificates/profiles across machines via an encrypted private
git repo. [docs.flutter.dev/deployment/cd](https://docs.flutter.dev/deployment/cd) ·
[docs.fastlane.tools](https://docs.fastlane.tools/) (accessed 2026-07-23).

- **Cost:** $0 on top of the store accounts. we-r-rudu/buzz is public, so macOS and
  Linux runners are free on standard GitHub-hosted runners. (If the repo ever goes
  private: macOS minutes bill at $0.062/min baseline, 2,000 free min/mo on GitHub
  Free — a ~30 min iOS lane would burn through that fast.)
  [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions).
- **Signing/accounts:** same store accounts as above. iOS certs managed via `match`
  (private certs repo + encryption passphrase in GitHub secrets); App Store Connect API
  key and Play service-account JSON stored as GitHub Actions secrets.
- **CI maintenance burden:** highest of the viable options, but modest and one-time-heavy:
  two `Fastfile`s, a private `match` repo, and one workflow triggered on `mobile-v*` tags
  (which the existing `auto-tag-on-release-pr-merge.yml` already emits — the workflow
  literally replaces the "human feeds tag to Buildkite" step in `RELEASING.md`). Ongoing:
  occasional fastlane/Xcode/Flutter version bumps.
- **Flutter fit:** first-class; this is the path Flutter's official docs walk through.
- **First build with no accounts yet:** nothing blocks authoring and locally testing the
  lanes today (`fastlane` runs fine locally; Flutter's guide recommends validating
  locally before moving to CI). Store uploads activate the day the accounts exist.

### (b) Codemagic

Flutter-focused hosted CI/CD, listed first in Flutter's "all-in-one options with
built-in Flutter functionality" (alongside Bitrise and Appcircle).
[docs.flutter.dev/deployment/cd](https://docs.flutter.dev/deployment/cd).

- **Cost:** single-user free plan with **500 free build minutes/month, refilled monthly,
  unlimited apps**; pay-as-you-go beyond that at **$0.095/min on macOS M2** (Linux
  cheaper). A release-only lane (a handful of ~20–30 min macOS builds/month) fits
  comfortably in the free tier. [codemagic.io/pricing](https://codemagic.io/pricing/)
  (accessed 2026-07-23).
- **Signing/accounts:** same store accounts. Codemagic's differentiator is **automatic
  iOS code signing**: it talks to the App Store Connect API and creates/renews
  certificates and profiles for you — no `match` repo to babysit. Android takes the
  upload keystore + Play service-account JSON.
- **CI maintenance burden:** lowest. Flutter SDK/Xcode versions are preinstalled and
  curated; configuration is one `codemagic.yaml` in the repo; builds can trigger on the
  same `mobile-v*` tag pattern. You trade in-repo workflow ownership for vendor
  dependency.
- **Flutter fit:** the best in the list — it is built for Flutter.
- **First build with no accounts yet:** can build unsigned/development artifacts
  immediately; store lanes wait on the accounts like everything else.

### (c) Expo EAS — not applicable to this app

EAS Build is Expo's hosted build service for **Expo and React Native** projects. The
adjacent new product, **Expo Launch** (beta, announced 2025-08-20), builds and submits
apps to the App Store from a GitHub repo with no config — but per the announcement it
"currently works with Expo, React Native, and websites," and the product page still
lists Play Store support as "Coming soon." Flutter is not a supported input on either
product as of this writing.
[expo.dev/blog/introducing-expo-launch](https://expo.dev/blog/introducing-expo-launch) ·
[launch.expo.dev](https://launch.expo.dev/) (accessed 2026-07-23).

Adopting it for buzz would mean migrating the app to Expo/React Native — out of scope
for a release-lane question. **Verdict: not applicable; do not plan around it.** Worth
re-checking in 6–12 months only if Expo adds Flutter inputs and Play support.

### (d) Fully manual local builds

`flutter build ipa` + Xcode Organizer / App Store Connect web upload for iOS;
`flutter build appbundle` + Play Console web upload for Android. The exact steps are the
"running deployment locally" half of Flutter's CD guide.
[docs.flutter.dev/deployment/cd](https://docs.flutter.dev/deployment/cd).

- **Cost:** $0 beyond store accounts and an existing Mac (iOS requires macOS + Xcode).
- **Signing/accounts:** same accounts; certificates/keystores live on the one
  developer's machine — no secrets management at all, which is genuinely simpler with
  exactly one committer.
- **Maintenance burden:** no CI to maintain, but every release is ~30–60 min of
  attended, error-prone clicking, and the "pipeline" is one laptop's disk. No bus
  factor, no audit trail, easy to ship from a dirty tree.
- **Flutter fit:** native — it is what the tooling assumes by default.
- **First build with no accounts yet:** this is the *only* lane that produces anything
  usable before the accounts exist: development-signed installs on the developer's own
  devices (free Apple account) and sideloaded APKs/AAB-internal-testing prep.

## Comparison

| | GH Actions + fastlane | Codemagic | Expo EAS | Manual local |
|---|---|---|---|---|
| Lane cost (this repo) | **$0** (public repo → free macOS/Linux runners) | $0 within 500 mac min/mo free tier; $0.095/mac-min after | n/a | $0 |
| Store accounts needed | Apple $99/yr + Play $25 (same for all) | same | n/a | same |
| iOS signing in CI | fastlane `match` (private certs repo) | automatic via App Store Connect API | n/a | none (local keychain) |
| CI maintenance | medium (Fastfiles + match repo + workflow) | low (one `codemagic.yaml`) | n/a | none, but per-release manual toil |
| Flutter fit | first-class (official docs path) | best (built for Flutter) | **none — RN/Expo only** | native |
| Reuses `mobile-v*` tag trigger | yes | yes | n/a | no |
| Works before accounts exist | lanes can be built/tested locally | unsigned/dev builds | n/a | dev-signed devices + sideload |

## Recommendation

**Do (d) now, then build (a) — with (b) as the documented escape hatch.**

Concretely, for the downstream "Mobile app migration" HITL ticket:

1. **Today (no accounts):** use fully manual local builds to validate that the fork's
   app builds and runs — `flutter build appbundle`, dev-signed iOS installs, sideloaded
   Android. In parallel, **open both store accounts now**: Apple Developer Program
   ($99/yr) and Play Console ($25). Opening the Play account early starts the clock on
   the new-personal-account gate (12 testers × 14 days before production access) — that,
   not CI, is the real time-to-ship bottleneck; consider an organization account to
   skip it if Rudu has/wants a legal entity.
2. **Once accounts exist:** wire **GitHub Actions + fastlane**, triggered on the
   `mobile-v*` tags the existing auto-tag workflow already pushes — this is a drop-in
   open-source replacement for the Buildkite handoff described in `RELEASING.md`.
   It costs $0 (the repo is public, so macOS runners are free), keeps all config,
   secrets, and release history in the one GitHub repo Rudu already operates, follows
   Flutter's officially documented path, and uses `match` + a private certs repo for
   iOS signing.
3. **If fastlane upkeep proves annoying** for one developer, switch the same tag
   trigger to **Codemagic** (500 free mac minutes/mo, automatic iOS code signing) —
   the store accounts, signing assets, and release cadence carry over unchanged.
4. **Ignore Expo EAS**: it does not build plain Flutter apps (Expo/React Native inputs
   only; Launch beta is iOS-only with Play "coming soon").

## Sources

- Flutter — Continuous delivery with Flutter: https://docs.flutter.dev/deployment/cd (accessed 2026-07-23)
- fastlane docs: https://docs.fastlane.tools/ (accessed 2026-07-23)
- GitHub — Actions billing (free for public repos; runner rates): https://docs.github.com/en/billing/concepts/product-billing/github-actions (accessed 2026-07-23)
- Codemagic — pricing (500 free min/mo single-user plan; $0.095/min macOS PAYG): https://codemagic.io/pricing/ (accessed 2026-07-23)
- Apple — Developer Program ($99/yr; TestFlight requires membership): https://developer.apple.com/programs/ (accessed 2026-07-23)
- Google Play Console Help — Get started ($25 one-time fee; identity verification): https://support.google.com/googleplay/android-developer/answer/6112435 (accessed 2026-07-23)
- Google Play Console Help — App testing requirements for new personal developer accounts (12 testers / 14 days closed test): https://support.google.com/googleplay/android-developer/answer/14151465 (accessed 2026-07-23)
- Expo — Introducing Expo Launch (beta scope: Expo, React Native, websites): https://expo.dev/blog/introducing-expo-launch (published 2025-08-20, accessed 2026-07-23)
- Expo Launch product page (Play Store "Coming soon"): https://launch.expo.dev/ (accessed 2026-07-23)
- Repo context: `RELEASING.md` (Buildkite `buzz-releases` handoff), `.github/workflows/auto-tag-on-release-pr-merge.yml` (`mobile-v*` tags), `mobile/pubspec.yaml` (plain Flutter app)
