# Verified native tutorial storyboard

Status: **complete**; source ranges verified against the parent capture log, final checks passed, and encoded output frames inspected. Total duration: **45.966667 seconds**, 1920×1080, 30 fps, silent.

## Frame 1 — A quick introduction

- src: `compositions/intro.html`
- Output: 0.000000–6.000000 seconds.
- Title: “Know your limits.”
- Instruction: “Claude and Codex quota, one click away.”
- Supporting caption: “A native macOS walkthrough”
- Real still hold: `assets/panel.png`; no reconstructed UI.
- Motion rules: `spring-pop-entrance` (calm chapter badge settle), `stat-bars-and-fills` (chapter progress only). Native UI remains stationary and is visible from the first source frame.

## Frame 2 — Read your limits

- src: `compositions/limits.html`
- Output: 6.000000–8.000000 seconds.
- Title: “Remaining at a glance.”
- Instruction: “Open the panel for used quota and reset times when available.”
- Supporting caption: “Remaining in the menu bar · Used in the cards”
- Real source: `assets/panel.mp4`; scene-local output start=0, source in=0, duration=2, playback=1x.
- Motion rules: `spring-pop-entrance` (calm chapter badge settle), `stat-bars-and-fills` (chapter progress only). Native UI remains stationary and is visible from the first source frame.

## Frame 3 — Update a provider

- src: `compositions/refresh.html`
- Output: 8.000000–12.966667 seconds.
- Title: “Update independently.”
- Instruction: “Select Update on the provider you want to refresh.”
- Supporting caption: “Old values remain visible while the reading finishes”
- Real source: `assets/panel.mp4`; scene-local output start=0, source in=2, duration=4.966667, playback=1x.
- Motion rules: `spring-pop-entrance` (calm chapter badge settle), `stat-bars-and-fills` (chapter progress only). Native UI remains stationary and is visible from the first source frame.

## Frame 4 — Menu bar appearance

- src: `compositions/layouts.html`
- Output: 12.966667–24.966667 seconds.
- Title: “Choose your layout.”
- Instruction: “Preview One row, Two rows, and Rings.”
- Supporting caption: “This recording returns to Two rows.”
- Real source: `assets/settings-layout.mp4`; scene-local output start=0, source in=0, duration=12, playback=1x.
- Motion rules: `spring-pop-entrance` (calm chapter badge settle), `stat-bars-and-fills` (chapter progress only). Native UI remains stationary and is visible from the first source frame.

## Frame 5 — Claude and Codex preferences

- src: `compositions/rhythm.html`
- Output: 24.966667–39.966667 seconds.
- Title: “Separate settings.”
- Instruction: “Choose appearance and update intervals independently.”
- Supporting caption: “Size changes here. Auto-update and alerts stay Off.”
- Real source: `assets/settings-claude.mp4`; scene-local output start=0, source in=0, duration=7.5, playback=1x.
- Real source: `assets/settings-codex.mp4`; scene-local output start=7.5, source in=0, duration=7.5, playback=1x.
- Motion rules: `spring-pop-entrance` (calm chapter badge settle), `stat-bars-and-fills` (chapter progress only). Native UI remains stationary and is visible from the first source frame.

## Frame 6 — Back to work

- src: `compositions/outro.html`
- Output: 39.966667–45.966667 seconds.
- Title: “Stay in your flow.”
- Instruction: “Your last readings stay close. Update when you need a new measurement.”
- Supporting caption: “KnowYourLimits · Claude + Codex”
- Real still hold: `assets/panel.png`; no reconstructed UI.
- Motion rules: `spring-pop-entrance` (calm chapter badge settle), `stat-bars-and-fills` (chapter progress only). Native UI remains stationary and is visible from the first source frame.

## Capture truth and timing

Panel source is 209 frames at 30 fps, 720×928, 6.966667 seconds. The real Updating state is at source 2.266667–2.833333 seconds, hence global 8.266667–8.833333 seconds. Verify global **8.5 seconds**. Old values remain during Updating; the new Codex reading follows naturally. No updating state or quota value is simulated.

The layout recording shows One row, Two rows, Rings, then restores Two rows. Both provider recordings change size Medium → Large → Medium. Auto-update and alerts stay Off; the film does not claim to enable or change their intervals.

All scene timing is local to matching sub-composition IDs; no timed ordinary wrapper surrounds timed video. UI Fidelity applies: no pixel grading, blur, or UI overlays. Offline typography uses the bundled Inter variable font; GSAP 3.14.2 is local. CLI is pinned to HyperFrames 0.8.149.
