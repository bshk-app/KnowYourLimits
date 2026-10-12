# Completed local media production

Status: **complete**. The native Cap tutorial, short teaser, and two screenshot
frames were rendered and verified locally with **HyperFrames 0.8.149**.
No cloud processing, voice, music, public hosting, or publication was used.

## Deliverables

| File | Dimensions | Timing | Size |
| --- | --- | --- | --- |
| `renders/tutorial.mp4` | 1920×1080 | 30 fps, 1379 frames, 45.966667 s | 3,361,396 bytes |
| `renders/tutorial-preview.gif` | 960×540 | 8 fps, 56 frames, 7.01 s | 578,027 bytes |
| `renders/overview.png` | 1600×960 | Static native panel cover | 123,479 bytes |
| `renders/settings.png` | 1600×960 | Static native settings cover | 139,954 bytes |

The MP4 is H.264/yuv420p with no audio stream. The GIF has a fully opaque warm
paper background: inspected corner RGBA **245, 243, 236, 255**. Its duration
reflects GIF centisecond rounding of the 6.966667-second source window.
Hashes and FFprobe metadata are in `../deliverables-manifest.json`.

## Verified source schedule

| Chapter | Output range, seconds | Actual source |
| --- | --- | --- |
| Introduction | 0–6 | `assets/panel.png`, honest still hold |
| Read limits | 6–8 | `assets/panel.mp4`, source 0–2 |
| Independent update | 8–12.966667 | `assets/panel.mp4`, source 2–6.966667 |
| Menu bar layouts | 12.966667–24.966667 | `assets/settings-layout.mp4`, source 0–12 |
| Claude preferences | 24.966667–32.466667 | `assets/settings-claude.mp4`, source 0–7.5 |
| Codex preferences | 32.466667–39.966667 | `assets/settings-codex.mp4`, source 0–7.5 |
| Closing hold | 39.966667–45.966667 | `assets/panel.png`, honest still hold |

All video plays **1×**. Every range was checked against its real EOF. The actual
Updating state is preserved at global **8.266667–8.833333 s** and inspected at
**8.5 s**. Old Codex 81%/19% remains visible during Updating, followed by the real
80%/20% result. Native footage is visible immediately; only instructional text and
chapter progress animate.

The layout capture shows One row, Two rows, Rings, and restored Two rows. Provider
captures show Medium → Large → Medium size changes. Auto-update and alerts remain
Off; captions do not claim they were enabled or changed. No UI or quota data was
reconstructed or generated. Native captures receive no grading or overlays.

## Final verification

- Main full check: no lint, runtime, layout, or contrast findings; 62 layout/transition
  samples and 33/33 contrast checks passed.
- Teaser full check: no findings; 20/20 contrast checks passed.
- Each screenshot composition: no findings; 25/25 contrast checks passed.
- GSAP animation map: 18/18 tweens reviewed. Long progress-fill tweens are deliberate
  chapter progress; native captures remain still.
- Main chapter midpoints, actual Updating, all three layouts, both provider views,
  and final 45.9-second output frame were visually inspected.
- GIF first, Updating midpoint, and final output frames were visually inspected.
- The PNGs were inspected again after local-font packaging; native labels and
  controls remain readable, with no private account information.
- Final MP4/GIF codec, dimensions, frame counts, duration, size, and absence of
  MP4 audio were verified with FFprobe.

Diagnostic evidence stays in private production: `check-final.json`, each
subproject's `check.json`, `animation-map/`, and `renders/verification/`.
The main encoder used **screenshot capture with hardware GPU**: 49.3 s wall time,
compile 0.7 s, extraction 0.1 s, setup 0.3 s, capture 43.1 s, encoding 4.9 s.
The pinned compiler warned about sparse keyframes in the original 2 fps settings
clips. PNG frame extraction and the inspected encoded output confirm the actual
One row/Two rows/Rings and size changes; sources were not altered to hide warnings.

## Portable source and reproduction

This is plain HyperFrames HTML. No assembler, private temporary file, or build
script is required. `composition-plan.json` and `STORYBOARD.md` record final
ranges; the HTML already contains them. `source-manifest.json` contains only
relative asset paths, byte sizes, and hashes, with no machine, account, or token
data. Ship the app-only assets, local GSAP, local Inter WOFF2, and its OFL license.

Preserve the relative asset links when copying the source:

- `teaser/assets` → `../assets`
- `covers/overview/assets` → `../../assets`
- `covers/settings/assets` → `../../assets`

These links stay within the portable project. Equivalent local asset copies also
work. Exclude raw desktop recordings, control logs, runtime state, thumbnails,
QA caches, and temporary files from the public source set.

From the project root:

```sh
HYPERFRAMES_SKIP_SKILLS=1 HYPERFRAMES_NO_TELEMETRY=1 npx --yes hyperframes@0.8.149 check --json --snapshots
HYPERFRAMES_SKIP_SKILLS=1 HYPERFRAMES_NO_TELEMETRY=1 npx --yes hyperframes@0.8.149 render --fps 30 --quality delivery --video-frame-format png --strict-all --workers 2 --output renders/tutorial.mp4
```

For the short GIF, run from `teaser/`:

```sh
HYPERFRAMES_SKIP_SKILLS=1 HYPERFRAMES_NO_TELEMETRY=1 npx --yes hyperframes@0.8.149 check --json --snapshots
HYPERFRAMES_SKIP_SKILLS=1 HYPERFRAMES_NO_TELEMETRY=1 npx --yes hyperframes@0.8.149 render --fps 8 --quality delivery --video-frame-format png --strict-all --workers 1 --format gif --gif-loop 0 --output ../renders/tutorial-preview.gif
```

For either static cover, run `snapshot --at 0.5 --no-end --describe false` in its
`covers/<name>/` directory after its full check, then use the midpoint PNG.
Delivered media lives one directory above this source project. The commands
above create fresh outputs in the project's `renders/` directory.
