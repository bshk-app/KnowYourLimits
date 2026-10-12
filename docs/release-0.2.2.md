# 0.2.2 — quota persistence between launches

Two causes of resets were confirmed: the coordinator held snapshots/connections
only in memory, and freshness rules hid percentages after five minutes. Before
the fix, `last_quota_and_connection_survive_restart_without_running_a_cli`
received zero windows after restart instead of one; the TypeScript test for a
15-minute snapshot received `C — · X —` instead of historical percentages.

- `quota-cache.json` stores only normalized snapshots and connected providers.
  Writes after completed readings are atomic with mode 0600; errors appear in
  the panel. Settings, raw output, credentials, and in-flight jobs are not saved.
- Reads are bounded to 64 KiB and validate version, dates, percentages, windows,
  and duplicates. Loading a corrupt file leaves it unchanged and displays a notice.
- Restart restores snapshots with their original timestamps as history. Restoration
  does not launch a CLI. Selected timers resume for saved connections after the
  configured interval.
- Percentages remain visible when stale, refreshing, or after service errors.
  History uses `~` and dimming in the menu, with snapshot wording and age in cards.
  Notification freshness rules are preserved; cache loading sends no notifications.
  Passing a reset time does not create a new limit.
- Claude pause deadlines continue using quota-retry.json. An active pause from
  0.2.1 restores a previously connected provider during migration.
- `live_read --live [claude] --persist` explicitly enables the app-owned cache.
  Default tests do not launch real CLIs or make network requests.

Checks cover restart without a CLI, failure → restart → history, timer resumption,
write errors, privacy/bounds/corrupt caches, UI age → reopen, history during HTTP
429, and actual native rasterization. Signing/installation follows building.

## Verified and installed

110 library + 2 examples + 46 TS + 15 Playwright = **173 cases**.
Clippy with warnings denied, fmt, TypeScript, Biome, and production build passed.

Zamok signed and notarized the DMG. codesign --deep --strict, stapler, and
Gatekeeper passed for the consumer DMG/app, reporting Notarized Developer ID.
DMG: 3 488 763 bytes, SHA-256
`90eaebef2a8f97823b5f05f0e4f352db4f004e18ac8c9ed3dfd3fc478c7ea62b`.

Version 0.2.2 was installed at /Applications/KnowYourLimits.app. The previous
version exited through owned SIGTERM/awaited shutdown and was saved at:
`~/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.2.2-1791663195225.app`.

A real Codex snapshot was obtained through the production reader with explicit
--persist: 91% used, fetchedAt 1791663165562. Cache restoration through the actual
coordinator restored `C — · X ~9%`, the original timestamp, and both connections
without executing the CLI or changing the cache file. The settings checksum was
unchanged before/after installation:
`c6fa2631291f8020d9f2466440b290caa7c199ca82d73dcc31daac17a1e5c7b3`.
No Claude quota GET was performed; its existing pause was preserved.

The application launched through CUA, with PID 20023 active. Native AX capture
timed out, so the physical window was not confirmed. Playwright checked mock IPC
UI, actual CoreText/CoreGraphics checked rasterization, and the production
coordinator checked cache behavior. The temporary cache probe was removed.

Artifact: `artifacts/macos/0.2.2/KnowYourLimits-0.2.2.dmg`;
port 55999, session 70146. The consumer download hash matches the DMG;
old links redirect to 0.2.2.
