const api = globalThis.browser ?? globalThis.chrome;

const HOST_NAME = "dev.phucvinh57.makora";
const MIN_RETRY_MS = 1_000;
const MAX_RETRY_MS = 5 * 60_000;
const HEALTHY_CONNECTION_MS = 30_000;

type BrowserName = "brave" | "edge" | "firefox" | "chrome";

type TabEvent = {
  browser: BrowserName;
  url: string | null;
  title: string;
};

let cachedBrowser: BrowserName | null | undefined;

async function detectBrowser(): Promise<BrowserName | null> {
  if (cachedBrowser !== undefined) return cachedBrowser;

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
  else cachedBrowser = null;
  return cachedBrowser;
}

let port: chrome.runtime.Port | null = null;
let retryMs = MIN_RETRY_MS;
let retryTimer: ReturnType<typeof setTimeout> | undefined;

// The bridge outlives the app, so a dropped port means the host is missing or crashed: back off
// instead of relaunching it in a tight loop.
function connect(): chrome.runtime.Port | null {
  if (port) return port;
  const connectedAt = Date.now();
  try {
    const next = api.runtime.connectNative(HOST_NAME);
    next.onDisconnect.addListener(() => {
      if (port === next) port = null;
      if (Date.now() - connectedAt >= HEALTHY_CONNECTION_MS) {
        retryMs = MIN_RETRY_MS;
      }
      scheduleReconnect();
    });
    port = next;
  } catch {
    scheduleReconnect();
  }
  return port;
}

function scheduleReconnect(): void {
  if (retryTimer !== undefined) return;
  retryTimer = setTimeout(() => {
    retryTimer = undefined;
    if (connect()) reportActiveTab();
  }, retryMs);
  retryMs = Math.min(retryMs * 2, MAX_RETRY_MS);
}

function webUrl(url: string | undefined): string | null {
  return url && /^https?:\/\//.test(url) ? url : null;
}

async function reportActiveTab(): Promise<void> {
  const browser = await detectBrowser();
  if (!browser) return;

  // Only the window the person is looking at: another profile or an unfocused window must not
  // overwrite what this browser is showing.
  const focusedWindow = await api.windows.getLastFocused();
  if (!focusedWindow.focused || focusedWindow.id === undefined) return;
  const [tab] = await api.tabs.query({
    active: true,
    windowId: focusedWindow.id,
  });
  if (!tab) return;

  const payload: TabEvent = {
    browser,
    url: webUrl(tab.url),
    title: tab.title ?? "",
  };

  try {
    connect()?.postMessage(payload);
  } catch {
    port = null;
    scheduleReconnect();
  }
}

api.tabs.onActivated.addListener(() => {
  reportActiveTab();
});

api.tabs.onUpdated.addListener((_tabId, changeInfo, tab) => {
  if (
    tab.active &&
    (changeInfo.url || changeInfo.title || changeInfo.status === "complete")
  ) {
    reportActiveTab();
  }
});

api.windows.onFocusChanged.addListener((windowId) => {
  if (windowId === api.windows.WINDOW_ID_NONE) return;
  reportActiveTab();
});

reportActiveTab();
