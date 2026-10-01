/** `localStorage` can throw (private windows, blocked site data), so every access goes through here. */
export function readStorage(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeStorage(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Persisting a choice is a convenience only.
  }
}
