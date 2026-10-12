# P0 prototype verification

## Completed

- `bun run test`: **34 unit tests** for the quota model and preview settings bridge.
- `cargo test`: **13 tests** for the native model, camel-case DTOs, and atomic settings storage.
- `bun run test:e2e`: **6 Chromium tests** covering both providers, minimum remaining
  quota, critical/stale/error/unavailable states, persistence after reload, drafts
  and focus during timers, freshness expiration, validation, phone/dark layouts,
  and absence of external account requests.
- Strict TypeScript, Biome, cargo fmt, and clippy `-D warnings` passed.
- Vite production build passed: approximately 18 KB JavaScript and 8 KB CSS before
  gzip, not the application size.
- Tauri debug bundle: `src-tauri/target/debug/bundle/macos/KnowYourLimits.app`,
  approximately 25.31 MiB. This is a debug `.app`, not an optimized release or
  notarized public distribution.
- The final packaged app launched: finishedLaunching, ActivationPolicy::Accessory,
  one process. The smoke-test process was stopped afterward.
- Launching the previous smoke build twice left one instance after initialization;
  an immediate probe could observe a second short-lived process.
- Preview server on port 4173 returned HTTP 200 locally and with the remote BB share URL's Host.

## Review and deliberate behavior revision

Independent review found a race in visibility-based toggling: macOS focus loss can
hide the panel before tray mouse-up, causing the callback to reopen it incorrectly.
P0 was deliberately simplified: **left click always opens the panel**, without
toggling; Escape, loss of focus, and closing hide it. The change is marked REVISED
in [the specification](specs/prototype.md). The follow-up review found no defect in
this minimal change; it does not prove that all bugs are absent.

## Incomplete verification

- Physical native tray clicks, visual focus, clipboard, multiple monitors/DPI,
  and panel boundaries require manual macOS QA. Browser tests do not replace it.
- Screen Recording permission was absent. Native screenshots were not captured,
  and permissions were neither requested nor changed. Browser screenshot:
  `artifacts/prototype-dark.png`.
- Native storage settings were tested with unit tests; real WebView IPC/actions
  in the packaged `.app` have not been verified end to end.
- Notifications, automatic startup, real CLI adapters, live polling, and alert
  deduplication across restarts are not implemented. All percentages are fictional,
  and the application, tray, and UI are labeled DEMO.
- macOS 13 is the configured minimum, not a claim of testing every macOS version.

## Preview

Run `bun run preview` for local preview. The server is the background task
`kyl-prototype-preview`; ending or restarting the session may stop it.
Run `bun run build && bun run preview` to start it again.
