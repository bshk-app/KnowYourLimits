# Publishing KnowYourLimits

Repository: [bshk-app/KnowYourLimits](https://github.com/bshk-app/KnowYourLimits).
The `Release` workflow builds Windows 10/11 x64 executables and a signed,
notarized DMG for macOS 13+ on Apple Silicon, then publishes them together.

## Configure GitHub Actions

The workflow uses the same Apple secret names as Murmator. As checked on
October 12, 2026, all five required secrets already exist in the `bshk-app`
organization with visibility `all`; KnowYourLimits is a public repository.
Their values were neither read nor copied during this setup.

| Secret | Value |
| --- | --- |
| `DEVELOPER_ID_P12` | Base64 of a Developer ID Application `.p12` export, including its private key. |
| `DEVELOPER_ID_P12_PASSWORD` | Password protecting that `.p12` export. |
| `NOTARY_KEY_P8` | Base64 of the App Store Connect `.p8` private key. |
| `NOTARY_KEY_ID` | App Store Connect key ID. |
| `NOTARY_ISSUER` | App Store Connect issuer ID. |
| `RELEASE_SIGNING_IDENTITY_SHA1` | Optional signing fingerprint override; defaults to `packaging/macos.json`. |

Export the Developer ID Application certificate and private key using Keychain
Access on a Mac. The `.p12` must contain exactly one valid Developer ID
Application signing identity. For a repository outside the organization, generate
its base64 locally and configure the five required Actions secrets:

```sh
openssl base64 -A -in /path/to/developer-id.p12 -out /tmp/kyl-certificate-base64.txt
```

Paste the file contents into the `DEVELOPER_ID_P12` secret and remove the local
temporary file afterward. Add the App Store Connect key and its identifiers from
the Apple account authorized to notarize this application. Keep all private keys
outside the repository.

GitHub's built-in `GITHUB_TOKEN` handles release publication. The workflow grants
`contents: write` only to the final publication job. No personal access token or
permanent runner Keychain is required. It installs Zamok 1.10.1 from a pinned,
SHA-256 verified archive. The signing job imports the certificate and Apple's
Developer ID intermediate CA into a temporary Keychain, creates a private
temporary `.p8` file, and removes credentials in an `always()` cleanup step.

The validation job checks for all five secrets before starting platform checks.
Ordinary pushes and pull requests run `Check` without any Apple credentials.

## Reference release processes

The setup was compared with Murmator's `scripts/release.sh` and
`.github/workflows/release.yml` at commit
`b46ca9f7f60de66c7ebadf92b42bb0145d26c83e` in `/Volumes/DATA/Murmur`, and
Tish's `.gitea/workflows/release-tish-win.yml` at commit
`f964619d820e1dda87921e2710d795b57dfef7f2` in `tishina/monorepo`
(local worktree `/Volumes/DATA/tishina-monorepo-worktrees/tish-nc-windows-release`).

Murmator supplies the separate Zamok signing/notarization step, temporary
Keychain, shared credential names, and draft publication sequence. Tish supplies
the tag/version checks, installable build artifacts, and verification after
upload. KnowYourLimits builds Windows natively in GitHub Actions; it does not
need Tish's models, sidecars, or cross-compilation container. GitHub supports the
v4 artifact actions used here. KnowYourLimits has no updater, so neither
Sparkle nor Tauri updater keys or a Zamok product registration are needed.

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
tag starts `Release` and publishes after verification. For a reviewable draft,
use **Actions → Release → Run workflow**, supply an existing version tag, and
leave `publish` off. Set `publish` on to publish after verification. The same
manual entry point can resume a failed draft. Branch names and tags that do not
match the committed version are rejected.

To publish an already verified draft without rebuilding its signed bytes:

```sh
gh release edit vX.Y.Z --repo bshk-app/KnowYourLimits --draft=false --latest
```

The workflow:

1. Validates the tag, release metadata, and required Apple secrets.
2. Reuses `Check`: scans Git history for secrets; runs TypeScript/Biome, frontend
   and release tests, Rust tests/fmt/Clippy on both platforms, and Playwright on
   macOS; builds the Windows NSIS installer and macOS `.app`.
3. Builds the macOS app on the `macos-15` ARM64 runner. Tauri creates the bundle;
   Zamok signs it with hardened runtime, notarizes it, staples Apple's ticket,
   and packages a signed, notarized, stapled DMG. The packaging script verifies
   both the app and DMG with `codesign`, `stapler`, and Gatekeeper. Apple variables
   are excluded from Tauri's build environment so notarization runs through Zamok.
4. Downloads both platform artifacts from the same workflow run. Verifies their
   versions, architectures, source commits, expected filenames, sizes, and SHA-256.
5. Creates or resumes a draft release, uploads all assets, verifies their names
   and sizes, downloads them again, and compares their SHA-256 before publication.
   A failed build, upload, or download verification cannot publish a partial
   release. An already public release is never overwritten.

The published release contains six files:

| File | Purpose |
| --- | --- |
| `KnowYourLimits-<version>-windows-x64-setup.exe` | NSIS installer for the current Windows user; installs WebView2 if needed. |
| `KnowYourLimits-<version>-windows-x64.exe` | Portable Windows executable. |
| `KnowYourLimits-<version>-windows-x64-manifest.json` | Windows version, source commit, sizes, hashes, and signing status. |
| `KnowYourLimits-<version>-macos-arm64.dmg` | Developer ID signed, notarized, and stapled macOS installer image. |
| `KnowYourLimits-<version>-macos-arm64-manifest.json` | macOS version, source commit, size, hash, and signing/notarization status. |
| `SHA256SUMS.txt` | SHA-256 for all three downloadable binaries and both manifests. |

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
GitHub Actions. Changes to the source or HEAD during packaging are also recorded
as dirty builds.

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

The same packaging path can use `APPLE_API_KEY_PATH`, `APPLE_API_KEY`, and
`APPLE_API_ISSUER` together instead of a Keychain profile. The CI entry point
additionally requires a clean checkout and verifies the resulting source commit:

```sh
bun run package:macos:ci
```

`package:macos --skip-build` is useful for inspecting an existing bundle, but its
manifest is marked dirty because the bundle's source commit cannot be established.

`bun run release:github` uses the same combined publisher as CI. It requires a
clean checkout of the exact version tag, GitHub CLI authorization, and Windows
artifacts built from that commit. Download them from the corresponding Actions run:

```sh
gh run download RUN_ID --repo bshk-app/KnowYourLimits \
  --name KnowYourLimits-windows-x64 --dir artifacts/windows
export KYL_GITHUB_REPOSITORY=bshk-app/KnowYourLimits
bun run release:github --dry-run
bun run release:github
# Publish immediately after verification:
bun run release:github --publish
```

The command always rebuilds macOS, verifies both platforms, and creates a verified
draft by default. It uses the active `gh auth login` session or `GH_TOKEN`.

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
