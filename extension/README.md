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
