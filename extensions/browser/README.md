# makora tab reporter

A small cross-browser WebExtension companion to the [makora](../../README.md)
desktop app. Makora tracks how long you spend in each application by
watching OS-level window focus, which means a browser is normally just one
opaque `chrome` or `firefox` block. This extension reports the active tab's
URL to makora so that block can be broken down per site instead.

It talks to makora over [native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging):
the browser starts `makora-bridge` (installed next to the app) and the
extension sends it `{ browser, url, title }` whenever the active tab of the
**focused** browser window changes — on tab switch, on navigation, and on
window focus change. Pages that are not `http(s)` are sent with `url: null`,
so their time is not credited to the last site. The bridge relays each
message to the running app over a per-user local socket and drops it while
the app is not running. Nothing leaves your machine.

Makora registers the bridge with Chrome, Chromium, Edge, Brave and Firefox
each time it starts (a host manifest named `dev.phucvinh57.makora` in the
browser's `NativeMessagingHosts` directory; the `HKCU` registry on Windows),
for browsers whose profile directory exists at that point. Install the browser
first, or restart makora after installing it.

This extension is entirely optional — without it loaded, browsers are
tracked like any other app.

### Limitations

- **Snap and Flatpak browsers** (Ubuntu's default Chromium/Firefox snaps,
  Flathub builds) are sandboxed and cannot start native-messaging hosts, so
  they cannot report tabs. Snap Firefox may work through the WebExtensions
  desktop portal, which reads `~/.mozilla/native-messaging-hosts` (untested).
  Use a `.deb`/`.rpm` or vendor build otherwise.
- **Safari** is not supported: it only talks to native code bundled inside
  its own app extension.
- **Extension 0.1.x** reported to a local HTTP server that makora no longer
  runs. Update the extension, or it stops reporting.

## Installing

### Building

The extension is written in TypeScript and has to be built before a browser
can load it. From the repo root:

```sh
bun run ext:build     # bundle src/background.ts into dist/
bun run ext:watch     # same, rebuilding on save
bun run ext:check     # typecheck (the build only strips types)
```

`dist/` is what browsers load — it holds the bundled `background.js` plus
copies of `manifest.json` and `icons/`. It is gitignored, so a fresh
checkout needs one `bun run ext:build` before the install steps below.

Local builds add a public `key` to `dist/manifest.json` so the unpacked
extension always gets the ID `bapccffcoeagcmgkjlmdfhinnokhmnab`, which
makora's host manifest allows next to the Chrome Web Store ID. The store
package is built with `bun run ext:build --store`, which leaves the key out.

### Chrome (and other Chromium browsers: Edge, Brave, ...)

1. Go to `chrome://extensions` (or the equivalent `edge://extensions`,
   `brave://extensions`, ...).
2. Enable **Developer mode** (top right).
3. Click **Load unpacked** and select this directory's `dist/` folder.
4. Start the makora app at least once so it registers the bridge, then
   reload the extension. While makora is not running, reports are dropped.

The extension has no options page and needs no configuration; it starts
reporting the active tab as soon as it's loaded. After a rebuild, hit
**Reload** on the extension's card to pick up the new bundle.

### Firefox

1. Go to `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on** and pick `dist/manifest.json`.
3. Start the makora app at least once so it registers the bridge.

Temporary add-ons are removed when Firefox restarts.
