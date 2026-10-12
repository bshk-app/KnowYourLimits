# P1b — native manual Codex integration

Scope: continue the approved demo → real-reader → native integration roadmap.
The P0 and P1a documents remain historical; this is the integration contract.

0.1.1 amendment: native startup is now `ready` with empty providers and no CLI
execution; demo is an explicit action. DTO accepts `ready|demo|live`. The rest of
the single-flight/consent/cleanup contract remains unchanged. See
`../release-0.1.1.md` for the shipped capability regression and signing checks.

## Behavior

- Native startup remains **demo**, no discovery/execution of Codex at startup.
- Explicit **Connect Codex** action discloses CLI-owned auth/network/cache
  side effects and requests one read-only account/rateLimits/read. No inference.
- Global `mode: demo | live`, **session-only**, not persisted. Counter/threshold
  settings keep their existing persistence. No auto polling, retries, login UI,
  notifications, autostart, Claude reader, executable-picker or shell commands.
- Enter live clears ALL demo snapshots before starting the read. Claude stays
  unavailable with empty windows and no timestamp; its UI says “Not connected”.
- Live refresh preserves only a previous live Codex snapshot as historical;
  it never renews its timestamp. While refreshing, its status is not Current.
  Error/no quota/expired/permission=false → no current percentage, never fake zero.
- Both panel and tray refresh use one backend single-flight coordinator; a second
  request during an active read does not queue or spawn another process.
- Return to demo cancels the managed read and regenerates both demo snapshots.
  Mode generation guards prevent late live results contaminating demo.
- Closing/blur hides the panel without stopping a read; ExitRequested (menu quit
  included) prevents exit until active read cancels/cleans up. SIGKILL/power loss
  cannot guarantee cleanup. Stopping disallows new work; duplicate exit is safe.

## IPC / DTO

Extend AppState (Rust/TS) with required camelCase fields:
`mode: "demo"|"live"`, `refreshing: boolean`, `revision: integer >= 0`.
Revision monotonically increases for all mutations, including settings and refresh
completion. Frontend ignores older revisions from events/command responses.
Existing providers/settings/scenario/notice fields remain unchanged.

Commands (no executable paths or arbitrary args):
- get_state, save_settings, hide_panel remain.
- begin_live_codex -> AppState: commit live/empty immediately, start owned job,
  return promptly (not a 15-second blocking command).
- refresh_codex -> AppState: live-only, start owned job or coalesce.
- return_to_demo -> AppState: cancel/reap active job before resolving.
- set_demo_scenario, refresh_demo: demo-only (reject while live).
- `state-changed` event carries complete normalized AppState. Subscribe before
  initial get_state; use revision guard and preserve unsaved settings drafts.
  Events/errors must never contain raw RPC, credentials, account IDs or paths.

Browser preview remains demo-only: live controls unavailable, no browser network
requests. Add runtime validation at native IPC/event boundary, not unchecked casts.

## Native coordinator and executable discovery

Use one owned coordinator shared by tray and IPC, with active job/cancel handle,
mode generation, stopping state. Never hold synchronous mutex across await; clone
state before publication; callbacks must run outside locks. Await managed reader
cancellation (rather than abandoning its future) so its process-group cleanup runs.
On refresh completion merge only Codex, preserving latest settings/revision.

Discovery is Rust-owned metadata-only after explicit activation. Prefer managed
Homebrew locations /opt/homebrew/bin, /usr/local/bin before HOME/.bun/bin,
HOME/.local/bin, HOME/.npm-global/bin, HOME/.volta/bin, then /usr/bin. This prevents
an obsolete user-bin shadow taking precedence over a working Homebrew install.
Require absolute locations and regular executable file (symlinks allowed). Never cwd/relative PATH
entries or shell/profile reconstruction; no frontend-controlled executable path.
CLI location is not cryptographic provenance: user-installed CLI is trusted.

For Finder launches, child-specific PATH consists of these known absolute bin
locations plus /bin, /usr/sbin, /sbin; support npm/Bun's /usr/bin/env node launcher.
Do not change global environment or CODEX_HOME. Nonstandard installs fail with a
static actionable notice, not a guessed percentage. Unix implementation only;
unsupported targets must compile or give a static unavailable outcome.

Extend existing reader minimally with managed cancellation + child PATH support;
keep standalone read and all resource/privacy constraints unchanged. Cancellation
select must flow through existing awaited cleanup; do not abort the job on exit.
Managed cleanup retains owned child/reaping until the OS confirms exit, even beyond
standalone's 2-second cleanup allowance. OS wait errors keep cleanup pending rather
than declaring successful mode/exit. This is kernel reaping, not a CLI/quota retry;
force-kill remains available if the OS cannot complete graceful cleanup.

## UI

Dynamic DEMO/LIVE badge, subtitle, card descriptions/age/ARIA, footer, window/tray
labels/menu status. Live counter is minimum remaining among actual usable windows;
weekly-only primary stays weekly. Manual refresh shows busy state, duplicate
refresh disabled; return-to-demo remains usable while reading. Settings are editable
and never overwritten by background completion/focus/timers. Panel stays compact,
existing one/two-counter order unchanged. No decorative/provider redesign.

## Verification

TDD vertical slices via real public coordinator/bridge/UI seams. At minimum:
startup has zero execution; live transition has no demo values; single flight
across simultaneous callers; setting changes preserved on completion; mode change
cancels child/helper and excludes late completion; stop denies new jobs and awaits
cleanup; missing CLI static error; permission false/no quota/error non-current;
revision/events cannot revert newer UI state; browser never activates live.

Discovery synthetic dirs/files only (no real CLI default tests). Existing fake
stdio harness verifies managed cancellation/PATH. Full locked Rust tests + fmt +
clippy, TS tests/typecheck/Biome, Playwright demo + mocked native IPC/events, native
build. One opt-in live smoke through the same coordinator/discovery used by GUI;
record only normalized quota fields. Real native tray clicks/focus/manual UX may
need human QA; do not equate browser mocks with native end-to-end verification.
