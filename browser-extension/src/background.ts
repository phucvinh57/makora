const api = globalThis.browser ?? globalThis.chrome;

const ENDPOINT = "http://127.0.0.1:47632/event";

type BrowserName = "brave" | "edge" | "firefox" | "chrome" | "other";

type TabEvent = {
  browser: BrowserName;
  url: string;
  title: string;
};

let cachedBrowser: BrowserName | null = null;

async function detectBrowser(): Promise<BrowserName> {
  if (cachedBrowser) return cachedBrowser;

  if (navigator.brave && typeof navigator.brave.isBrave === "function") {
    try {
      if (await navigator.brave.isBrave()) {
        cachedBrowser = "brave";
        return cachedBrowser;
      }
    } catch {}
  }

  const ua = navigator.userAgent;
  if (ua.includes("Edg/")) cachedBrowser = "edge";
  else if (ua.includes("Firefox/")) cachedBrowser = "firefox";
  else if (ua.includes("Chrome/")) cachedBrowser = "chrome";
  else cachedBrowser = "other";
  return cachedBrowser;
}

function normalizeUrl(rawUrl: string): string | null {
  try {
    const parsed = new URL(rawUrl);
    return `${parsed.origin}${parsed.pathname}${parsed.search}`;
  } catch {
    return null;
  }
}

async function reportActiveTab(): Promise<void> {
  const [tab] = await api.tabs.query({ active: true, lastFocusedWindow: true });
  if (!tab?.url || !/^https?:\/\//.test(tab.url)) {
    return;
  }

  const url = normalizeUrl(tab.url);
  if (!url) return;

  const payload: TabEvent = {
    browser: await detectBrowser(),
    url,
    title: tab.title ?? "",
  };

  try {
    await fetch(ENDPOINT, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
  } catch {}
}

api.tabs.onActivated.addListener(reportActiveTab);

api.tabs.onUpdated.addListener((_tabId, changeInfo, tab) => {
  if (tab.active && (changeInfo.url || changeInfo.status === "complete")) {
    reportActiveTab();
  }
});

api.windows.onFocusChanged.addListener((windowId) => {
  if (windowId === api.windows.WINDOW_ID_NONE) return;
  reportActiveTab();
});
