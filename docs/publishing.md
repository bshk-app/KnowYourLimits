# Publishing KnowYourLimits

One release distributes macOS 13+ on Apple Silicon and Windows 10/11 x64 through
[GitHub Releases](https://github.com/bshk-app/KnowYourLimits/releases), Homebrew
(`bshk-app/tap/knowyourlimits`), and Zamok's public stable updater.

## Release commands

Release Please maintains a version/changelog PR from conventional commits
(`feat:`, `fix:`). It updates package.json, Tauri config, Cargo.toml, and only
the KnowYourLimits package in Cargo.lock. Review and merge that PR to create a
version tag and GitHub draft; the same workflow then builds, verifies, and
publishes GitHub, Homebrew, and the stable updater. It calls the reusable release
workflow and PR checks directly because GITHUB_TOKEN events do not trigger a
second workflow. Future release notes use the matching CHANGELOG.md section; a curated
`docs/releases/<version>.md` takes precedence.

Commit matching versions in `package.json`, `src-tauri/tauri.conf.json`,
`src-tauri/Cargo.toml`, and the application entry in `src-tauri/Cargo.lock`.
Add English notes in `docs/releases/<version>.md`, starting with `# <version> `.
Push the source and a new annotated `v<version>` tag. Never move an existing tag.

```sh
bun run check:release
bun run test:release
git tag -a v0.2.7 -m "KnowYourLimits 0.2.7"
git push origin v0.2.7
```

The tag starts `Release` and creates verified **drafts** in GitHub and Zamok.
It does not offer an update or publish a cask. `bun run release --draft` requests
this workflow for an existing tag. The first live run of a new distribution path
must remain a draft until its signed files are checked.

From a checkout of that version tag:

```sh
bun run release --promote
```

This dispatches the workflow with `promote=true`. It downloads and verifies the
original GitHub files, checks Zamok's hosted updater bytes, publishes GitHub,
publishes and audits Homebrew through `zamokctl cask`, then publishes the stable
updater. No rebuilding or re-signing occurs. An older version cannot replace a
newer stable release. Release runs are serialized. For subsequent releases,
`bun run release --publish` (`publish=true`) builds, verifies drafts, and promotes
in one workflow invocation.

## Credentials and configuration

Existing organization secrets are reused:

| Secret | Purpose |
| --- | --- |
| `DEVELOPER_ID_P12`, `DEVELOPER_ID_P12_PASSWORD` | Developer ID certificate and private key. |
| `NOTARY_KEY_P8`, `NOTARY_KEY_ID`, `NOTARY_ISSUER` | App Store Connect notarization. |
| `TAP_GITHUB_TOKEN` | Write access to `bshk-app/homebrew-tap`. |
| `RELEASE_SIGNING_IDENTITY_SHA1` | Optional override for the public identity in `packaging/macos.json`. |

Repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `ZAMOK_API_KEY` support the
updater. The first is a dedicated KnowYourLimits Minisign key; the second reuses
the existing Zamok automation credential. The public key is pinned in
`tauri.conf.json`. Ordinary branch and pull-request checks need no release secrets.
Keep the private updater key backed up: replacing the public key without a
transition signed by the old key prevents existing installations from updating.
The current key has no password; its file and Actions secret are access-controlled.

[`packaging/zamok.json`](../packaging/zamok.json) names product `knowyourlimits`,
channel `STABLE`, endpoint `https://zamok01.bshk.app`, the repository, and tap.
[`packaging/cask.json`](../packaging/cask.json) holds Homebrew metadata. Updates
require no license or account. The cask uses the Zamok CLI; this Tauri-only
product does not use the server's Sparkle cask generator.

## Build and verification

The workflow validates tag/version consistency and credentials before native
checks. `Check` scans Git history, runs frontend/release/Rust tests, formatting,
Clippy on both platforms, macOS Playwright, and builds Windows NSIS.

Tauri builds an unsigned macOS app. Checksum-pinned Zamok 1.10.1 signs,
notarizes, staples, and packages the final app and DMG. The script verifies
`codesign`, `stapler`, and Gatekeeper. Apple credentials use a temporary Keychain
and files removed after the signing job.

`sign-updates.mjs` archives the **final signed and stapled app**, then signs that
archive and the Windows installer with `tauri signer sign --app-version`.
Minisign verifies their signatures and authenticated version comments against the
pinned public key. The app requires version-bound signatures.

The publisher verifies platforms, versions, source commits, clean-build state,
filenames, sizes, and SHA-256. GitHub draft files are downloaded again and hashed.
Zamok uses presigned uploads and Tauri finalize with `publish=false`; hosted files
are downloaded and compared before promotion. Public updater endpoints must
return the expected version, signature and URL for an older client, and HTTP 204
for the current version.

## Distribution files

Nine files are published:

| Suffix after `KnowYourLimits-<version>` | Purpose |
| --- | --- |
| `-windows-x64-setup.exe` | Per-user NSIS installer and Windows updater. |
| `-windows-x64-setup.exe.sig` | Version-bound updater signature. |
| `-windows-x64.exe` | Portable executable for manual replacement. |
| `-windows-x64-manifest.json` | Windows provenance, hashes, and updater metadata. |
| `-macos-arm64.dmg` | Signed, notarized, stapled installer and Homebrew download. |
| `-macos-arm64.app.tar.gz` | Final signed, stapled app for the updater. |
| `-macos-arm64.app.tar.gz.sig` | Version-bound updater signature. |
| `-macos-arm64-manifest.json` | macOS provenance, hashes, and signing metadata. |
| `SHA256SUMS.txt` (without prefix) | Hashes of the eight other files. |

Windows has updater signatures; its executables lack an Authenticode publisher
signature. macOS has Apple signing/notarization and updater signatures. Intel
Macs and Windows ARM64 are not distribution targets.

## Recovery

Failed builds/uploads leave drafts. A draft consumes its version/channel;
staging can reuse it only if hosted bytes match exactly. Do not rebuild a verified
draft: Developer ID timestamps change signed bytes. If promotion fails after
GitHub becomes public, run `release --promote` again. It verifies immutable files,
retries Homebrew, and offers the update only after Homebrew succeeds. It can also
recheck already-published updater endpoints. Use a new version for changed files.
`gh release edit` alone skips Homebrew and updater publication.

## Local packaging

On Windows, `bun run package:windows` writes installer, executable, and manifest
to `artifacts/windows/<version>/`. On an ARM64 Mac, `bun run package:macos` writes
a verified DMG and final app staging directory to `artifacts/macos/<version>/`.
Developer ID and the `notarize` Keychain profile are required. Overrides:
`KYL_SIGNING_IDENTITY_SHA1`, `KYL_NOTARY_PROFILE`, `KYL_NOTARY_KEYCHAIN`, or all of
`APPLE_API_KEY_PATH`, `APPLE_API_KEY`, and `APPLE_API_ISSUER`. Dirty builds and
`package:macos --skip-build` are local-only. `package:macos:ci` requires a clean
source commit. Keep private keys, settings and CLI authorization out of Git.

References: [Tauri updater](https://v2.tauri.app/plugin/updater/),
[Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/), and
[GitHub release CLI](https://cli.github.com/manual/gh_release_create).
Version automation follows [Release Please](https://github.com/googleapis/release-please-action).
