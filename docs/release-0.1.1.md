# KnowYourLimits 0.1.1 — native fixes and Zamok signing

10 October 2026. Replaces the locally signed 0.1.0 prototype.

## Native behavior

- Native startup is now `ready`, with empty snapshots and dash counters. No demo
  samples, CLI discovery or execution on startup. Settings continue to persist.
- Connect Codex is at the top of the panel. It starts the same single-flight,
  managed production reader used by tray refresh. Live mode remains session-only.
- Demo is explicit: Show demo / Return to demo. Browser preview remains demo-only.
- Claude remains disconnected until its reliable quota source is qualified.
  No polling, notifications or autostart have been added by this patch.

## Fixed native event permissions

The shipped capability had no permissions. `@tauri-apps/api/event.listen` invokes
`plugin:event|listen`, so the real Tauri authority denied the command and the UI
reported: «Обновления панели недоступны. Состояние можно прочитать при открытии панели».
The browser native mocks accepted this command and did not exercise that authority.

Added only `core:event:allow-listen` and `core:event:allow-unlisten` for main.
Regression uses the application's generated context and Tauri's actual
`RuntimeAuthority::resolve_access`, not a stub of the permission decision.
It failed with the shipped capability, then passed. Shell execution remains denied.
Native startup regression likewise failed on fabricated quotas before the fix.

Browser regression verifies ready/empty state, connection above the quota cards,
live transition and event-based completion. It first failed because the connection
button was at y=449; it passes after moving that section above the cards.

## Same signing workflow as Murmur and Tish

Reviewed Murmur's `scripts/release.sh` and Tish's `apps/tish-mac/Taskfile.yml`.
Both use Developer ID + Apple notarization + stapling through zamokctl. Murmur
hands an unsigned app to `zamokctl package`; Tish's existing archive is already
Developer ID-signed and passes `--no-codesign` to the release pipeline.

This Tauri product follows the unsigned-app path:

```sh
bun run package:macos
```

`scripts/package-macos.mjs` builds the release, then invokes installed
`zamokctl 1.10.1 package` with the local Developer ID certificate and the existing
`notarize` Keychain profile. It signs with hardened runtime + timestamp, notarizes,
staples and produces DMG + manifest. `packaging/macos.json` contains public certificate
identity and profile name only. No private key/password/token is copied into the repo.
CLI overrides: `KYL_SIGNING_IDENTITY_SHA1`, `KYL_NOTARY_PROFILE`.

No Swift licensing SDK, restricted entitlements or provisioning profile are used by
KnowYourLimits, so those Tish-specific steps do not apply. No Zamok/GitHub product
publication or appcast is needed for this signed installer.

## Verification and artifact

- 80 Rust library + 2 example + 42 TS + 14 browser = **138 passing test cases**.
  Locked Rust suite, rustfmt/Clippy, strict TypeScript/Biome and release build pass.
- Explicit live smoke through production coordinator returned current real weekly
  Codex, no notice, no refresh pending and empty/unavailable Claude. It started from
  the same ready state as the app. No raw account/credential fixture was saved.
- Developer ID Application: Aleksandr Beshkenadze, team `Q8H6GWJ658`.
- Both staged app and consumer DMG pass `codesign`, Gatekeeper and stapler validation.
  Gatekeeper reports **Notarized Developer ID**. The DMG was mounted read-only;
  its embedded app also passed signature, Gatekeeper and ticket checks, then unmounted.
- Physical tray/panel clicks are not claimed as automated QA: native CUA app capture
  timed out. The permission regression exercises the actual Tauri authority; browser
  tests exercise DOM with mock transport. Those checks have distinct boundaries.

Artifact: `artifacts/macos/0.1.1/KnowYourLimits-0.1.1.dmg`, Apple Silicon, macOS 13+.
SHA-256: `525e0a9d89aa2a7604f3811bcf46d520d4fbb740ef67d249019b139768a7ffab`.
`manifest.json` records `notarized=true`, `stapled=true`, matching bundle/version/hash.

The signed app is under `artifacts/macos/0.1.1/staging/KnowYourLimits.app`.
The original Tauri build output remains unsigned/ad hoc; distribute the Zamok artifact.

## References

- [Murmur pipeline](/Volumes/DATA/Murmur/scripts/release.sh)
- [Tish pipeline](/Users/akira/Projects/My/TishinaProject/monorepo/apps/tish-mac/Taskfile.yml:1196)
- [Zamok release skill](/Users/akira/Projects/My/ZamokProject/skills/zamok-release/SKILL.md)
- [Tauri event permissions](https://v2.tauri.app/reference/javascript/api/namespaceevent/#remarks)
