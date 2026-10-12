# KnowYourLimits

A Claude and Codex quota indicator for the macOS menu bar and Windows system tray.
Current version: **0.2.6**.

[Downloads](https://github.com/bshk-app/KnowYourLimits/releases/latest) are published
by GitHub Actions: a Windows installer and portable executable, and a Developer ID
signed, notarized DMG for Apple Silicon Macs. Each release includes SHA-256
checksums and platform manifests. See [release setup and publishing](docs/publishing.md)
for the required Apple secrets and version tag instructions.

Each provider has its own connect and manual refresh buttons. Counters start empty.
After a successful quota reading, the application saves the latest snapshots,
measurement times, and connected providers between launches. Provider settings
persist too. There are no demo or fabricated quotas in the application.

The menu bar shows **remaining quota**, using the lowest value among primary
windows. Cards show **used quota** separately for each window. Older snapshots stay
visible with `~` in the menu bar and a stale data label in cards. Their age and
measurement time do not change after an error or restart. `—` means no measurement
or a disconnected account. Passing a reset time does not create a new quota
measurement. Refreshing preserves the current values until a new snapshot arrives.

## Interface language

In **Settings → Menu bar → Language**, select the system language or one of 25
languages: English, Russian, and all 24 official EU languages, including English.
Saving applies the choice to open windows, native menus, and notifications, and
preserves it between launches. Unsupported system languages fall back to English.
Times, numbers, and percentages use the selected locale.

Translations in `src/locales/*.json` are shared by TypeScript and Rust. Tests check
key completeness, interpolation parameters, settings migration, and layout across
all supported languages in the panel and settings window.

## Connecting providers

- **Codex:** install and authorize the official CLI, then select **Connect Codex**.
  Refresh from its card or tray menu.
- **Claude:** install Claude Code **2.1.296**, then select **Connect Claude**.
  If authorization is missing, **Sign in to Claude** starts the official
  `claude auth login --claudeai` command, which opens a browser. Complete sign-in
  there; the application then reads the quota automatically. Sign-in and readings
  can be canceled, and application exit waits for CLI termination.

On macOS, CLI discovery checks `/opt/homebrew/bin`, `/usr/local/bin`, `~/.bun/bin`,
`~/.local/bin`, `~/.npm-global/bin`, `~/.volta/bin`, and `/usr/bin`. It does not run
shell profiles or depend on the launch directory.

Official CLIs handle authorization, requests, and token storage. KnowYourLimits
does not open credential or cookie files. Claude reads structured `/usage` data
through the `get_usage` control request using stream JSON without a prompt; it does
not fall back to parsing percentages from text. An empty machine report means
the quota is unavailable.

When the usage endpoint returns HTTP 429, the application makes one minimal Haiku
request through the same CLI and reads the five-hour and weekly windows from rate
limit headers. This costs approximately $0.0001 per reading: `claude -p` spends
Agent SDK credit rather than plan quota. Without headers, the application remains
paused until the retry time. While HTTP 429 is active, readings use this request
directly. The CLI caches its prompt for an hour, making repeated readings cost
approximately $0.00001. Supporting the pinned CLI does not guarantee stability of
its undocumented machine format. See [details and limitations](docs/claude-spike.md).

## Settings and refresh

Settings open in a separate window with **Menu bar · Claude · Codex** tabs. Open
them from the panel's settings item or a card's gear button, which selects that
provider's tab. The menu bar supports **One row**, **Two rows**, and **Rings**.
Each provider has separate icon, color, size, refresh interval, notification,
sound, and recovery alert settings. Ordinary text is not selectable; input fields
remain editable. The interface follows the KnowYourLimits Claude Design project.

Refresh timers can be disabled or set to 1, 2, 5, 10, 15, 30, or 60 minutes.
**Auto** refreshes every five minutes when the panel was opened within the last
30 minutes and every 15 minutes otherwise, following Claude Desktop. Repeated
readings within one minute of success or 20 seconds of an error reuse the latest
value without launching the CLI. Timers run in the background after a provider
connects, including when the panel is closed.

Changing an interval applies and saves it immediately. The card shows the last
refresh time, and settings show the interval. If saving fails, the selection
returns to the saved value and displays an error. Other settings use **Save**.
Settings and existing alert thresholds persist. After a restart, connections and
the latest snapshot load from disk, and timers resume after the selected interval.
Reading the cache does not launch a CLI or send notifications.

Timers and notifications are disabled by default. Enabling notifications on macOS
requests permission; the test button sends a local notification. Alerts do not
repeat for each refresh of the same quota. When Claude is rate limited, the panel
shows the pause and retry time; clicks and timers respect that deadline. The last
snapshot and pause survive restarts. Write errors appear in the panel while the
current data remains in memory.

Automatic startup is not implemented. Snapshots become stale after five minutes
even with a longer refresh interval, but percentages remain visible as the last
known values. Notifications use only fresh readings.

## Installation and development

### Windows

Windows 10/11 x64. Development requires Visual Studio Build Tools with the
**Desktop development with C++** workload, Rust 1.90+, Bun, and WebView2 Runtime.

```powershell
bun install --frozen-lockfile
bun run tauri dev
bun run package:windows
```

Packaging creates `KnowYourLimits-0.2.6-windows-x64-setup.exe`,
`KnowYourLimits-0.2.6-windows-x64.exe`, and a SHA-256 `manifest.json` in
`artifacts/windows/0.2.6/`. The installer runs for the current user and installs
WebView2 when needed. Windows builds currently lack an Authenticode signature.
GitHub Actions builds and retains the installer and portable executable as
workflow artifacts, and publishes them from version tags.

Claude and Codex have separate tray icons with remaining quota rings. Symbols are
white on dark taskbars and dark on light taskbars. Ring colors follow the Windows
taskbar theme separately from the app theme, updating within five seconds without
a restart. Stale values retain their brightness and use dashed rings. System
colors mark remaining quota at or below 20% in yellow and at or below 5% in red;
custom ring colors come from settings. Historical values use dashed rings and `~`
in tooltips, while unknown quotas use gray dashed rings. Provider, symbol, color,
and size choices apply to the icons. Percentages appear in tooltips and the panel.

CLI discovery checks `~/.local/bin`, `~/.bun/bin`, `~/.cargo/bin`, `%APPDATA%/npm`,
WinGet Links, the Node.js directory, and absolute `PATH` directories. Native `.exe`
and official npm installations of Claude/Codex with Node.js are supported. npm
installations launch their JavaScript entry points directly without running `.cmd`
through a shell. CLIs use their Windows authorization; signing in only through WSL
does not authorize the Windows CLI. The Claude version remains **2.1.296**.

Settings and caches are stored in `%APPDATA%/dev.knowyourlimits.desktop/`. Native
menus follow the Windows system language. Cancellation and exit terminate the CLI
and its child processes through a Windows Job Object. Native notifications and
the three menu bar layouts are implemented on macOS; Windows uses square ring
indicators. The application runs as a single instance: launching an installed or
portable copy again opens the existing application's panel.

### macOS

macOS 13+ on Apple Silicon. Development requires Xcode Command Line Tools,
Rust 1.90+, and Bun:

```sh
bun install --frozen-lockfile
bun run tauri dev
```

A left click opens or closes the panel. Escape and loss of focus close it.
A right click opens the refresh and exit menu.

The release workflow uses `bun run package:macos:ci` to build and sign the Tauri
app, notarize and staple it, create a signed DMG, and verify Apple notarization
and Gatekeeper acceptance. Artifacts appear in `artifacts/macos/<version>/`.
Configure the Apple secrets as described in [publishing](docs/publishing.md).

For local Zamok packaging with an existing Developer ID identity and the
`notarize` Keychain profile or an App Store Connect API key:

```sh
bun run package:macos
```

This command checks notarization access, builds the Tauri `.app`, and runs
`zamokctl package`. Public signing settings are in `packaging/macos.json`.
Distribute the verified signed and notarized output.

Browser preview uses the same interface with empty quotas; provider connections
require the native app. Native settings are stored in
`~/Library/Application Support/dev.knowyourlimits.desktop/settings.json`.
Normalized snapshots and connections use `quota-cache.json` in the same directory.
Writes are atomic with mode `0600`; the file contains no tokens, cookies, or CLI
logs. Browser settings are stored separately in localStorage.

## Checks

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

Synthetic subprocess tests use `/usr/bin/python3`; the application does not need
Python. Default tests do not start real CLIs or sign-in flows. Playwright tests
the UI with mocked IPC, and native Tauri ACL tests use the real `RuntimeAuthority`.
These checks do not replace testing a physical macOS window.

Explicit readings use the application's coordinator and CLI discovery:

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --example live_read -- --live
cargo run --locked --manifest-path src-tauri/Cargo.toml --example live_read -- --live claude
```

This smoke check does not sign in and prints only normalized state. Append
`--persist` to read and save the application cache; close the application first.
See [0.2.6 changes](docs/releases/0.2.6.md). Historical specifications and reports
remain in `docs/`.
