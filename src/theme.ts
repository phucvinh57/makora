import { useSyncExternalStore } from "react";

export type Theme = "light" | "dark";

const STORAGE_KEY = "makora-theme";

function getInitialTheme(): Theme {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark") return stored;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

let theme: Theme = getInitialTheme();
const listeners = new Set<() => void>();

document.documentElement.classList.toggle("dark", theme === "dark");

export function toggleTheme() {
  theme = theme === "dark" ? "light" : "dark";
  document.documentElement.classList.toggle("dark", theme === "dark");
  localStorage.setItem(STORAGE_KEY, theme);
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getSnapshot() {
  return theme;
}

export function useTheme(): Theme {
  return useSyncExternalStore(subscribe, getSnapshot);
}
