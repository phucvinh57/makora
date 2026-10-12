# Contributing to the GNOME extension

The extension is a small JavaScript module that runs inside GNOME Shell. It
reports the focused window to Makora over the user's session D-Bus. There is
no build step.

## Develop locally

From the repository root, install the extension into your user account:

```sh
./extensions/gnome/install.sh
```

If this is the first install, log out and back in so GNOME Shell discovers the
new extension. Enable it with:

```sh
gnome-extensions enable makora@phucvinh57.dev
```

After editing `extension.js`, run the install script again and disable then
enable the extension to reload it:

```sh
./extensions/gnome/install.sh
gnome-extensions disable makora@phucvinh57.dev
gnome-extensions enable makora@phucvinh57.dev
```

The DEB and RPM packages install `metadata.json` and `extension.js` under
`/usr/share/gnome-shell/extensions/makora@phucvinh57.dev/`. If the extension
adds files, update the file list in
[`src-tauri/tauri.linux.conf.json`](../../src-tauri/tauri.linux.conf.json) too.

## D-Bus protocol

The extension owns `dev.phucvinh57.MakoraShell` on the session bus and exports
`dev.phucvinh57.MakoraShell.Focus1` at `/dev/phucvinh57/MakoraShell`:

- `FocusChanged(b present, u pid, s app_id, s wm_class, s title)` is emitted
  when the focused window changes or its title changes.
- `GetFocusedWindow() -> (b present, u pid, s app_id, s wm_class, s title)`
  returns the current focus so Makora can start tracking without waiting for a
  window switch.
- `Version` is a `u` property containing the protocol version, currently `1`.

`present` is false when there is no focused window, which usually means the
GNOME Activities overview is open. Makora ignores that state and keeps the
current activity block open. `pid` is zero when GNOME cannot provide a process
ID; Makora then uses `app_id` to identify the application.

The matching interface names and signatures are in
[`src-tauri/src/tracking/adapters/platform/linux/gnome.rs`](../../src-tauri/src/tracking/adapters/platform/linux/gnome.rs).
Keep the extension and Rust definitions in sync. If a signature changes, bump
`PROTOCOL_VERSION` in both places so Makora can detect mismatched versions.

## Debugging

Read the current focused window:

```sh
busctl --user call dev.phucvinh57.MakoraShell /dev/phucvinh57/MakoraShell \
    dev.phucvinh57.MakoraShell.Focus1 GetFocusedWindow
```

Watch focus changes while switching windows:

```sh
busctl --user monitor dev.phucvinh57.MakoraShell
```

GNOME Shell extension errors appear in the user journal:

```sh
journalctl --user -f -o cat /usr/bin/gnome-shell
```

Supported GNOME Shell versions are listed in [`metadata.json`](metadata.json).
