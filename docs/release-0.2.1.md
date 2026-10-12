# 0.2.1 — correct handling of Claude rate limits

Reproduction: the user's installed 0.2.0 showed empty Claude quota and Check your
subscription. The same production coordinator reproduced the empty snapshot.
A read-only CLI probe showed one quota GET → **HTTP 429**; official `/usage`
returned result=success with zero model counters and no structured limits.
Old text percentages in the seeded fallback do not prove freshness.

This does not check whether a subscription exists. The earlier umbrella traffic
flag hypothesis did not cover HTTP errors: successful CLI completion does not
mean a successful quota GET. New real checks show a server-provided extended pause.

## Fix

- The pinned CLI quota stage uses `--debug-file /dev/stderr` for narrow diagnostics.
  Application code does not save debug stdout/stderr or pass them through IPC.
  The parser extracts only a usage HTTP 429 marker and numeric pause deadline,
  bounded to one hour. Strings, URLs, headers, identity, and authorization material
  are neither retained nor printed.
- Empty quota with confirmed HTTP 429 produces a typed rate-limit error. A late
  CLI logger write is awaited for up to 250 ms within the shared 15-second deadline;
  the owned process group is then killed before reaping, stderr is drained, and
  cleanup is awaited.
- Manual and timer retries are blocked until the server retry deadline. Without a
  deadline, use 300 seconds, the verified CLI 2.1.296 fallback. Codex continues
  independently. A successful new reading clears the pause.
- An empty response does not remove the last real snapshot or timestamp. They
  remain historical rather than becoming current or fabricating 0%.
- The panel shows a pause/countdown and the menu action is temporarily disabled.
  The incorrect subscription advice was removed. Time alone reenables the button
  without requiring another IPC event.
- `quota-retry.json` in the app-owned configuration directory stores only provider
  deadlines, preserving pauses across restarts. Writes are atomic with mode 0600;
  loading does not execute a CLI. Future deadlines are bounded to one hour;
  old/invalid values are ignored.
- Removed the duplicate No measurement footer.

## Verification

The synthetic subprocess regression first failed:
`http_429_is_an_error_even_when_cli_returns_success_with_empty_report`
converted HTTP 429 silently into a successful empty snapshot. It passed after the fix.

Coverage includes diagnostic scope/bounds/privacy, late logging, Retry-After,
reader → production mapper → cooldown, repeated clicks/timers without new CLI
launches, independent Codex operation, preserved history/timestamps, expiry, and
restart persistence. UI mocks check countdown, disabled/reenabled actions, and Codex availability.

106 Rust library + 2 examples + 45 TS + 14 Playwright = **167 cases**.
Default tests do not perform real quota GETs. After the fix, the production
example returned a typed server rate limit and `retryAt`; the server still returned
HTTP 429. This check did not obtain fresh percentages or claim quota recovery.

Last confirmed response deadline: Unix ms `1791663458948`. Installation transfers
this deadline into app-owned retry metadata so the new build also respects the
permitted request time.

CLI contract/flags:
[official CLI reference](https://code.claude.com/docs/en/cli-reference).
Logger/seeded fallback/retry constants were observed in the installed pinned
executable; they are not a public SLA for other versions.

Signing, consumer DMG, and installed version are verified separately after building.

## Result

Signed/notarized DMG: 3 466 261 bytes,
SHA-256 `773c5b8780ad0d762fccea98ee2e778a31ddc315721a56e673a289386834045b`.
The consumer DMG/app passed codesign, stapler, and Gatekeeper assessment.
Version 0.2.1 was installed at /Applications/KnowYourLimits.app; backup:
`~/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.2.1-1791661463341.app`.
Deadline 1791663458948 was written to app-owned quota-retry.json without a new
quota GET. The application launched and PID 64062 stayed active. CUA capture timed
out, so the physical window was not confirmed; mock IPC UI and the actual
production reader were checked. Download server: port 55999, session 99981;
previous links redirect to 0.2.1. All 167 cases, Clippy with warnings denied, fmt,
TypeScript, Biome, and production build passed.
