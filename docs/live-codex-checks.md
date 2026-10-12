# P1b: live Codex verification

## Automated verification

- 42 TypeScript tests; strict TypeScript and Biome passed.
- 13 Playwright tests: seven browser-preview and six synthetic native IPC/event tests.
- 57 Rust library tests and one opt-in example test; `--locked --all-targets`,
  fmt/check, and clippy `-D warnings` passed. **113 automated test cases total**.
- P0 settings/order/freshness/expired-reset semantics are preserved; the browser
  remains demo-only. Native mocks verify connection, weekly labels, disconnected
  Claude, draft isolation, refresh/return-to-demo, and rejection of late revisions.
- Fake stdio/coordinator tests verify a single job, historical timestamps, settings
  merging, cancellation, mode/shutdown races, and child/helper termination.

## Real reading

The opt-in `live_read --live` example uses **the same production coordinator and
resolver** as IPC/tray. Running with `PATH=/usr/bin:/bin:/usr/sbin:/sbin` passed:
`mode=live`, `refreshing=false`, current Codex with a weekly window, empty Claude
without a timestamp, and no notice. The tested CLI was Homebrew `0.162.0`.
No real raw RPC/stderr, identity, or credential fixture was retained.

The first smoke selected a broken duplicate in a user bin and correctly returned
Error/`—`. Discovery was fixed to prioritize known Homebrew bins over user bins,
using metadata rather than shell/version probes, without fallback/retry readings
through another CLI. The ordering regression passed RED/GREEN. Global PATH and
CODEX_HOME are unchanged.

## Findings and fixes

- GUI signal listener installation now follows `manage(RuntimeState)` so an early
  signal cannot invoke a callback before the managed state exists.
- The specification audit found a conditional cleanup gap: after two seconds, the
  managed reader could delegate reaping to Drop and complete before process exit.
  Managed cleanup now **retains ownership until wait confirms exit**, including
  OS wait errors with backoff, not new RPC/CLI calls. The standalone reader keeps
  its bounded cleanup. The regression reproduced RED at 2.01 seconds, then GREEN.
  An OS failure can make managed cleanup wait longer; force-kill does not guarantee cleanup.

## Native build and UI

- Debug `.app` built at `src-tauri/target/debug/bundle/macos/KnowYourLimits.app`,
  **27.27 MiB**. It is not a notarized or optimized release.
- Two packaged executable launches with minimal PATH ran for three seconds. No
  Codex child was found at startup. SIGTERM and SIGINT caused graceful exit with
  codes 143 and 130 rather than OS signal termination, without panic. Smoke
  processes were stopped.
- This signal smoke had **no active GUI reading**. Active cancellation and child/helper
  cleanup were covered by synthetic managed tests. Physical tray clicks, packaged
  WebView IPC, focus/DPI, and multiple-monitor UX still require manual QA.
- Focused correctness review found no confirmed bugs. The specification audit was
  **GREEN** after the managed cleanup fix. This does not prove all errors are absent.
- macOS Screen Recording/Accessibility permissions were unchanged.
  See [the manual checklist](live-codex-qa.md).

## Outside this stage

Polling, a Claude reader, notifications, login UI, automatic startup, and arbitrary
executable selection. Live mode/session consent does not persist; settings do.
