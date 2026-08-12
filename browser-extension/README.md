# makora tab reporter

A small cross-browser WebExtension companion to the [makora](../README.md)
desktop app. Makora tracks how long you spend in each application by
watching OS-level window focus, which means a browser is normally just one
opaque `chrome` or `firefox` block. This extension reports the active tab's
URL to makora's local ingest server so that block can be broken down
per site instead.

It POSTs `{ browser, url, title }` to `http://127.0.0.1:47632/event`
whenever the active tab changes — on tab switch, on navigation, and on
window focus change. Nothing is sent anywhere except `127.0.0.1`; if the
app isn't running, requests just fail silently.

This extension is entirely optional — without it loaded, browsers are
tracked like any other app.

## Installing

### Chrome (and other Chromium browsers: Edge, Brave, ...)

1. Go to `chrome://extensions` (or the equivalent `edge://extensions`,
   `brave://extensions`, ...).
2. Enable **Developer mode** (top right).
3. Click **Load unpacked** and select this `extension/` directory.
4. Make sure the makora app is running — the extension POSTs to
   `127.0.0.1:47632` and fails silently if nothing's listening there.

The extension has no options page and needs no configuration; it starts
reporting the active tab as soon as it's loaded.

### Safari

Safari doesn't support loading a raw WebExtension folder directly — it has
to be wrapped as a Safari Web Extension app via Xcode's converter tool
first.

1. Install Xcode (from the App Store) and its command line tools.
2. From the repo root, run:
   ```sh
   xcrun safari-web-extension-converter extension/
   ```
   This generates an Xcode project that wraps the extension in a macOS app.
3. Open the generated project in Xcode and build & run it once (⌘R) — this
   registers the extension with Safari and only needs to be done the first
   time (or after any change to `extension/`).
4. In Safari, go to **Settings → Extensions** and enable "Makora's tab
   reporter".
5. Since the extension is unsigned/local, also enable **Settings →
   Advanced → Show features for web developers**, then **Develop → Allow
   Unsigned Extensions**.
6. When prompted, grant the extension permission to run on the sites you
   want tracked (or "Allow on All Websites").
