# Developing KnowYourLimits

[README](../README.md) · [User guide](usage.md) · [Publishing](publishing.md)

KnowYourLimits **0.2.6** uses Tauri 2 with a Rust backend and a TypeScript/Vite
interface. Run the native app to connect providers; browser preview has empty
quotas. Code, comments, tests, documentation, and release notes are written in
English. Product translations belong in locale resources.

## Prerequisites

| Target | Requirements |
| --- | --- |
| macOS | macOS 13+ on Apple Silicon; Xcode Command Line Tools, Rust 1.90+, Bun, and Git. |
| Windows | Windows 10/11 x64; Visual Studio Build Tools with **Desktop development with C++**, Rust 1.90+, Bun, Git, and WebView2 Runtime. |

For real readings, install and authorize the official Codex CLI or Claude Code
**2.1.296** on the same operating system as the app. Windows supports native
executables and official npm installations with Node.js. WSL authorization does
not authorize native Windows CLIs. Intel Macs and Windows ARM64 are not release
targets.

Signing credentials and Zamok are needed for signed macOS packaging, not for
ordinary development. See [packaging](#packaging).

## Run locally

From a checkout on the target platform:

```sh
git clone https://github.com/bshk-app/KnowYourLimits.git
cd KnowYourLimits
bun install --frozen-lockfile
bun run tauri dev
```

The Bun commands are the same in PowerShell. The native app starts with empty
quotas unless an existing app cache is restored. Use its connection controls to
opt into readings. Left-click an indicator to open/close the panel; right-click
for the native menu. Escape or loss of focus closes the panel.

For interface work without native provider connections:

```sh
bun run dev
```

Vite serves the panel at `http://127.0.0.1:1420` and the settings page at
`http://127.0.0.1:1420/settings.html`. Browser settings are kept in localStorage,
separately from native settings. `bun run build` type-checks and produces the
frontend in `dist/`; it does not produce an installable desktop package.

## Project map

| Path | Responsibility |
| --- | --- |
| [`src/main.ts`](../src/main.ts), [`src/settings.ts`](../src/settings.ts) | Panel and settings interface. |
| [`src/model.ts`](../src/model.ts), [`src/bridge.ts`](../src/bridge.ts) | UI state, quota presentation, and native IPC. |
| [`src/locales/`](../src/locales/) | Shared TypeScript/Rust translations for 25 languages. |
| [`src-tauri/src/live.rs`](../src-tauri/src/live.rs) | Provider discovery, reading coordination, timers, and cancellation. |
| [`src-tauri/src/claude.rs`](../src-tauri/src/claude.rs), [`codex.rs`](../src-tauri/src/codex.rs) | Official CLI readers and quota normalization. |
| [`src-tauri/src/storage.rs`](../src-tauri/src/storage.rs) | App-owned settings, quota cache, and retry persistence. |
| [`src-tauri/src/tray_render.rs`](../src-tauri/src/tray_render.rs), [`windows_tray.rs`](../src-tauri/src/windows_tray.rs) | macOS menu bar layouts and Windows tray rings. |
| [`scripts/`](../scripts/), [`packaging/`](../packaging/) | Platform packaging and verified release publication. |

The current [user guide](usage.md) describes freshness, reset, and privacy
semantics. Earlier specifications and verification reports in `docs/` are
historical; their demo modes, integration status, and test counts may no longer
describe the current app.

## CLI discovery

Discovery does not run shell profiles or depend on the launch directory. It does
not offer an arbitrary executable picker. On macOS it checks these locations
in order, preferring Homebrew installations over user-bin shadows:

```text
/opt/homebrew/bin
/usr/local/bin
~/.bun/bin
~/.local/bin
~/.npm-global/bin
~/.volta/bin
/usr/bin
```

On Windows it checks:

```text
%USERPROFILE%/.local/bin
%USERPROFILE%/.bun/bin
%USERPROFILE%/.cargo/bin
%APPDATA%/npm
%LOCALAPPDATA%/Microsoft/WinGet/Links
%ProgramFiles%/nodejs
absolute directories from PATH
```

Native `.exe` and official npm Claude/Codex installations with Node.js are
supported. For npm installations, the app launches the official JavaScript entry
point directly instead of running a `.cmd` wrapper through a shell. Windows
cancellation and exit terminate the CLI and its children through a Job Object.

macOS native notifications and the **One row**, **Two rows**, and **Rings** layouts
are platform-specific. Windows uses separate square ring indicators, with
percentages in tooltips. Avoid treating browser mocks as proof of platform parity.

## Local app data

| Platform | Directory |
| --- | --- |
| macOS | `~/Library/Application Support/dev.knowyourlimits.desktop/` |
| Windows | `%APPDATA%/dev.knowyourlimits.desktop/` |

`settings.json` holds preferences, `quota-cache.json` holds normalized snapshots
and connections, and `quota-retry.json` holds retry deadlines. Writes are atomic;
macOS files use `0600` permissions. Tokens, cookies, and raw CLI logs are not
serialized. See [storage and privacy](usage.md#storage-and-privacy) for the
distinction between app storage and CLI-owned activity. Do not commit local app
state, credentials, signing keys, dependencies, or generated build artifacts.

## Checks

Run checks from the repository root:

```sh
bun run test
bun run test:release
bun run check:release
bun run typecheck
bun run check
bun run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
bun run test:e2e
```

Install Playwright's Chromium browser with `bunx playwright install chromium`
if it is not already present. The Playwright configuration starts Vite as needed.

Default tests do not start real CLIs or sign-in flows. Unix synthetic subprocess
tests use `/usr/bin/python3`; the app itself does not require Python. Playwright
tests the interface with mocked IPC, and native Tauri ACL tests use the real
`RuntimeAuthority`. Translation tests check key completeness, interpolation,
settings migration, and panel/settings layout across supported languages.

These checks do not replace physical native QA for tray clicks, packaged WebView
IPC, focus behavior, multiple monitors, Windows taskbar themes, or macOS
notifications. Earlier [Codex verification](live-codex-checks.md) and its
[native checklist](live-codex-qa.md) record the scope of that earlier stage;
demo-specific steps are historical.

## Explicit live readings

> [!CAUTION]
> Live checks launch your installed official CLI and can cause CLI-owned authorization, network, and cache activity. Claude's HTTP 429 fallback can make a paid model request; see [cost details](usage.md#claude-rate-limits-and-cost).

On macOS, use the production coordinator and discovery policy explicitly:

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --example live_read -- --live
cargo run --locked --manifest-path src-tauri/Cargo.toml --example live_read -- --live claude
```

The first command reads Codex; the second reads Claude. The example does not sign
in and prints only normalized state. It does not save app state by default.
Append **`--persist` as the last argument** to load and save app-owned quota cache
and retry deadlines; **close the app first** to avoid competing writes. This
example is Unix-only, and its persistence path is the macOS app data directory;
verify Windows readings through the native Windows app.

Readings use the existing CLI authorization. Do not retain real raw RPC/debug
output, account identity, credentials, or quota fixtures in the repository.
Cancellation must finish before the reading owner exits. Force termination and
power loss do not provide graceful child-cleanup guarantees.

## Packaging

The release workflow is configured. As of October 12, 2026, no releases have been
published. Track [GitHub Releases](https://github.com/bshk-app/KnowYourLimits/releases).
The [publishing guide](publishing.md) covers tag/version validation, signing,
artifact verification, and draft publication. The first combined Windows/macOS
release needs a new version tag; the existing `v0.2.6` predates that workflow.

### Windows

Build on Windows with the prerequisites above:

```powershell
bun install --frozen-lockfile
bun run package:windows
```

For version 0.2.7, output in `artifacts/windows/0.2.7/` is:

- `KnowYourLimits-0.2.7-windows-x64-setup.exe` — installer for the current user;
  installs WebView2 if needed.
- `KnowYourLimits-0.2.7-windows-x64.exe` — portable executable; requires WebView2.
- `manifest.json` — version, source commit, sizes, SHA-256, and signing status.

Windows executables currently lack an Authenticode signature. GitHub Actions
retains platform packages as workflow artifacts; configured tag releases combine
them with the macOS package, version-bound updater signatures, and checksums
after verification. Promotion also publishes Homebrew and the Zamok stable feed.

### macOS

For local packaging on an Apple Silicon Mac, use Zamok with an existing Developer
ID identity and the **`notarize` Keychain profile** or an App Store Connect API key:

```sh
bun run package:macos
```

This checks notarization access, builds the Tauri `.app`, runs `zamokctl package`,
and verifies the signed, notarized, stapled app and DMG. Output appears in
`artifacts/macos/<version>/`. Public signing settings are in
[`packaging/macos.json`](../packaging/macos.json). Distribute the verified output;
a development bundle is not a notarized release.

The CI entry point is `bun run package:macos:ci`. It requires a clean checkout,
builds and signs the app, notarizes and staples it, creates a signed DMG, and
verifies Apple notarization and Gatekeeper acceptance. Required Apple credentials
already exist as organization secrets for this repository; no duplicate manual
secret setup is needed. See [publishing](publishing.md) for exact requirements and
local credential overrides.
