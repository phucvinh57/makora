# makora

A desktop app that tracks how long you spend in each application, as a
series of focus sessions: a session starts the moment an app gains focus and
ends the moment a different app takes focus. Sessions are stored locally in
SQLite — nothing leaves the machine.

It's a [Tauri](https://tauri.app) app: the focus watcher and the browser
tab-ingest server run in-process alongside the window, and closing or
minimizing the window hides it to the system tray instead of quitting —
tracking keeps running in the background, and the tray icon brings the
window back.

A companion browser [extension](extension) can optionally report the active
tab's URL, so browser time is broken down per site instead of showing up as
one opaque `chrome`/`firefox` block.
