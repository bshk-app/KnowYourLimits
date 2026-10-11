# Manual native QA — P1b

This is a checklist, **not a record that these native UI actions passed**.
Browser mock tests cannot prove AppKit/WebView/tray behavior.

## Setup

- Build/open the new `.app` (README), not an old P0 bundle.
- Keep the official Codex CLI installed and authenticated with its own login flow.
- Launch from Finder with a minimal PATH as well as from a terminal.
- Do not enable Screen Recording/Accessibility just for automated testing unless
  you explicitly choose to grant those permissions.

## Native actions

1. First launch: DEMO badge/tray marker, no CLI started; both fake counters labelled.
2. Open panel with tray left click. Read startup-side-effects disclosure, activate
   Codex. Both demo snapshots disappear immediately; Claude is «Не подключён».
3. Successful read: actual Codex windows/durations, minimum remaining in tray;
   LIVE label, no DEMO wording on real quota rows. Weekly-only is not called 5h.
4. Refresh simultaneously from panel/tray: one child group, no queued second read.
   Refresh disabled/busy; settings and return-to-demo remain usable.
5. Edit but do not save thresholds while read finishes: draft stays untouched.
   Save while reading: completion preserves saved order/thresholds.
6. Return to demo while reading: child group cleans up, fake snapshots reappear
   explicitly labelled; no late result changes them back to live.
7. Leave live without refresh >5 minutes or past reset: current counters become
   `—`; past rows stay historical. No automatic CLI launch/request occurs.
8. Missing CLI/login/network/read error: static notice, current counter `—`, no
   fake fallback and no raw account/error output.
9. Escape/blur/close hides the panel, not the app. Reopen shows latest backend state.
10. Quit while reading (and normal Unix termination signal): owned read cancels
    and child/helper processes finish before app exits. SIGKILL/power loss is
    outside graceful cleanup guarantees.
11. Restart: demo again, saved counter settings preserved, no automatic live read.

## Scope limits

No polling, Claude reader, notifications, login UI, arbitrary executable picker,
or autostart. Executable discovery supports documented known installation bins;
nonstandard installs are not silently searched via shell profiles/cwd.
