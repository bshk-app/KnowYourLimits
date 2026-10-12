# Tutorial visual direction

Use the shipped `assets/icons/app.svg` as the brand reference. The tutorial presents real UI; it must not redesign, regenerate, or simulate the interface.

- Canvas: 1920×1080, 30 fps for MP4. Keep text at least 32 px and all essential content inside 80 px safe margins.
- Background: warm paper `#F5F3EC`. Text: dark teal `#1C3434`. Accent: mint `#8DDCB3`. Supporting text: `#49615A`. Use app amber `#EDBD79` sparingly as decoration, with dark text.
- Typography: shipped local sans-serif font or system sans-serif. Left-align chapter number, heading and one concise instruction. Keep headings to two lines at most.
- UI presentation: a stable capture panel on the right, sized to make native labels readable. Keep the actual menu bar crop when teaching menu bar behavior. A clean frame and subtle shadow may sit outside the native capture.
- Motion: short readable chapter entrances, no arbitrary continuous drift. Hold the native UI while viewers read it. Use a small progress marker to explain sequence.
- Source handling: crop only to remove irrelevant desktop content. Preserve native controls, percentages, timestamps and state labels. If capture shows a real error, unknown state or rate-limit pause, captions must describe that state.
- Sound: silent tutorial with chapter captions. No microphone, system audio, music or generated narration.
- End: a readable hold on the genuine app UI and the app name. No black tail.

The output screenshots must be inspected for clipping, illegible text and private information before README integration. Source hashes and actual action ranges belong in `BRIEF.md` after capture.

## Final local typography

Inter is bundled in `assets/fonts/inter-variable.woff2` with its SIL Open Font
License in `assets/fonts/OFL.txt`. Explicit local @font-face declarations prevent
the pinned compiler from resolving system-ui through a Google Fonts cache.
GSAP is local too; composition playback makes no required CDN requests.
