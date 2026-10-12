# Codex P1a spike

## Goal

Prove read-only subscription quota retrieval through official Codex and prepare an
isolated reader. The P0 panel and browser preview **remain demo-only**; this is not
yet a live monitor. See [the contract](specs/codex-spike.md).

## Confirmed findings

- Local `codex-cli 0.162.0` successfully answered `account/rateLimits/read` after
  `initialize` / `initialized`. No thread, turn, or inference request was sent.
- The real response contained only `primary` with a duration of **10080 minutes
  (one week)**, nullable secondary, and a map with canonical `codex`. Primary must
  therefore not be labeled five hours.
- Reset values are Unix **seconds**; normalization uses checked conversion to milliseconds.
- `ordinaryUsageAllowed` is a separate backend permission, not derived from percentages.
- Responses can contain `accountId` and unnecessary account/credits/banner fields;
  they must not enter normalized DTOs, diagnostics, or saved fixtures.
- The probe ran the CLI in a separate process group. A scoped group kill and wait
  for child exit closed stdio after reading. Raw RPC/stderr and account identity
  were not retained.

## Startup limitations

`app-server` **has side effects**. Without restrictions it can synchronize plugin
repositories, upgrade marketplaces automatically, update catalogs/caches, access
the network, and refresh CLI-owned authorization. Absence of inference does not
imply absence of background networking or CLI-owned cache writes.

This spike uses per-process overrides without changing user configuration:

- `analytics.enabled=false`;
- `otel.exporter="none"`, `otel.trace_exporter="none"`, `otel.metrics_exporter="none"`;
- `features.plugins=false`, `features.remote_plugin=false`;
- `features.hooks=false`, `features.apps=false`.

In the pinned source, plugin startup tasks are guarded by `plugins_enabled`,
derived from Feature::Plugins; the MCP processor constructor only stores fields.
The reader sends no MCP/thread/turn requests. This does not guarantee absence of
all CLI background tasks: configuration/authentication and model/cache startup
remain. Application code does **not** read configuration/credentials or assume
that `mcp_servers={}` clears layered configuration.

`CODEX_HOME` and authorization are inherited from the launch environment and
belong to the official CLI. BB/terminal and future Finder launches may have
different PATH/CODEX_HOME values. Use a trusted installed executable, not an
arbitrary file from the working directory.

## CLI example

After building the reader:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --example codex_read -- "$(command -v codex)"
```

The example does not start the GUI. It prints only a normalized quota DTO or a
static error category; safe numeric RPC codes are allowed. It does not write quota
snapshots to disk. Avoid redirecting its output into public documents/issues unnecessarily.

The reader performs one bounded reading, not retries or polling. The RPC deadline
is 15 seconds, with a separate cleanup allowance of up to two seconds. Ctrl-C and
SIGTERM cancel the example's reading future; process-group ownership also covers
npm launchers and nested Rust CLIs. Graceful cleanup after owner SIGKILL or OS
shutdown cannot be guaranteed.

## Bucket selection and accurate normalization

- A present map permits only `rateLimitsByLimitId.codex`, without a legacy fallback;
  the server's legacy value may contain the first **different** bucket.
- An absent/null map permits legacy data only without a foreign limitId.
- Null windows mean no window, not zero usage; no windows means unavailable.
- Labels follow actual duration. Unknown duration in the P0 DTO uses sentinel `0`
  and an honest label, not a measured zero-minute window.
- Invalid percentages/durations/epochs produce errors, without clamping or fabricated quota.
- `ordinaryUsageAllowed=false` blocks a current counter; null does not become true.
- No login, API-key authorization, or no snapshots can produce RPC errors instead
  of empty quota. Do not extract raw error text; use one safe category and advise
  checking ChatGPT login in the CLI.

## Implementation verification

- **25 Codex tests + 13 P0 tests = 38 Rust tests** passed with `--locked --all-targets`.
  Coverage includes schema/normalization, pipe/frame limits, timeout, EOF, RPC/privacy,
  process-group cleanup, and future cancellation. Synthetic subprocess tests require
  `/usr/bin/python3`; the runtime reader does not depend on Python.
- Fixed a discovered bug: `ordinaryUsageAllowed` belongs to the **top-level response**,
  not a bucket. Authentic fixtures produced five RED failures; top-level false,
  null/missing, conflicting nested permission, and invalid permission tests passed
  after the fix. Focused independent re-review confirmed it, without proving all
  bugs absent.
- The final **Rust example** successfully obtained a real normalized quota snapshot:
  current status, a weekly window, preserved top-level permission, and reset in
  milliseconds. No raw account fields or real response fixture were retained.
- CLI signal smoke with synthetic child/helper: **SIGINT and SIGTERM**, exit 130,
  empty stdout, a static cancellation message only, and both child PIDs terminated.
- cargo fmt/check, clippy `-D warnings`, and locked dependency resolution passed.
  The updated `Cargo.lock` includes Tokio process/macros/signal dependencies.
- P0 frontend regressions: **34 TS + 6 browser tests**, strict TypeScript, and Biome
  passed. **78 automated test cases total**, plus a manual live reading and two
  CLI signal smokes. This step does not enable a live UI source.

## Official pinned sources

Version tag `rust-v0.162.0`, commit `c1382380de69521303b416720a52f42d51af6248`:

- [Account handler, map selection and auth errors](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/app-server/src/request_processors/account_processor.rs#L1190-L1296)
- [DTO and nullable permission](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/app-server-protocol/src/protocol/v2/account.rs)
- [Core RateLimitWindow, explicit reset seconds](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/protocol/src/protocol.rs)
- [Feature registry](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/features/src/lib.rs)
- [Plugin startup guard](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/core-plugins/src/manager.rs#L2814-L2922)
- [MCP processor construction](https://github.com/openai/codex/blob/c1382380de69521303b416720a52f42d51af6248/codex-rs/app-server/src/request_processors/mcp_processor.rs)

App-server is experimental: a verified version does not imply a permanent contract or SLA.
