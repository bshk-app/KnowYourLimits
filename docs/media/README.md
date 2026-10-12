# Native screenshots and tutorial

These assets show KnowYourLimits **0.2.6** on macOS. The interface and quota
readings were captured from the installed app on October 12, 2026, using already
authorized official Claude Code and Codex CLIs. Quotas are observations at capture
time and will differ from a later reading.

- [Quota panel](overview.png) — both providers and their actual quota windows.
- [Menu bar settings](settings.png) — the native appearance choices.
- [Tutorial](tutorial.mp4) — a 46-second, 1920×1080 H.264 walkthrough at 30 fps.
- [Animated preview](tutorial-preview.gif) — a short teaser linking to the tutorial.

## Capture and editing

Cap Desktop **0.6.0** and its CLI **0.1.0** recorded the native interface.
The final screenshot layouts, captions, and tutorial are produced with
HyperFrames **0.8.149**. Decorative framing sits outside the original UI pixels.
The captures contain no recreated interface, fabricated quotas, synthetic cursor,
camera, microphone, or system audio.

The panel footage includes a genuine Codex refresh: existing values remain visible
during **Updating…**, then the observed weekly usage changes from 19% to 20%.
The appearance sequence previews One row, Two rows, and Rings and returns to
Two rows. Provider settings show Medium → Large → Medium; automatic refresh and
notifications remain off. Temporary appearance selections were restored.

Settings were recorded as individual Cap windows. The always-on-top quota panel
was cropped to its measured bounds from a Cap screen recording; only the app crop
is included in these assets. The original screen recording remains private.

Cap's sparse settings recordings needed a lossless conversion to constant frame
cadence in an isolated working copy before export. The original captured frames
and their hold times were preserved. The panel export used Cap's native decoder.
All source projects passed Cap validation, and exported state changes were checked
against the captured frames.

## Versions

| Component | Version |
| --- | --- |
| KnowYourLimits | 0.2.6 |
| Claude Code | 2.1.296 |
| Codex CLI | 0.162.1 |
| Cap Desktop / CLI | 0.6.0 / 0.1.0 |
| HyperFrames | 0.8.149 |

The macOS screenshots do not demonstrate Windows tray behavior. See the
[user guide](../usage.md) for platform differences and quota freshness semantics.

## Reproduce the render

The [portable HyperFrames project](tutorial/PRODUCTION.md) includes the actual
app-only Cap exports, screenshot inputs, local GSAP and Inter font with its
license. It contains no raw desktop recording or private control logs.

From `docs/media/tutorial/`:

```sh
HYPERFRAMES_SKIP_SKILLS=1 HYPERFRAMES_NO_TELEMETRY=1 npx --yes hyperframes@0.8.149 check
HYPERFRAMES_SKIP_SKILLS=1 HYPERFRAMES_NO_TELEMETRY=1 npx --yes hyperframes@0.8.149 render --fps 30 --quality delivery --video-frame-format png --output renders/tutorial.mp4
```

The final compositions passed runtime, layout, and contrast checks. Encoded
chapter frames, the actual Updating state, all appearance changes, and the final
frame were visually reviewed. The GIF's background is opaque and the MP4 has
no audio stream.
File hashes and video metadata are in the [deliverables manifest](deliverables-manifest.json).
