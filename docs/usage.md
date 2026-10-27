# Using KnowYourLimits

[README](../README.md) · [Development](development.md)

This guide describes version **0.2.6**. KnowYourLimits reads quota through the
official Claude Code and Codex CLIs. It starts with empty counters; it does not
display demo or fabricated quotas.

## First use

1. Install and authorize the official CLI for each provider you want to use.
   Claude Code must be **2.1.296** or newer; older versions are rejected.
   Codex uses its own CLI sign-in. Windows requires native Windows CLIs and
   authorization; signing in only through WSL does not authorize them.
2. Launch KnowYourLimits and left-click its menu bar or tray indicator to open
   the panel. Select **Connect Codex** or **Connect Claude**. Each provider
   connects and updates independently.
3. If Claude requires authorization, select **Sign in to Claude**. This starts
   the official `claude auth login --claudeai` flow. Finish sign-in in the
   browser; the app then reads quota automatically.
4. Read each card's used quota and reset time. Select **Update** on a card or
   use **Update Claude/Codex** in the right-click menu for another reading.
5. Open **Settings**, or select a card's gear to open that provider's tab.
   Choose appearance and update intervals. Auto-update and notifications are
   disabled by default.

Sign-in and readings can be canceled. Quitting waits for CLI termination.
Escape and loss of focus close the panel; the app continues running. Right-click
an indicator for the native menu and **Quit**. Launching another installed or
portable copy opens the existing app's panel instead of starting a second instance.
Automatic startup is not implemented.

## Reading the indicators

The menu bar or tray summarizes **remaining quota**, taking the lowest value
among the provider's primary windows. Cards show **used quota** for each window
separately. Claude's global five-hour and weekly windows are included when
reported; Codex's reported primary and secondary windows both participate in
the summary. Window lengths come from the reading; a weekly-only reading is not
presented as a five-hour limit.

| Display | Meaning |
| --- | --- |
| Current percentage or solid ring | A usable recent reading for that window. |
| `~` in menu bar text or tooltips; dashed Windows ring | A last known value, including while an update is in progress. |
| **Out of date** / **Previous value** in a card | Historical data, retained for reference. |
| **Usage blocked** | Codex reports that usage is blocked. Quota measurements can still be fresh; this does not establish a sign-in problem. |
| `—` / unknown ring | No usable measurement or a disconnected/signed-out account; not zero quota. |
| **Awaiting reset** | The reported reset time passed; a new reading is still needed. |
| **Reset time unavailable** | The provider did not supply a reset time. |

Readings become stale after five minutes, even when a longer update interval is
selected. Errors, unavailable responses, and restarts do not turn an old reading
into a new one or change its measurement time. Passing a reset time does not
reset a counter to zero used or 100% remaining. Refreshing keeps the existing
values visible until a new snapshot arrives. Restored snapshots are historical;
notifications require a fresh reading.

When Codex blocks usage, the card keeps the quota measurements and their original
timestamp. A fully used active window shows **Limit reached**; a block with quota
remaining shows **Usage blocked**. The remaining percentage describes the reported
quota, not permission to send a request. Blocked readings do not send quota recovery
notifications. Empty, expired, or failed readings ask for an update without assuming
that the account is signed out. Older percentages are labeled **Previous value**
beside the number.

Claude's usage response may reuse up to 60 seconds of CLI cache without an
observation timestamp. The app conservatively dates that reading to the start
minus 60 seconds, so it may show as current for only about four more minutes.
Quota percentages do not predict a number of remaining prompts or tokens.

### macOS menu bar

Choose **One row**, **Two rows**, or **Rings** in **Settings → Menu bar**.
Choose the first and second providers, or hide the second. Each provider has
separate symbol, color, and size preferences.

### Windows tray

Claude and Codex have separate square tray icons with remaining quota rings.
Percentages appear in tooltips and the panel. Provider selection, symbol, color,
and size preferences apply; the three macOS menu bar layouts do not.

Symbols are white on dark taskbars and dark on light taskbars. Rings follow the
Windows taskbar theme independently of the app theme; theme changes apply within
five seconds without restarting. Historical rings are dashed and retain their
brightness. Unknown quotas use gray dashed rings. With **System** color, remaining
quota at or below 20% is yellow and at or below 5% is red. A custom color overrides
the ring color.

## Settings and language

Settings open in a separate window with **Menu bar**, **Claude**, and **Codex**
tabs. A card's gear selects its provider tab. Ordinary interface text is not
selectable; input fields remain editable.

In **Settings → Menu bar → Language**, choose **System default** or one of
25 languages: Russian and all 24 official EU languages, including English.
Saving applies the choice to open windows, native menus, and notifications, and
preserves it between launches. Unsupported system languages fall back to English.
Times, numbers, and percentages use the selected locale. With **System default**,
Windows native menus follow the Windows system language.

Most settings apply with **Save**. Changing an update interval applies and saves
it immediately. If saving fails, the selection returns to the saved value and
shows an error. Existing alert thresholds and preferences persist.

## Updates and persistence

