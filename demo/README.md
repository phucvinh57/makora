# Makora demo

A 44-second, 1920 × 1080, 30 fps Remotion video for introducing Makora.
It is designed to work without sound: headings and captions carry the story.
Screenshots show the earlier React interface with fictional activity data.

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

## Screenshots

`public/screens/` holds screenshots of the earlier React interface. The
capture script that produced them, which also copied the app's Geist font to
`public/fonts/geist.woff2`, was removed with that UI. Replace the PNGs and font
by hand when the screens change.

## Storyboard

| Time    | Scene                                                           |
| ------- | --------------------------------------------------------------- |
| 0–5 s   | Introduce local-only tracking, built-in AI, and personalization |
| 5–12 s  | Illustrate focus changes and automatic activity blocks          |
| 12–19 s | Search activity history by app and website                      |
| 19–25 s | Show weekly statistics and daily trends                         |
| 25–31 s | Introduce on-device AI category suggestions                     |
| 31–37 s | Show how categories and past choices guide suggestions          |
| 37–44 s | Reaffirm the three pillars and close with the project URL       |

Edit `src/MakoraDemo.tsx` for the visuals and copy, and `src/timing.ts` for
timing and dimensions. Animations use
Remotion frames rather than wall-clock timers so preview and export match.
