# Contributing to the GNOME extension

The extension is one JavaScript file that runs inside GNOME Shell and reports the focused window to Makora over the session D-Bus. There is no build step.

## Develop

```sh
./extensions/gnome/install.sh                   # install for your user
gnome-extensions enable makora@phucvinh57.dev   # after a first install, log out and in first
```

After editing `extension.js`, reinstall and reload:

```sh
./extensions/gnome/install.sh
gnome-extensions disable makora@phucvinh57.dev
gnome-extensions enable makora@phucvinh57.dev
```

Supported Shell versions are in [`metadata.json`](metadata.json).

## Packaging

The DEB and RPM packages ship `metadata.json`, `extension.js`, `session-is-gnome-wayland.sh` and `install-system.sh` in `/usr/share/makora/gnome-extension/`. [`post-install.sh`](../../packaging/linux/post-install.sh) runs `install-system.sh`, which installs to `/usr/share/gnome-shell/extensions/makora@phucvinh57.dev/`.

When adding a file, add it to both asset lists in `crates/makora/Cargo.toml` (`[package.metadata.deb]` and `[package.metadata.generate-rpm]`) and to the post-remove scripts.

## D-Bus protocol

The extension owns `dev.phucvinh57.MakoraShell` and exports `dev.phucvinh57.MakoraShell.Focus1` at `/dev/phucvinh57/MakoraShell`:

| Member                                                                    | Kind         | Purpose                                                   |
| ------------------------------------------------------------------------- | ------------ | --------------------------------------------------------- |
| `FocusChanged(b present, u pid, s app_id, s wm_class, s title)`           | signal       | Focused window or its title changed                       |
| `GetFocusedWindow() -> (b present, u pid, s app_id, s wm_class, s title)` | method       | Current focus, so tracking starts without a window switch |
| `Version`                                                                 | property `u` | Protocol version, currently `1`                           |

- `present` is false when nothing is focused, usually the Activities overview. Makora keeps the current block open.
- `pid` is 0 when GNOME can't provide it. Makora then uses `app_id`.

The Rust side is [`gnome.rs`](../../crates/makora-core/src/tracking/adapters/platform/linux/gnome.rs). Keep both in sync, and bump `PROTOCOL_VERSION` in both when a signature changes.

## Debugging

```sh
# Current focus
busctl --user call dev.phucvinh57.MakoraShell /dev/phucvinh57/MakoraShell \
    dev.phucvinh57.MakoraShell.Focus1 GetFocusedWindow

# Watch focus changes
busctl --user monitor dev.phucvinh57.MakoraShell

# Extension errors
journalctl --user -f -o cat /usr/bin/gnome-shell
```
