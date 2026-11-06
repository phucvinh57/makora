# Makora demo

A 44-second silent Remotion video (1920 × 1080, 30 fps). Captions carry the story. Screenshots show the earlier React UI with fictional data.

## Preview and render

From the repo root:

```sh
bun run demo:install
bun run demo:dev        # Remotion Studio on port 3001
bun run demo:check
bun run demo:render     # writes demo/out/makora-demo.mp4
bun run --cwd demo poster   # closing-card thumbnail
```

To use an installed browser instead of downloading one, set `CHROME_PATH`, e.g. `CHROME_PATH=/usr/bin/google-chrome bun run demo:render`.

## Editing

- Visuals and copy: `src/MakoraDemo.tsx`. Timing and size: `src/timing.ts`.
- Screenshots in `public/screens/` and the font in `public/fonts/geist.woff2` are replaced by hand; their capture script is gone.

| Time    | Scene                                             |
| ------- | ------------------------------------------------- |
| 0–5 s   | Local-only tracking, built-in AI, personalization |
| 5–12 s  | Focus changes become activity blocks              |
| 12–19 s | Search history by app and website                 |
| 19–25 s | Weekly statistics and daily trends                |
| 25–31 s | On-device AI category suggestions                 |
| 31–37 s | Categories and past choices guide suggestions     |
| 37–44 s | The three pillars and the project URL             |
