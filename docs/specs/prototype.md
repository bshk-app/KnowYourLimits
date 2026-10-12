# KnowYourLimits — P0 prototype

## Goal and scope

A runnable macOS-first Tauri 2 prototype using Rust and vanilla TypeScript, based on
[the research](../../claudedocs/research_limits_tauri2_2026-10-10.md). This validates
UX rather than providing a working autonomous monitor. All percentages are fictional
and explicitly labeled Demo. Real CLIs, OAuth, credentials, inference, notifications,
and automatic startup are not connected. An autonomous Claude adapter remains a
separate future technical spike.

## Behavior

- One template icon in the menu bar, with a system title containing `C`/`X` beside it.
- Start with the panel closed and no Dock icon. Left click opens the panel under the
  icon; clicking again leaves it open. Loss of focus, Escape, and closing hide it.
  The right-click menu offers open, refresh demo, and quit.
  **REVISED after review:** P0 uses idempotent opening instead of the initial toggle
  to avoid depending on the order of macOS blur/mouse-up events. This deliberately
  narrows prototype UX; toggling with event coordination is deferred.
- Show both cards together, with separate short/weekly windows, used quota, and reset countdown.
- The menu counter is the rounded minimum remaining quota among available fresh
  primary windows. Only current samples no older than five minutes qualify; an
  expired window does not become zero. Stale/error/unavailable states or no eligible
  windows produce `—`, not `0%`. Historical card values remain visible but are
  explicitly marked as outdated.
- Settings select the first provider and a second provider or None, without duplicates;
  an array of one or two IDs preserves order. Thresholds range from 1–100%, are
  unique, and allow up to five per provider; defaults are 80/95/100. In P0 they
  change visual warnings only and do not send notifications.
- Native settings use app configuration JSON with atomic replacement. Preview saves
  only settings in localStorage, not the demo scenario. Read/write errors are visible.
- Demo scenarios: normal usage, nearly exhausted, stale, error, and no data.
  Refresh demo creates a new fictional snapshot instead of querying an account.
- Preview pages support remote demonstrations, not an additional web product.

## Modules and contract

- `src/model.ts`: DTOs, freshness, formatting, counters, and settings validation.
- `src/fixtures/demo.json`: shared fictional percentages and reset offsets for Rust/TS.
- `src/bridge.ts`: narrow native commands with a local browser demo fallback.
- `src/main.ts`, `src/style.css`: compact UI without React, a router, or remote assets.
- `src-tauri/src/model.rs`: native DTOs, demo snapshot, settings validation, and counter.
- `src-tauri/src/lib.rs`: state, atomic settings storage, menu/tray/window, and fixed IPC.

Camel-case DTOs:

```ts
type ProviderId = 'claude' | 'codex';
type Scenario = 'normal' | 'critical' | 'stale' | 'error' | 'unavailable';
type Settings = {
  barProviders: ProviderId[];
  thresholds: Record<ProviderId, number[]>;
};
type QuotaWindow = {
  id: string; label: string; usedPercent: number;
  resetsAt: number | null; // Unix milliseconds
  durationMins: number; primary: boolean;
};
type ProviderSnapshot = {
  id: ProviderId; label: string;
  status: 'current' | 'stale' | 'error' | 'unavailable';
  fetchedAt: number | null; // Unix milliseconds
  windows: QuotaWindow[];
};
type AppState = {
  settings: Settings; providers: ProviderSnapshot[];
  scenario: Scenario; notice: string | null;
};
```

IPC: `get_state()`, `save_settings({settings})`, `set_demo_scenario({scenario})`,
`refresh_demo()`, `hide_panel()`. State changes and reads return `AppState`;
hiding returns void. No frontend-controlled paths or shell/HTTP/filesystem plugins.

## Verification criteria

1. TS/Rust tests: minimum remaining quota, order, a single provider, missing/stale/error
   data, expired resets, invalid percentages, settings validation, and camel-case parity.
2. Strict TypeScript, Biome, Vite build, cargo fmt/test/clippy, and native build.
3. Preview: scenario switching, editing/saving settings, restoration after reload,
   light/dark themes, and absence of external requests or JavaScript errors.
4. Native smoke: title, hidden panel, opening from the tray, settings persistence,
   and packaged `.app`. Clearly identify unverified behavior in README.
