# 0.2.3 — applying automatic refresh intervals

The user selected 15 minutes, but both providers displayed Manual refresh and
snapshots older than four hours. App-owned settings.json confirmed refreshMinutes=0
for both. Selecting an interval changed only the draft until a separate Save
submission below the other settings.

The regression initially failed: selecting 15 minutes without submitting left
schedule-status at Manual refresh. Selection changes now save and apply the
interval immediately. Cards show the active setting, which survives settings
closure and restart. Write failures restore the active selection and show a
message beside the field and in the panel.

The new native command set_refresh_minutes atomically updates only the selected
provider's timer, preserving other applied settings. Other unsaved UI fields are
not applied, and timer changes do not request notification permission. Both timer
selectors are disabled while saving. save_settings and partial updates share a
locked commit/persistence path. Timer changes do not reset notification deduplication.

Backend tests cover 15 minutes before/after deadlines, startup after restoration,
independent preferences, invalid values, and rollback on write errors. UI tests
cover selection without Save/close/reopen, unsaved notification/size drafts,
save-error rollback, and absence of raw diagnostics.

## Verification and installation

183 cases: 116 Rust library + 2 examples + 47 TS + 18 Playwright. fmt, Clippy with
warnings denied, typecheck, Biome, and production build passed. The 15-minute
selection regression went RED → GREEN.

Zamok signed and notarized 0.2.3. codesign/deep/strict, Gatekeeper (Notarized
Developer ID), and stapler passed for the DMG/app. DMG: 3 498 977 bytes,
SHA-256 `69acd3ffec367501a4e95629a0153cc1d6787d224cced0148d11bbfc89759090`.
The consumer download hash matches. Artifact:
`artifacts/macos/0.2.3/KnowYourLimits-0.2.3.dmg`.

Installed at /Applications/KnowYourLimits.app. Previous 0.2.2 exited through
owned SIGTERM/awaited shutdown; backup:
`~/Library/Caches/KnowYourLimits/app-backups/KnowYourLimits-before-0.2.3-1791737291084.app`.
Original settings.json was saved in the same directory with timestamp 1791737291084.

Native 0.2.3 launched with a closed panel and a temporary one-minute interval,
PID 85042. No live smoke or reading-button clicks were performed. The background
ticker itself updated both sources from morning fetchedAt values
Claude 1791720096820 / Codex 1791720162026 to
Claude 1791737358213 / Codex 1791737362877, both current with 67%/97% remaining.
The next automatic cycle updated timestamps to
Claude 1791737423128 / Codex 1791737427985, both current. These were two real
background cycles with a one-minute interval. Default tests do not run real CLIs.

After smoke verification, the app exited, 15 minutes was saved atomically for both
providers, and the app relaunched. Other settings stayed unchanged: the checksum
with only refreshMinutes zeroed matched before/after:
`923da1a73e7e0a5884975b393ff50aa4671020a7421eaa821447fadf96dee5c5`.
A deterministic test verified the 15-minute deadline: no reading at 899 seconds,
a reading at 901 seconds. The smoke did not wait for a real 15-minute cycle.
CUA AX capture timed out. The process confirmed launch; changing real on-disk
snapshots confirmed background readings. Physical UI clicks are not claimed.
