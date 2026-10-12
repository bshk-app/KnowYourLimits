# Codex — read-only P1a spike

## Purpose

The next step after P0 proves subscription quota retrieval through the official
Codex CLI. It is a separate on-demand reader, not live data in the existing demo
UI. P0 and browser preview continue to show only explicitly labeled demo data.

## Scope

- The measured supported version for this step is the installed `codex-cli 0.162.0`.
- Use only stdio `app-server`, without a daemon/WebSocket, shell interpolation, or direct HTTP API.
- Handshake: `initialize` → `initialized` → `account/rateLimits/read`.
- Do not create threads/turns, run inference, call account/read/login/logout, or
  read/rewrite credentials, user configuration, or history in application code.
  Official Codex uses its existing authorization itself.
- Use a separate neutral working directory and disable child CLI telemetry through
  per-process overrides. Verify startup side effects against official source before
  a real request.
- Do not retain raw RPC/stderr, email, account IDs, tokens, or real quota fixtures.
- Do not add polling, notifications, automatic startup, new authentication UI, or a Claude adapter.

## Narrow contract

The reader accepts the path to a trusted installed Codex executable from a Rust
or example caller, not a new frontend-controlled path. It returns a normalized
`ProviderSnapshot` or a typed safe error. The example prints only a quota snapshot
or static error category; real responses are not written into the repository.

- Percentages use native `usedPercent`, without local usage/cap calculation or clamping.
- Resets use checked conversion from Unix seconds to milliseconds.
- Labels follow the actual `windowDurationMins`, not primary/secondary position.
- Support legacy `rateLimits` and current `rateLimitsByLimitId`. Only the confirmed
  general Codex bucket contributes to menu quota, not code review or unknown model limits.
- Nullable/missing windows and non-ChatGPT quotas are unavailable, not 0% remaining.
- Invalid responses and RPC errors do not expose raw message/data. Errors do not imply exhaustion.
- Expired resets do not fabricate recovered quota; P0 freshness/counter logic remains
  responsible for minimum remaining quota.
- `ordinaryUsageAllowed=false` blocks a current menu counter even with quota remaining.
  Nullable permission stays unknown rather than becoming `true`.
- Positive duration can be nullable. Unknown duration uses sentinel `0` and an honest
  label in the P0 DTO; this is not a measured zero-length window. Percentages stay native.
- A present map is authoritative: if `codex` is absent, do not use the legacy fallback,
  which may contain the server's first unrelated bucket.

## Resource limits

- One request without automatic retries. The 15-second RPC deadline includes handshake
  and writes, followed by a separate cleanup allowance of up to two seconds.
- Bound stdout frame and total bytes; drain and discard stderr to avoid filling its pipe.
- On EOF, oversized output, protocol errors, or timeout, kill the scoped process group
  and await child exit. When the future is canceled, Drop synchronously kills the group
  and schedules reaping on the still-running Tokio runtime; Drop cannot await.
  The caller must keep the runtime alive for reaping. Graceful cleanup after owner
  SIGKILL or OS shutdown is not guaranteed. Normal completion/error/cancellation must
  not leave an app-server daemon. Tests do not run the real CLI by default.

## Verification

1. Obtain a live reading without inference; report only success, version, and quota fields.
2. Test-first normalization: flat/map, nulls, unknown buckets, duration, Unix units,
   invalid percentages/timestamps, API/no quota, and absence of raw error leakage.
3. Fake stdio subprocess tests: handshake, unrelated notifications, EOF/malformed
   output, blocking/timeout, large stdout/stderr, and child cleanup for success/error/cancellation.
4. cargo fmt/test/clippy, P0 regressions, and independent correctness review.
5. Record actual limitations. Tray/panel integration is a separate next step after
   the spike succeeds and the UI explicitly separates live and demo sources.
