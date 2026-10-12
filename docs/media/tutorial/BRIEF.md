---
workflow: general-video
flow: automation
storyboard: no
language: en
canvas: 1920x1080
target_duration_seconds: 46
---

# KnowYourLimits walkthrough

Create a short, captioned tutorial for the English GitHub README using a real native recording made on a native Mac through Cap. The app is version 0.2.6, installed from a verified Developer ID signed, notarized, stapled DMG built from b6c0ba1d3bad25b528992dd57b0b7b563fe91c1a.

## Viewer arc

Read the panel, understand remaining versus used quota and reset times when available, watch a real independent Codex update, preview the three macOS menu bar layouts, inspect separate Claude/Codex appearance and update settings, and finish on the genuine panel.

## Production decisions

- Use native UI footage and genuine provider state. Never create demonstration percentages, account labels, notifications, or fabricated quota updates.
- Show English tutorial captions, a clear chapter number, and a short instruction next to a readable capture.
- Use a warm neutral background, dark teal typography and mint accents derived from the app icon. Preserve native screenshot pixels; decorative framing must not disguise missing features.
- Aim for 45–75 seconds, guided by the actual footage and readable holds. Trim idle gaps with HyperFrames `data-media-start` source ranges and `data-start` output placement.
- Keep camera, microphone and system audio off. Use captions without voiceover or paid processing.
- Export a 1920×1080 H.264 MP4, a short GIF preview, and crisp PNG overview/settings frames.
- Check HyperFrames lint/runtime/layout/contrast, inspect chapter midpoint snapshots and final frames, and verify exported dimensions, duration and file size with FFprobe.

## Source status

All real native Cap assets are present in `assets/`. Sources and SHA-256 hashes are recorded in `source-manifest.json`; exact scene ranges and captions are in `composition-plan.json` and `STORYBOARD.md`. Original Cap export metadata remains in private production. Portable source evidence is recorded here without desktop footage or control logs.

- Panel:720×928,H.264,30fps,209frames,6.966667seconds. Actual old→Updating→new Codex reading is preserved at1x.
- Layouts:840×658,H.264,2fps,12seconds. One row→Two rows→Rings→restoredTwo rows are genuine native interactions.
- Provider settings:840×1060,H.264,2fps,7.5seconds each. Actual size changes Medium→Large→Medium; auto-update and alerts remain Off.
- Panel PNG:720×928; latest real Claude53%remaining,Codex80%remaining. No caption invents fixed quota or a reset time.
- Settings PNG:840×658; existing real capture before the final provider reading.

Only app-only crops are staged here. Native capture and cropping are owned by the parent thread; the compositor does not record unrelated windows or create substitute UI.

## Deliverables

- `docs/media/overview.png`: native provider panel, styled in a clear README frame.
- `docs/media/settings.png`: native settings view, with actual choices visible.
- `docs/media/tutorial.mp4`: Cap footage assembled, captioned and rendered through HyperFrames.
- `docs/media/tutorial-preview.gif`: short teaser that links to the full tutorial.
- A reproducible HyperFrames project with the actual media and final source ranges, moved into `docs/media/tutorial/` after validation.

## Completion

The 45.966667-second silent MP4, 7.01-second opaque GIF teaser, and two
1600×960 PNG covers are complete and visually verified. Final codecs, timing,
sizes, check results, and reproduction commands are recorded in `PRODUCTION.md`.
All source ranges are actual Cap exports; no pending input or temporary assembler
is required. The parent owns README integration and publication decisions.
