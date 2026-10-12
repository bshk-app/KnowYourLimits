# 0.2.4 — native design and settings window

The interface was adapted from the KnowYourLimits Claude Design project:
“Make the design as close to a native app as possible.”

- **Settings is a separate window** with Menu bar · Claude · Codex tabs
  (`settings.html`, `src/settings.ts`). Open it through Settings… ⌘, in the panel
  or a card's gear button to select that provider's tab. An already open window
  preserves its draft and only switches tabs via `settings-tab`. Window height
  follows content. ⌘W and Cancel close without saving. Refresh intervals still
  apply immediately.
- **Panel:** system grays, provider icon tiles, status dots, large remaining quota,
  and gear buttons. Window bars have threshold ticks; card footers show Snapshot ·
  2 min ago · auto 5 min and Refresh. During readings, the same button cancels
  refresh. Number color follows the worst nonexpired window.
- **AppKit controls:** segmented icon/size/color controls, NSSwitch, and pop-up menus.
- Differences from the mockup: stale data retains its last snapshot percentage;
  the Previous snapshot badge is neutral; the panel window is opaque.

## Native

`open_settings` is synchronous: Tauri runs it on the main thread, so window lookup
and creation cannot overlap. The `settings` window uses `TitleBarStyle::Overlay`,
a hidden title, and a toolbar drag region. Focus-loss hiding and
`CloseRequested → hide` apply only to `main`; settings closes normally. Its
`settings` capability permits events, dragging, `set_size`, and `close`; the panel
does not have those permissions, as checked by `permissions_tests`.

## Verification

Rust 122, TS 50, Playwright 27 against a `file://` build without a development
server; Clippy, Biome, typecheck, and production build. Cross-model review (Codex):
expired windows colored cards, RED → GREEN; double-click races were addressed by
the synchronous command; browser popup tab selection went RED → GREEN. Losing a
tab during window loading was rejected as a finding because it was unreachable manually.

## Signing and installation

`bun run package:macos` built the application, but notarization failed because
Keychain lacked the notarytool `notarize` profile: No Keychain password item found.
`zamokctl notary-profiles list` was also empty outside the sandbox. No DMG was created.

For local installation, the app was signed with the same Developer ID
(Q8H6GWJ658), hardened runtime, and timestamp, without entitlements, as in 0.2.3.
`codesign --verify --strict --deep` passed. Without notarization, Gatekeeper rejects
the downloaded copy; the local installation is not quarantined.

Version 0.2.3, PID 91017, exited through SIGTERM. Backups are in
`~/Library/Caches/KnowYourLimits/app-backups/`:
`KnowYourLimits-before-0.2.4-1791742526.app` and `settings-before-0.2.4-1791742526.json`.
Version 0.2.4 was installed at `/Applications/KnowYourLimits.app` and launched,
PID 51389. Binary SHA-256:
`8b50a7a7f98c1152097de5a67cf9b44cf689bbddbdbabd74eb484e05803ad978`.
`/settings.html` and `assets/settings-*.js` are embedded in the binary.

The native settings window was not verified: osascript lacked Accessibility
permission, and Kuroko lacked a license. No physical clicks were performed.

Restore the notarization profile with `xcrun notarytool store-credentials notarize`,
then run `bun run package:macos --skip-build`.

## Compact settings window: 0.2.4 rebuild

Following feedback about excessive empty space: width reduced from 480 to 420 pt;
tab toolbar lowered with 18 pt icons; group rows use 5 px instead of 7; layout
thumbnails use 32 pt instead of 40; body/footer spacing reduced. Menu bar tab
height changed from approximately 400 to 307 pt, and provider tabs from 611 to
548 pt. Rebuilt, signed with the same Developer ID, and reinstalled, PID 74298.
Binary SHA-256:
`7ee838e7a213f6f56a14ac413b9d7f7605f4bbc8cf65d3c8f2ca89b5bc33e7b8`.
The first 0.2.4 build is in `app-backups/KnowYourLimits-0.2.4-first-build-*.app`.

## Closer two-row columns: 0.2.4 rebuild

Columns reserved width for ~100% and centered their text, placing spare space
inward: 34 pt between dashes and approximately 16 pt between ~66% and ~95%.
Column widths remain unchanged so the icon does not jump, but the pair now aligns
toward the center: the left column aligns right and the right column aligns left,
placing spare space outside. Columns are separated by 6 pt. The raster-based
`stacked_columns_sit_close_together` gap test went RED at 68 px → GREEN. Rust 123.
Reinstalled, PID 84198; the previous build is in
`app-backups/KnowYourLimits-0.2.4-compact-*.app`.

## Repeated menu bar clicks: 0.2.4 rebuild

Left click toggles the panel, opening it when hidden and hiding it when open.
State is captured on mouse-down; the action runs on mouse-up, so focus loss
between them does not turn closing into reopening. If AppKit removes focus before
the mouse-down handler, a click over the icon is checked with `NSEvent` and the
status item's window frame in shared screen coordinates. Escape, outside clicks,
and switching applications still hide the panel. Right click opens the existing
menu; the settings window is unaffected.

Rust: 124 library and 2 example tests, Clippy `-D warnings`, and fmt;
TypeScript and production Tauri build passed. Automated physical clicks are not
confirmed: CUA cannot obtain application AX state (`timeoutReached`).

The updated local copy is signed with Developer ID Q8H6GWJ658, hardened runtime,
and timestamp; `codesign --verify --strict --deep` passed. The `notarize` profile
is still absent; no new notarization or DMG was created. Settings were preserved.
Previous copy:
`~/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-0.2.4-before-tray-toggle-1791744807637.app`.
Installed binary SHA-256:
`01e4fcfb821f5856295c9e3ec78101da6c68e809a0769b8caafde929b2f7bf74`.

## Content-based panel height: 0.2.4 rebuild

The panel was always 620 pt tall, leaving empty space below Settings.
`ResizeObserver` now reports `.kyl-panel` height; `fit_panel` resizes the window
to 360 pt wide, at least 160 pt tall, and no taller than the screen's work area,
then restores its previous position so the top stays below the icon. `show_panel`
bounds position using the current height. Tests: Rust `panel_height` and e2e
panel window follows its content height. Rust 125, TS 55, Playwright 30;
one additional localization test hardcodes `http://127.0.0.1:1420` and cannot run
through `file://`. Reinstalled, PID 96880, SHA-256:
`f16be385241e1029fdd358bbf41415084daa6bf7b30f866ea0408af37eab32cb`.
Previous copy: `app-backups/KnowYourLimits-0.2.4-before-panel-fit-*.app`.
