# makora focus reporter

A GNOME Shell extension companion to the [makora](../README.md) desktop app.
Makora tracks how long you spend in each application by watching which window
has focus. On Windows and macOS the app can watch that itself; on
Wayland it cannot, so on GNOME this extension is **required** rather than
optional — without it the app runs but records nothing.

## Why an extension is necessary

Wayland deliberately denies clients any view of other windows, and the two
usual escape hatches are both closed on GNOME:

| Approach | Why it doesn't work |
| --- | --- |
| `org.gnome.Shell.Introspect` D-Bus API | Exists, but is allowlisted to xdg-desktop-portal-gnome. Every other caller gets `Access denied`. |
| `ext-foreign-toplevel-list-v1` | Mutter implements it, but the protocol carries only `title`, `app_id` and `identifier` — **no focus state**. It can list windows, not say which one is focused. |
| `wlr-foreign-toplevel-management-v1` | Has the `activated` flag that would answer the question, but Mutter does not implement it. (Makora does support it, for sway/Hyprland/river.) |

That leaves code running inside GNOME Shell itself. This extension is the
smallest thing that can answer "which window is focused" and does nothing
else — no UI, no indicator, no network access.

## The contract

The extension owns `dev.phucvinh57.MakoraShell` on the **session** bus and
exports `dev.phucvinh57.MakoraShell.Focus1` at
`/dev/phucvinh57/MakoraShell`:

- `signal FocusChanged(b present, u pid, s app_id, s wm_class, s title)` —
  emitted when the focused window changes and when that window's title changes.
- `method GetFocusedWindow() -> (b present, u pid, s app_id, s wm_class, s title)` —
  so the app can pick up what's already focused the moment it starts,
  instead of recording nothing until you next switch apps.
- `property u Version` — the protocol version, currently `1`.

`present` is `false` when nothing is focused, which on GNOME mostly means
the Activities overview is open. Makora ignores those and keeps the running
session open, so glancing at the overview doesn't split a session in two.

`pid` is `0` when the compositor has no process for the window; Makora then
falls back to `app_id` for identity instead of `/proc/<pid>/exe`.

These names are duplicated in
[`src-tauri/src/core/platform/linux/gnome.rs`](../src-tauri/src/core/platform/linux/gnome.rs) —
if you change one, change the other, and bump `PROTOCOL_VERSION` in both if
the signature changes. The app warns on a version mismatch rather than
failing silently.

No authentication is needed or used: the session bus is per-user, so only
processes already running as you can reach it. (This is a stronger position
than the browser extension's ingest server, which relies on an `Origin`
header check.)

## Installing

From a git checkout:

```sh
./gnome-extension/install.sh
```

GNOME Shell only scans for new extensions **at startup** and does not watch
the directory, so on first install you have to log out and back in — under
Wayland the shell cannot be restarted in place. After that:

```sh
gnome-extensions enable makora@phucvinh57.dev
gnome-extensions info makora@phucvinh57.dev     # expect State: ACTIVE
```

Installing from the `.deb` puts the extension in
`/usr/share/gnome-shell/extensions/`, so only the `enable` step is needed.

## Checking it works

```sh
# what's focused right now
busctl --user call dev.phucvinh57.MakoraShell /dev/phucvinh57/MakoraShell \
    dev.phucvinh57.MakoraShell.Focus1 GetFocusedWindow

# watch focus changes live — alt-tab a few times
busctl --user monitor dev.phucvinh57.MakoraShell

# exceptions thrown inside the extension end up in the shell's journal
journalctl --user -f -o cat /usr/bin/gnome-shell
```

## Supported versions

`metadata.json` lists GNOME 45 through 50. GNOME 45 is the floor because
that's where extensions moved to ES modules. Adding a new GNOME release is
usually just a matter of appending to `shell-version`.
