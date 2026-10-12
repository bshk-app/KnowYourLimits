# P1c: standalone Claude source verification

**0.2.1 update:** an empty machine report was found to also hide HTTP 429 with a
seeded fallback. The earlier traffic-flag hypothesis did not cover this cause.
The reader now classifies HTTP 429 and preserves the server retry deadline;
it does not restore missing percentages from text. See [the current fix](release-0.2.1.md).
The remainder is the historical spike report.

Date: October 10, 2026. **The standalone reader is implemented and tested;
suitability for native LIVE is not yet confirmed.**

CLI 2.1.296 has a local, noninteractive `/usage` command with a structured
`usage_report`, so this spike did not need a PTY. The first probe obtained global
session/weekly windows with percentages and RFC3339 resets. The final CLI result
reported zero turns, API duration, cost, and tokens. This is a CLI observation and
local dispatch check, not an independent audit of all network requests.

## Implemented

- `src-tauri/src/claude.rs`: a separate Unix reader, not connected to GUI/IPC.
  Returns only the existing `ProviderSnapshot` or a static error category.
- Trusted absolute executable → canonical target → version → one `/usage`.
  Only **2.1.296** is supported, without fallback to another CLI, PTY, or inference.
- Per-process `--safe-mode`, empty tools/MCP, no Chrome or session persistence,
  zero-turn limit, hooks/connectors/auto-continue restrictions, and disabled
  nonessential traffic/telemetry/error reporting/updates/background tasks/memory.
  `--bare` excludes subscription OAuth. `--disable-slash-commands` also removes
  local built-in commands in the pinned implementation, so neither flag is suitable.
- Bounds cover stdout/stderr, JSON frames/count, and a shared 15-second deadline
  for both launches. On every outcome, the owned group is killed **before reaping**,
  then the reader awaits confirmed exit and retains ownership across temporary OS
  wait errors. Cancellation must be awaited, not aborted; SIGKILL/power loss has
  no such guarantee.
- The parser accepts only local usage results with confirmed zero model counters.
  Account identity, session details, activity, costs, extra usage, scoped model
  windows, and raw diagnostics do not enter DTOs.
- Global `session`/`weekly_all` normalize separately; `is_active=false` does not
  exclude a valid window. Invalid/duplicate windows produce errors; missing windows
  are unavailable without percentages/timestamps. RFC3339 parsing uses Chrono
  0.4.45, already in the lockfile and now an explicit Unix dependency.
- `claude_read` requires `--live <trusted-absolute-path>` and completes managed
  cancellation before exiting on SIGINT/SIGTERM.

## Source limitations

**Successful `/usage` completion does not prove current quota.** Later real runs
showed percentages in the text while `usage_report.rate_limits.limits` contained
no windows. The Rust reader correctly returned:

```json
{"id":"claude","label":"Claude","status":"unavailable","fetchedAt":null,"windows":[]}
```

The reason `limits` was absent in that response was not conclusively established.
A diagnostic launch showed one CLI-owned quota GET attempt and zero model counters;
it did not prove the source or freshness of text percentages. Text is not a
fallback source. No new authorization, custom OAuth client, or changes to the
user's CLI/configuration were performed to address it.

Two important paths were confirmed in the pinned embedded implementation:

1. `R`/`bXn` can return saved utilization younger than **60 seconds** without querying
   the endpoint; the machine report omits observation timestamps. The reader's
   `fetchedAt` is therefore a conservative lower bound: start minus 60 seconds,
   not an exact measurement time. With the existing five-minute TTL, this leaves
   at most approximately four minutes of current status after reading.
2. `Ghn` removes generic `limits` from the `seeded` fallback but retains legacy
   fields for text display. `v` generates the machine twin only from `limits` and
   `extra_usage`. Missing generic windows cannot be restored from legacy text
   without freshness evidence. The first positive probe could also have used the
   permitted short cache; it does not prove an autonomous cold reading by itself.

Consequently, **Claude was not enabled in native LIVE and polling was not added**.
A confirmed cold-reading path with observation time/source status, or a corrected
official machine report, is needed. PTY text alone does not resolve this freshness
gap. Manual Codex integration continues under its previous contract.

## Startup and trust boundaries

Official `--safe-mode` preserves CLI-owned authorization and disables user
customizations, but **admin-managed settings/policy hooks still apply**.
`system/init` may list installed plugin/skill metadata even in safe mode;
its presence or absence does not prove execution. Tools and MCP were empty in the
probes. Application code does not open credentials/configuration/cookies; the
official CLI can read them, refresh authorization/cache, and analyze activity.
This launch has side effects and does not provide a public API SLA.

Verified local native binary: 2.1.296,
SHA-256 `c9b5341637becbd423ddffc5b254afb645682a3868cb708bbc6cc0e7bb419937`.
The hash identifies the tested sample, not the installation's provenance.
Embedded dispatch: local `/usage` → `shouldQuery=false`; the structured report
is a separate sibling field in a synthetic assistant message. Binary code was not
copied into the project, and no real raw RPC/debug/identity/credential fixtures
were saved.

## Verification

- **20 Claude tests** cover schema/privacy/freshness, local `ui_invalidate`,
  compatibility gates, exact isolated launch, executable symlink retargeting,
  budgets, EOF, timeout, cancellation before startup and in both subprocess
  stages, and confirmed child/helper termination. All fixtures are synthetic.
- Normalization started with four RED failures, then GREEN. The first Rust live
  run found `system/ui_invalidate` before init; a targeted regression and handling
  within stream budgets were added. The next run correctly returned unavailable.
- Full locked suite: **77 Rust library + 2 example + 42 TS + 13 Playwright = 134**.
  TypeScript, Biome, production frontend build, rustfmt, and all-target Clippy passed.
- Default tests do not invoke the real CLI. The live smoke was explicit and its
  result is recorded above. Browser native tests remain synthetic IPC mocks and
  do not replace physical native QA.
- The built `claude_read` was checked with synthetic child/helper processes and
  real SIGINT/SIGTERM. Both runs awaited cancellation, exited 130 with empty
  stdout and a static message, and terminated both processes. This validates the
  standalone signal path, not the native GUI or network source.

## Run the standalone example

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --example claude_read -- --live "$HOME/.local/bin/claude"
```

Use the actual trusted absolute path to your installation. The reader rejects
other versions. The user signs in through the official CLI; the example does not
start login or save snapshots. `--help` does not launch the CLI.

## Primary sources

- [CLI reference: safe mode, bare, settings, tools, MCP, persistence](https://code.claude.com/docs/en/cli-reference)
- [Built-in `/usage`](https://code.claude.com/docs/en/commands#built-in-commands)
- [Hooks: per-process restrictions and managed hooks](https://code.claude.com/docs/en/hooks#disable-or-remove-hooks)
- [Environment variables: nonessential traffic, telemetry, and updates](https://code.claude.com/docs/en/env-vars)
- [Chrono RFC3339 parsing](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html#method.parse_from_rfc3339)

The machine twin/cache/dispatch details above were verified against the local
pinned executable, not promised as stable undocumented fields.
See [the implementation contract](specs/claude-spike.md).
