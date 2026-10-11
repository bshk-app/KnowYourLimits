# P1c — standalone Claude quota reader

Continue the source-verification roadmap before adding Claude to native LIVE.
Verified candidate: official Claude CLI **2.1.296**, non-interactive `/usage`.
No PTY or terminal-text parsing; no direct OAuth/API client or credential reads.

## Boundary

- One explicit opt-in standalone read; no app startup execution, native IPC/UI,
  polling, notifications, login, configuration edits, or executable discovery.
- Caller supplies a trusted absolute executable and child-specific PATH.
  Canonicalize it before version/read so an updated symlink cannot change the
  binary between them. Trust the installed binary, not cryptographic provenance.
- Verify `--version` equals `2.1.296 (Claude Code)` before sending `/usage`.
  Unsupported versions stop; no guessed compatibility, PTY or inference fallback.
- Use `--safe-mode --print --output-format stream-json --verbose`, empty tools,
  strict empty MCP config, no Chrome, no session persistence, `--max-turns 0`,
  inline hook/connector/auto-continue restrictions. Built-in `/usage` must remain
  available; **do not** use `--disable-slash-commands`, which also removes local
  commands in this pinned implementation. `--bare` drops subscription auth.
- Disable nonessential traffic, telemetry/error reporting, updates, background
  tasks and auto memory per process. Never modify global PATH/HOME/config/auth.
- Safe mode still honors admin-managed policy, including hooks. CLI owns auth,
  policy/network, utilization caches and possible local activity scans. No claim
  that startup is side-effect-free or that zero CLI counters audit all networking.

## Stream / quota contract

- Require one pinned-version system init with empty tools and MCP servers; one
  synthetic assistant `/usage` response identified by `local_command_run.command`;
  one success result identified by `local_command=usage`. Reject reordered or
  duplicate meaningful frames, arbitrary assistant/model output, errors and
  nonzero turns/cost/API duration/tokens/model usage.
  Ignore only bounded `system/ui_invalidate` notifications, including before init.
- Only parse `usage_report.rate_limits.limits`. Never parse terminal text, local
  behaviors, session costs, `extra_usage`, account identity or model usage as quota.
- Recognize global `session` (5h) and `weekly_all` (week); ignore other kinds.
  Each recognized window needs valid finite 0–100 percent, matching group,
  nullable RFC3339 reset and no non-null model/surface scope. Reject duplicates.
- `is_active` is not an availability flag: the real stream contains an inactive
  global session window with valid usage. Both global windows contribute to MIN.
- Missing/null report/limits or only scoped windows → unavailable, never fake zero.
  Bad schema/reset/percent → static error. Historical text cannot restore current
  windows when CLI strips `limits` from its seeded stale-cache fallback.
- CLI's fresh-cache path accepts snapshots under 60s but omits observation time.
  Set snapshot `fetchedAt` conservatively to **read start minus 60s**; never claim
  exact server observation time. Existing stale/reset semantics remain in effect.
- Discard raw stdout/stderr after bounded in-memory processing; only normalized
  fixed-label DTO or static error categories leave the reader. No raw fixtures.

## Resource ownership / verification

- A shared 15s operation deadline covers version and quota subprocess exchanges.
  Bound frame/stream/count/stderr; drain both pipes concurrently. No retries.
- Isolated process group, null stdin. Kill owned group before first wait/reap to
  retain PID reservation. Await cleanup on success, error, timeout and cancellation;
  retry OS wait errors with backoff until exit confirmation. Caller must await the
  cancellation future, not abort it. Drop is only emergency best-effort cleanup.
- Tests use synthetic processes, never the installed CLI/credentials by default.
  Cover pinned-version gate, protocol/privacy/schema, exact arguments, bounds,
  EOF, deadline, cancellation before launch and during each stage, child/helper
  cleanup, unchanged environment and conservative freshness.
- Example requires `--live <absolute-path>`, installs signal handlers first and
  awaits cancellation/cleanup before exit. One opt-in live run of the final Rust
  reader, locked Rust tests/fmt/clippy; preserve existing regression suite.

Official behavior references and pinned local-code findings are recorded in
`../claude-spike.md`; the native integration remains the next separate stage.
