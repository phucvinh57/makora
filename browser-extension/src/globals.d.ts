// Firefox and Safari expose the same WebExtension surface as Chromium's
// `chrome`, so `@types/chrome` describes both. `browser` is only declared here
// so `globalThis.browser ?? globalThis.chrome` typechecks under Chromium, where
// the global genuinely is absent.
declare var browser: typeof chrome | undefined;

interface Navigator {
  // Brave-only; feature-detected in detectBrowser().
  brave?: { isBrave(): Promise<boolean> };
}
