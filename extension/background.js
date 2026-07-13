// Cross-browser: Firefox exposes `browser` (promise-based), Chrome/Edge/Brave
// expose `chrome` (callback-based but also promise-based for the APIs used
// here). Using whichever is present lets one script run unmodified on both.
const api = globalThis.browser ?? globalThis.chrome;

const ENDPOINT = "http://127.0.0.1:47632/event";

let cachedBrowser = null;

// Brave deliberately mimics Chrome's user-agent string for compatibility, so
// UA sniffing alone can't tell them apart — `navigator.brave` is the only
// reliable signal, and only Brave defines it.
async function detectBrowser() {
  if (cachedBrowser) return cachedBrowser;

  if (navigator.brave && typeof navigator.brave.isBrave === "function") {
    try {
      if (await navigator.brave.isBrave()) {
        return (cachedBrowser = "brave");
      }
    } catch {
      // fall through to UA sniffing
    }
  }

  const ua = navigator.userAgent;
  if (ua.includes("Edg/")) cachedBrowser = "edge";
  else if (ua.includes("Firefox/")) cachedBrowser = "firefox";
  else if (ua.includes("Chrome/")) cachedBrowser = "chrome";
  else cachedBrowser = "other";
  return cachedBrowser;
}

// Strips query string and fragment before the URL ever leaves the browser —
// query params can carry tokens/PII, and the collector only needs origin +
// path to break usage down per site.
function normalizeUrl(rawUrl) {
  try {
    const parsed = new URL(rawUrl);
    return `${parsed.origin}${parsed.pathname}`;
  } catch {
    return null;
  }
}

async function reportActiveTab() {
  const [tab] = await api.tabs.query({ active: true, lastFocusedWindow: true });
  if (!tab || !tab.url || !/^https?:\/\//.test(tab.url)) {
    return; // no active tab, or an internal page (chrome://, about:, extension pages, ...)
  }

  const url = normalizeUrl(tab.url);
  if (!url) return;

  const payload = {
    browser: await detectBrowser(),
    url,
    title: tab.title ?? "",
  };

  try {
    // The browser sets the `Origin` header itself (it's a forbidden header
    // for fetch to override) — the collector trusts that to allowlist
    // extension requests, so nothing needs to be signed here.
    await fetch(ENDPOINT, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
  } catch {
    // makora-collector isn't running, or the port is unreachable — drop
    // silently. No retry queue in v1.
  }
}

api.tabs.onActivated.addListener(reportActiveTab);

api.tabs.onUpdated.addListener((_tabId, changeInfo, tab) => {
  if (tab.active && (changeInfo.url || changeInfo.status === "complete")) {
    reportActiveTab();
  }
});

api.windows.onFocusChanged.addListener((windowId) => {
  if (windowId === api.windows.WINDOW_ID_NONE) return; // focus left the browser entirely
  reportActiveTab();
});
