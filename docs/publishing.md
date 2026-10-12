# Publishing KnowYourLimits

Repository: [bshk-app/KnowYourLimits](https://github.com/bshk-app/KnowYourLimits).
The `Release` workflow builds Windows 10/11 x64 executables and a signed,
notarized DMG for macOS 13+ on Apple Silicon, then publishes them together.

## Configure GitHub Actions

In the repository's **Settings → Secrets and variables → Actions**, add these
repository secrets before creating a release tag:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64 of a Developer ID Application `.p12` export, including its private key. |
| `APPLE_CERTIFICATE_PASSWORD` | Password protecting that `.p12` export. |
| `APPLE_API_PRIVATE_KEY` | Full contents of the App Store Connect `.p8` private key, with real newlines. |
| `APPLE_API_KEY` | App Store Connect key ID. |
| `APPLE_API_ISSUER` | App Store Connect issuer ID. |

Export the Developer ID Application certificate and private key using Keychain
Access on a Mac. The `.p12` must contain exactly one valid Developer ID
Application signing identity. Generate its base64 locally:

```sh
openssl base64 -A -in /path/to/developer-id.p12 -out /tmp/kyl-certificate-base64.txt
```

Paste the file contents into the `APPLE_CERTIFICATE` secret and remove the local
temporary file afterward. Add the App Store Connect key and its identifiers from
the Apple account authorized to notarize this application. Keep all private keys
outside the repository.

GitHub's built-in `GITHUB_TOKEN` handles release publication. The workflow grants
`contents: write` only to the final publication job. No personal access token,
Zamok installation, or permanent runner Keychain is required. The signing job
imports the certificate into a temporary Keychain, creates a private temporary
`.p8` file, and removes both in an `always()` cleanup step.

The validation job checks for all five secrets before starting platform checks.
Ordinary pushes and pull requests run `Check` without any Apple credentials.

## Create a release

Commit the workflow changes before releasing. The version must match in
`package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`; update the
package version in `src-tauri/Cargo.lock` too. Write English release notes in
`docs/releases/<version>.md`, starting with `# <version> `.

Use a new stable `x.y.z` version for changes after an existing version tag. The
existing `v0.2.6` tag predates the combined Windows/macOS workflow; create a new
version and tag for the first combined release.

```sh
bun run check:release
bun run test:release
git add .
git commit -m "Prepare Windows and macOS release"
git push origin main
kyl_release_version=0.2.7
git tag -a "v$kyl_release_version" -m "KnowYourLimits $kyl_release_version"
git push origin "v$kyl_release_version"
```

Set `kyl_release_version` to the version committed in the metadata. A push of a `v*`
tag starts `Release`. To retry, use **Actions → Release → Run workflow** and supply
the existing version tag. Branch names and tags that do not match the committed
version are rejected.

The workflow:

1. Validates the tag, release metadata, and required Apple secrets.
2. Reuses `Check`: scans Git history for secrets; runs TypeScript/Biome, frontend
   and release tests, Rust tests/fmt/Clippy on both platforms, and Playwright on
   macOS; builds the Windows NSIS installer and macOS `.app`.
3. Builds the macOS release on the `macos-15` ARM64 runner. Tauri signs the app with
   hardened runtime, notarizes it, and staples Apple's ticket. The packaging script
   creates a DMG with an Applications shortcut, signs and notarizes the DMG, staples
   it, and verifies both the app and DMG with Apple tools.
4. Downloads both platform artifacts from the same workflow run. Verifies their
   versions, architectures, source commits, expected filenames, sizes, and SHA-256.
5. Creates or resumes a draft release, uploads all assets, verifies the complete
   asset list, then publishes it. A failed build or upload cannot publish a partial
   release. An already public release is never overwritten.

The published release contains six files:

| File | Purpose |
| --- | --- |
| `KnowYourLimits-<version>-windows-x64-setup.exe` | NSIS installer for the current Windows user; installs WebView2 if needed. |
| `KnowYourLimits-<version>-windows-x64.exe` | Portable Windows executable. |
| `KnowYourLimits-<version>-windows-x64-manifest.json` | Windows version, source commit, sizes, hashes, and signing status. |
| `KnowYourLimits-<version>-macos-arm64.dmg` | Developer ID signed, notarized, and stapled macOS installer image. |
| `KnowYourLimits-<version>-macos-arm64-manifest.json` | macOS version, source commit, size, hash, and signing/notarization status. |
| `SHA256SUMS.txt` | SHA-256 for all three downloadable binaries. |

Windows executables currently do not have an Authenticode signature. macOS
publication requires successful Developer ID signing, notarization, and stapling.
Intel Macs and Windows ARM64 are not release targets.

## Local Windows packaging

```powershell
bun install --frozen-lockfile
bun run package:windows
```

Files appear in `artifacts/windows/<version>/` alongside `manifest.json`. Local
packaging allows uncommitted changes, but records `sourceDirty: true`; publication
rejects these manifests. Public releases use artifacts built from a clean tag in
GitHub Actions.

## Local macOS packaging

The existing `bun run package:macos` command uses Zamok, a Developer ID identity in
Keychain, and the `notarize` Keychain profile or an App Store Connect API key.
Public local signing settings live in `packaging/macos.json`. Override the
identity using `KYL_SIGNING_IDENTITY_SHA1` and the profile using
`KYL_NOTARY_PROFILE`; use `KYL_NOTARY_KEYCHAIN` for a separate Keychain.

```sh
xcrun notarytool store-credentials notarize
bun run package:macos
```

The command checks notarization access, builds the Tauri app, runs Zamok packaging,
and verifies the manifest, SHA-256, signature, stapling, and Gatekeeper assessment.
Output is in `artifacts/macos/<version>/`.

To run the same packaging path as GitHub Actions on an Apple Silicon Mac, import
the certificate and set `APPLE_SIGNING_IDENTITY` to its SHA1, then set
`APPLE_API_KEY_PATH`, `APPLE_API_KEY`, and `APPLE_API_ISSUER` together:

```sh
bun run package:macos:ci
```

This path uses Tauri and Apple tools and requires a clean Git checkout. Optional
`KYL_SIGNING_KEYCHAIN` selects the Keychain for DMG signing.

The older `bun run release:github` command is a separate local Zamok publication
path. It requires a selected `LICENSE`, clean Git, `KYL_GITHUB_REPOSITORY`, and
GitHub CLI authorization. Use the combined workflow for Windows/macOS releases.

## Source files and secrets

Commit source, tests, documentation, icons, configuration, and lockfiles. Keep
dependencies, generated artifacts, `.env` files, private signing keys, agent
settings, and application state out of Git; `.gitignore` covers these files.
Do not force-add them. Gitleaks scans Git history using a pinned binary and
verified SHA-256.

The installer does not contain CLI authorization, settings, or quota data from
the build machine. The application uses the official Claude Code and Codex CLIs
for authorization and never opens their credential or cookie files.

Developer ID fingerprints, Team IDs, and Keychain profile names are public
identifiers; they do not grant access to private keys or Apple accounts.

References: [Tauri GitHub pipeline](https://v2.tauri.app/distribute/pipelines/github/),
[Tauri macOS signing and notarization](https://v2.tauri.app/distribute/sign/macos/),
[GitHub certificate import](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications),
[GitHub CLI release creation](https://cli.github.com/manual/gh_release_create).
