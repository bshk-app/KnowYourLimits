# 0.2.0 — compact menu bar and provider settings

Ordinary panel text is no longer selectable. Input fields retain selection,
replacement, and keyboard editing commands. Buttons use a normal arrow cursor,
following native panel behavior.

## Menu bar

Three actual variants are available in settings:

- **Row:** provider symbol beside the remaining percentage.
- **Two rows:** symbol/label above, percentage below, following the user's reference.
- **Rings:** two compact indicators showing remaining quota.

Each provider has independent settings: a symbol, letter, dot, or no icon;
system/custom color; and small, medium, or large size. Settings open through the
corresponding card's gear button. Preview shows actual current quota or `—`.

The native menu bar renders through CoreText/CoreGraphics into RGBA at 2x, using
the system font and a 22 pt height. Width is reserved for 100% so adjacent items
do not shift as values change. System color uses a macOS template image; custom
colors use a color image, redrawn after theme changes. Rendering is cached by
settings/values/theme, not revision. NSStatusItem explicitly receives a logical
image size because tray-icon defaults to an 18 pt image height limit.

`examples/render_tray.rs` exports the production renderer with empty startup
state. PNGs in `artifacts/design/native-tray/` were visually checked; white-background
previews are only for QA of the transparent image. Neither runtime nor export
tools contain fictional quotas.

## Automatic refresh

Claude and Codex have separate intervals: disabled, 1, 2, 5, 10, 15, 30, or 60
minutes. Disabled is the default. Timers operate after explicit account connection
in the current application session, including with the panel closed. Connections
must be reestablished after restart; intervals and other settings persist.

One owned ticker checks deadlines every five seconds. It does not queue parallel
CLIs, skips busy state, chooses the earliest deadline, and does not catch up after
sleep. Intervals start after reading completion; errors also defer the next attempt.
An interval change starts a new period. Disabling a timer cancels future automatic
readings; a separate button cancels the current request. Exit cancels and awaits
the ticker and owned CLI.

Native get_state/focus remains read-only. Automatic refresh does not start login
or generation. Snapshots older than the existing five minutes or with expired
resets do not become current merely because a longer interval is configured.

## Notifications

Each provider separately configures enablement, sound, quota recovery, and used
percentage thresholds. Notifications are disabled by default. First enablement
requests actual macOS permission. The test button sends a local notification
through the same native service.

The implementation uses UserNotifications and a retained stateless delegate that
allows banners while the panel is open. Permission/delivery callbacks are awaited
through oneshot, without raw NSError/identity in IPC. UserNotifications initializes
only when invoked from a real .app bundle; unbundled development startup does not
call this API automatically.

Warnings use only a new current primary snapshot. Several thresholds crossed at
once produce one notification. Repeated readings of the same window/threshold in
the session do not repeat warnings; fractional reset differences normalize to a
minute. Color/size/sound changes do not reset deduplication. Recovery requires a
new valid measurement, not an expired timer. Errors, stale data, future timestamps,
and missing windows do not fabricate recovery or 0%. Notification Center keeps
the most recent notification for each provider.

Physical banner delivery, system permission dialogs, and sound require user
verification in the installed version. Default tests do not request permission or
send real system notifications. Browser native tests use mock IPC rather than
verifying the packaged WebView.

## Persistence and verification

Existing 0.1.2 settings.json files load without losing order/thresholds or being
rewritten at startup. New fields receive safe defaults. Invalid colors, enums,
intervals, and notification flags are rejected. Writes remain atomic.

99 Rust library + 2 example + 44 TypeScript + 13 Playwright = **158 cases**.
Coverage includes migration, independent settings, timers/errors/single-flight,
notification deduplication/recovery, drafts, layout selection, and disabled ordinary
text selection with retained input editing. Native rasters are checked for size,
visibility, transparency, color, and template mode.

Builds use existing `zamokctl package`, Developer ID Q8H6GWJ658, and the `notarize`
Keychain profile. Signing/notarization and the final artifact are verified separately
after building. No public Zamok/GitHub release is created.

Primary interfaces:
[CoreText](https://docs.rs/core-text/22.0.0/core_text/font/struct.CTFont.html),
[CoreGraphics](https://docs.rs/core-graphics/0.25.0/core_graphics/context/struct.CGContext.html),
[UserNotifications](https://developer.apple.com/documentation/usernotifications),
[Tauri tray API](https://docs.rs/tauri/2.12.2/tauri/tray/struct.TrayIcon.html).

## Final installation

Final DMG: 3 465 674 bytes, SHA-256
`17bf573ff015757c96ec7b5efc4bda9e88d38601e27ac7e5e6adaaa4e3f3bb75`.
Apple notarization and stapling were confirmed; codesign --deep --strict and
Gatekeeper accept the DMG and its embedded application.

`/Applications/KnowYourLimits.app` was updated to version 0.2.0. The previous
bundle exited correctly and was saved at:
`~/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.2.0-1791659651320.app`.
The new version launched; PID 6615 stayed active for more than a minute.
Computer Use returned timeoutReached when capturing the panel, so physical
permission/notification clicks were not confirmed. Renderer PNGs and mock IPC UI
were checked.

Download server: port 55999, session 85200. Old links redirect to final 0.2.0.
Intermediate bundles before rounding/Retina alignment are not distributed.
