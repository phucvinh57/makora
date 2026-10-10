# Makora demo

A 44-second, 1920 × 1080, 30 fps Remotion video for introducing Makora.
It is designed to work without sound: headings and captions carry the story.
Screenshots show the actual React interface with fictional activity data.

## Preview and render

From the repository root:

```sh
bun run demo:install
bun run demo:dev
bun run demo:check
bun run demo:render
```

Preview opens Remotion Studio on port 3001. Rendering writes the H.264 MP4
to `demo/out/makora-demo.mp4`. The included video can be shared as-is.
Create a closing-card thumbnail with `bun run --cwd demo poster`.

Remotion can download Chrome Headless Shell when rendering. To use an
already-installed browser instead, set `CHROME_PATH`:

```sh
CHROME_PATH=/usr/bin/google-chrome bun run demo:render
```

The demo dependencies and lockfile are separate from the desktop app.

## Refresh screenshots

Install the app's dependencies with `bun install` and the demo dependencies
with `bun run demo:install`. Build and preview the production frontend in
another terminal, so the video shows release behavior:

```sh
bun run build
bun run preview --host 127.0.0.1
```

Then capture the screens:

```sh
CHROME_PATH=/usr/bin/google-chrome bun run demo:capture
```

The capture script uses Playwright with an isolated browser context and a
fictional desktop-command bridge. It does not start the Rust backend or
access the real database. The fixture dates are fixed to October 2–8, 2026;
weekly totals are derived from the sample sessions. Screenshot capture also
copies the app's bundled Geist font for offline rendering.

`DEMO_APP_URL` defaults to `http://127.0.0.1:4173`. Set it if Vite is hosted
elsewhere. `CHROME_PATH` should point to your local Chrome/Chromium executable.
The captured PNGs and font are included, so recapture is only necessary when
the UI or sample data changes.

## Storyboard

| Time | Scene |
| --- | --- |
| 0–5 s | Introduce local-only tracking, built-in AI, and personalization |
| 5–12 s | Illustrate focus changes and automatic activity blocks |
| 12–19 s | Search activity history by app and website |
| 19–25 s | Show weekly statistics and daily trends |
| 25–31 s | Introduce on-device AI category suggestions |
| 31–37 s | Show how categories and past choices guide suggestions |
| 37–44 s | Reaffirm the three pillars and close with the project URL |

Edit `src/MakoraDemo.tsx` for the visuals and copy, `src/timing.ts` for timing
and dimensions, and `scripts/fixtures.ts` for sample activity. Animations use
Remotion frames rather than wall-clock timers so preview and export match.
