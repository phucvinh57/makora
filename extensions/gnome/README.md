# Makora's GNOME focus reporter

GNOME on Wayland hides the active window from apps. This GNOME Shell extension tells [Makora](../../README.md) which window is focused, so it can track your time. It talks only to Makora on your computer.

Supports GNOME Shell 45–51.

## Install

**From a DEB or RPM package:** the package installs the extension if it detects a GNOME Wayland session. If you installed the package from outside that session, run this from a GNOME Wayland terminal:

```sh
sudo bash /usr/share/makora/gnome-extension/install-system.sh
```

**From source:** run `./extensions/gnome/install.sh` in a GNOME Wayland session.

Then enable it, in the Extensions app or with:

```sh
gnome-extensions enable makora@phucvinh57.dev
```

No configuration is needed.

## Troubleshooting

```sh
gnome-extensions info makora@phucvinh57.dev
```

Look for `State: ACTIVE`. If GNOME can't find the extension after a first install, log out and back in.

Development notes and the D-Bus protocol are in [contributing.md](contributing.md).