Each provider offers **Off**, **Auto: 5–15 min**, or an interval of **1, 2, 5, 10,
15, 30, or 60 minutes**. Auto updates every five minutes when the panel was opened
within the last 30 minutes, and every 15 minutes otherwise.

Timers run in the background after a provider connects, including while the panel
is closed. Repeated update requests within one minute of success or 20 seconds
of an error reuse the latest result without launching the CLI. A rate-limit pause
takes precedence over both manual and automatic updates.

After a successful reading, normalized snapshots, measurement times, and connected
providers are saved locally. On restart, those connections and historical values
load, and enabled timers resume after the selected interval. Loading the cache
does not launch a CLI or send notifications. Write errors appear in the panel;
current data remains in memory.

## App updates

The native app checks Zamok's stable channel once at startup. Check failures do
not interrupt quota readings; use **Check for updates** in the panel to retry.
Download an available update explicitly, then confirm installation and restart.
Downloads are verified against the pinned public key and the signed app version.
Active quota readers finish cancellation and CLI cleanup before installation.
Use the installed Windows application for managed updates; replace a portable
executable manually. Development builds are for local use.
App update messages currently use English; quota and settings translations remain available.

## Notifications on macOS

Notifications are optional and disabled by default. Each provider has usage
alerts, sound, and quota recovery preferences. Enabling them requests macOS
permission; **Test notification** sends a local notification.

Usage thresholds accept 1–5 distinct whole percentages from 1 to 100. Defaults
are **80, 95, 100% used**. Alerts do not repeat on every update of the same quota
and use only fresh readings. If permission was denied, enable KnowYourLimits in
macOS notification settings before testing again.

Native Windows notifications are not implemented in this version.

## Claude rate limits and cost

> [!IMPORTANT]
> A Claude update can incur a small model-request charge when the usage endpoint is rate limited. Consider this before enabling frequent automatic updates.

The normal path reads structured `/usage` data through a `get_usage` control
request using stream JSON, without a prompt. It does not infer percentages from
terminal text. An empty machine report means the quota is unavailable; any older
snapshot remains historical.

When the usage endpoint returns HTTP 429, the app makes one minimal Haiku request
through the same CLI and reads the five-hour and weekly windows from its rate-limit
headers. This `claude -p` request spends **Agent SDK credit rather than plan quota**.
Approximate observed costs for this path are **$0.0001 per reading**, or
**$0.00001 for repeated readings** while the CLI's one-hour prompt cache applies.
These are approximate observations, not a fixed price or a guarantee of caching.

While that endpoint cooldown is active, subsequent readings use the minimal
request directly. If usable headers are absent, the panel shows **Paused** and
the retry time. Clicks and timers respect that deadline; the last snapshot and
pause survive restarts.

Claude Code **2.1.296** or newer is accepted, but its undocumented machine format
may change in any release; an unexpected response is shown as unavailable or
failed, never as guessed percentages. The
[historical source investigation](claude-spike.md) explains why text
percentages are not a reliable fallback; some of its integration status
describes earlier versions.

## Storage and privacy

Official CLIs handle provider authorization, requests, and token storage. KnowYourLimits
does not open their credential or cookie files. The CLI itself can read its own
configuration and credentials, refresh authorization or caches, and perform
network activity. Claude's safe mode still permits administrator-managed policy
hooks. A quota reading is therefore not a promise of zero CLI side effects.

App update checks contact `zamok01.bshk.app` with app version, operating system,
architecture, and the stable channel. The server receives ordinary connection
metadata such as the IP address. No provider credentials, quota readings, or
account identity are included. Downloads contact Zamok's artifact storage only
when requested. Updates require no Zamok sign-in or license.

| Platform | App data directory |
| --- | --- |
| macOS | `~/Library/Application Support/dev.knowyourlimits.desktop/` |
| Windows | `%APPDATA%/dev.knowyourlimits.desktop/` |

The app stores `settings.json`, normalized snapshots and connected providers in
`quota-cache.json`, and retry deadlines in `quota-retry.json`. These files contain
no tokens, cookies, or raw CLI logs. Writes are atomic; macOS files use private
`0600` permissions. Browser-preview settings use separate localStorage and do not
authorize or connect providers.

## Troubleshooting

| Symptom | Next step |
| --- | --- |
| CLI not found | Check the [discovery paths](development.md#cli-discovery) and use an official supported installation. The app does not run shell profiles. |
| Claude version rejected | Update Claude Code to **2.1.296** or newer. |
| Sign-in required | Sign in with the provider's official CLI. For Claude, use the app's browser sign-in flow with a Claude subscription. |
| Windows account still signed out | Authorize the native Windows CLI; WSL sign-in is separate. |
| Usage unavailable or update failed | Check CLI authorization and network access, then update again. Historical values remain historical. |
| Claude paused | Wait for the displayed retry time; repeated clicks do not bypass it. |
| Old data after reopening | Select **Update**, or enable an update interval. Reopening alone does not refresh cached data. |
| Notification missing | Check macOS permission and use **Test notification**. Windows notifications are not implemented. |
| Provider cannot connect in browser preview | Run the native app. Browser preview has empty quotas and no CLI connection. |
