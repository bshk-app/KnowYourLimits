# 0.1.2: Claude integration and demo removal

Problem: version 0.1.1 could connect only Codex. Claude always showed Not connected,
and the panel offered a return to fictional data.

Both cards now offer their own connection and refresh actions. Mode, Scenario, and
demo snapshot generators are absent from the model; bundled `src/fixtures/demo.json`
was removed. Demo behavior was removed from IPC, the tray menu, browser UI, CSS,
and initial HTML. Synthetic data is used only in tests.

State tracks connected providers and the specific provider being refreshed.
Claude readings preserve Codex snapshots and vice versa. The coordinator permits
one active process, coalesces repeated identical actions, awaits cleanup of the
previous job, preserves settings changes, and rejects late results after cancellation/exit.

## Claude

Readings use the same official CLI 2.1.296 and structured `/usage` previously used
only in the standalone spike. `claude auth status --json` runs before a reading;
identity and raw diagnostics do not enter DTOs.

The first reading reported `loggedIn:false`, `authMethod:none`; later the same
official auth status reported a signed-in subscription. The old `/usage` check
silently returned an empty snapshot. Missing authorization now produces `signedout`
and a **Sign in to Claude** button.

Sign-in starts only through a separate explicit user action:
`claude auth login --claudeai`. The official CLI opens the browser, performs OAuth,
and saves authorization. The application discards output and accepts only a
static success message, then rechecks auth status and reads quota. Application
code never opens credential files. Sign-in is bounded to 180 seconds and can be
canceled from the panel or menu. Cancellation and exit await owned group termination.

Claude windows are global `session` / `weekly_all`, without token estimates, PTY,
inference, or text percentages when a machine report is absent. The source can
return empty structured limits, shown as unavailable quota. Empty responses still
do not become fictional percentages. Browser sign-in is covered by synthetic
CLI/IPC tests; the agent did not perform real user sign-in in a browser.

### Empty cold-reading fix

A cause of the earlier spike's source gap was found: in local Claude Code 2.1.296,
`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` removes structured `limits` from cold
`/usage`. A differential check of one variable showed:

- Flag enabled: one quota GET, no cache answer, successful result, but no windows.
- Flag absent: one quota GET, no cache answer, global session/weekly windows.
- Both cases had zero turns, API duration, cost, and model tokens.

Only the umbrella flag is removed from the child environment, including possible
inheritance from the parent. Telemetry, error reporting, updates, background tasks,
memory, hooks, MCP, and tools remain individually disabled. Global environment is
unchanged. The regression checks a real subprocess launch and first failed
(`Command`), then passed after the fix. This is an observation of the pinned CLI,
not a claim about other versions.

## Verification

- Initial UI reproduction:
  `bun run test:e2e tests/native.spec.ts -g 'Claude can be connected'`
  first failed because the button was absent, then passed after the fix.
- 84 Rust library + 2 example + 41 TypeScript + 10 Playwright cases.
- Coverage includes the production resolver → managed Claude reader with a
  synthetic CLI, auth failures, official login dispatch, cleanup at every stage,
  linked counters, single-flight execution, errors/expired quota, and late events.
- Real Codex smoke through the production coordinator returned current weekly
  quota. Real Claude was tested with an empty response and then real session/weekly
  windows after the environment fix. Final verification details follow below.
- Default tests do not perform network authorization. Browser tests use mock IPC;
  physical packaged WebView verification is recorded separately after building.

## Build

Version updated to 0.1.2 in frontend, Cargo/lock, and Tauri metadata.
`bun run package:macos` uses the same Zamok workflow, Developer ID
Aleksandr Beshkenadze (Q8H6GWJ658), and `notarize` Keychain profile.
Credentials are not extracted. No product publication to Zamok/GitHub is performed.
Artifact: `artifacts/macos/0.1.2/KnowYourLimits-0.1.2.dmg`.
Final DMG: 3 395 199 bytes, SHA-256
`3b8d6ebf961e27abf6d3517e5de01db793affa2ff70dd1bde9b490c62e3a2e5a`.
Apple notarization and stapling were confirmed. Gatekeeper accepts the DMG and
the app inside the mounted read-only consumer DMG; codesign --deep --strict passes.

The fixed production coordinator with minimal PATH obtained real Claude
session/weekly windows without a prior diagnostic probe. Codex also returned
current weekly quota. The differential CLI probe confirmed a GET without a cache
answer and zero inference counters.

The final version was installed at `/Applications/KnowYourLimits.app`.
The previous version exited correctly through SIGTERM and was backed up at:
`~/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.1.2-1791656658488.app`.
The installed bundle's version 0.1.2 and strict signature were verified; its
process was started. Computer Use returned timeoutReached when capturing the
panel, so physical connection/sign-in clicks and packaged WebView appearance
were not confirmed. Actual ACL, production coordinator, and mock IPC UI checks pass.

Final download server: port 55999, session 96594. Old 0.1.0/0.1.1 links redirect to
final 0.1.2. The intermediate artifact before the cold-reading fix is retained
separately at `artifacts/macos/0.1.2-before-cold-read-fix` and is not distributed.
