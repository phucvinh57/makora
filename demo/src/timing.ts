export const FPS = 30;
export const WIDTH = 1920;
export const HEIGHT = 1080;

export const scenes = [
  { name: "intro", start: 0, duration: 150 },
  { name: "tracking", start: 150, duration: 180 },
  { name: "history", start: 330, duration: 210 },
  { name: "categories", start: 540, duration: 210 },
  { name: "statistics", start: 750, duration: 210 },
  { name: "privacy", start: 960, duration: 150 },
  { name: "outro", start: 1110, duration: 210 },
] as const;

export const DURATION = scenes.reduce(
  (total, scene) => total + scene.duration,
  0,
);
