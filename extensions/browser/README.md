# Makora tab reporter

[Makora](../../README.md) tracks apps by window focus, so a browser is one opaque block. This optional extension reports the active tab's URL so that time splits per site.

When the focused window's active tab changes, the extension sends `{ browser, url, title }` to `makora-bridge` through [native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging). Non-`http(s)` pages send `url: null`. The bridge passes each message to the running app over a local socket, and drops it if the app isn't running. Nothing leaves your machine.

Each time it starts, Makora registers the bridge with Chrome, Chromium, Edge, Brave and Firefox, if their profile folder exists. Install the browser first, or restart Makora after.

## Limitations

- **Snap and Flatpak browsers** can't start the bridge. Use a `.deb`, `.rpm` or vendor build.
- **Safari** is not supported.
- **Extension 0.1.x** no longer works. Update it.

## Build

From the repo root:

```sh
bun run ext:build     # bundle into dist/
bun run ext:watch     # rebuild on save
bun run ext:check     # typecheck
```

`dist/` is gitignored, so build once after cloning. Local builds pin the extension ID to `bapccffcoeagcmgkjlmdfhinnokhmnab`, which the bridge allows. `bun run ext:build --store` builds the Chrome Web Store package.

## Install

Start Makora once first, so it registers the bridge.

**Chrome, Edge, Brave and other Chromium browsers**

1. Open `chrome://extensions` (or `edge://extensions`, `brave://extensions`).
2. Turn on **Developer mode**.
3. Click **Load unpacked** and pick `dist/`.

After a rebuild, click **Reload** on the extension's card.

**Firefox**

1. Open `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on** and pick `dist/manifest.json`.

Firefox removes temporary add-ons when it restarts.
