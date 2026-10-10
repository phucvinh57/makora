export const FPS = 30;
export const WIDTH = 1920;
export const HEIGHT = 1080;

export const scenes = [
  { name: "intro", start: 0, duration: 150 },
  { name: "tracking", start: 150, duration: 210 },
  { name: "history", start: 360, duration: 210 },
  { name: "statistics", start: 570, duration: 180 },
  { name: "categories", start: 750, duration: 180 },
  { name: "adaptation", start: 930, duration: 180 },
  { name: "outro", start: 1110, duration: 210 },
] as const;

export const DURATION = scenes.reduce(
  (total, scene) => total + scene.duration,
  0,
);
