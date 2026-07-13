# makora tab reporter

A small cross-browser WebExtension that reports the active tab's URL to the
local `makora-collector` process, so browser sessions can be broken down
per site instead of showing up as one opaque `chrome`/`firefox` block.

It POSTs `{ browser, url, title }` to `http://127.0.0.1:47632/event`
whenever the active tab changes — on tab switch, on navigation, and on
window focus change. `url` is already stripped down to origin + path (no
query string or fragment) before it leaves the browser. Non-`http(s)` tabs
(`chrome://`, `about:`, extension pages, ...) are skipped. If the collector
isn't running, the request just fails silently — there's no retry queue.

Nothing is sent anywhere except `127.0.0.1`.

## Loading it

The collector must be running first (`cargo run -p makora-collector`) —
otherwise the requests just fail silently until it is.

**Chrome / Edge / Brave**

1. Go to `chrome://extensions` (or `edge://extensions`, `brave://extensions`).
2. Enable "Developer mode" (top right).
3. Click "Load unpacked" and select this `extension/` folder.

**Firefox**

1. Go to `about:debugging#/runtime/this-firefox`.
2. Click "Load Temporary Add-on…" and select `manifest.json` in this folder.
3. Temporary add-ons are removed when Firefox restarts — reload it each session.

## Why a fixed port

`47632` is hardcoded in both `background.js` and
[`crates/collector/src/ingest.rs`](../crates/collector/src/ingest.rs). There's
no config file yet, so if you change one, change the other.
