# Makora's GNOME focus reporter

This GNOME Shell extension lets [Makora](../../README.md) see which window is
active, so it can track your time by app in a GNOME Wayland session. It is
required there because GNOME does not expose window focus to regular desktop
apps.

The extension shares focus information only with Makora on your computer.
Nothing is sent over the internet, and your activity data stays on your
computer.

## Install and enable

Makora supports GNOME Shell 45 through 51.

### If you installed Makora from a DEB or RPM package

The package installs the extension only when it detects a GNOME Wayland
session. Enable **Makora's focus reporter** in the GNOME Extensions app, or run
this command in a terminal. If the package was installed outside your desktop
session, first run the installer from a GNOME Wayland terminal:

```sh
sudo bash /usr/share/makora/gnome-extension/install-system.sh
```

Then enable it:

```sh
gnome-extensions enable makora@phucvinh57.dev
```

### If you installed Makora another way

From a Makora source checkout, run the install script in GNOME on Wayland. It
skips installation for other sessions:

```sh
./extensions/gnome/install.sh
```

Then enable **Makora's focus reporter** in the GNOME Extensions app, or run:

```sh
gnome-extensions enable makora@phucvinh57.dev
```

GNOME Shell discovers newly installed extensions when it starts. If this is
your first install and GNOME does not find the extension, log out and back in,
then enable it.

Once the extension is enabled, Makora can track the active app. You do not
need to configure the extension.

## Troubleshooting

Check whether GNOME has enabled the extension:

```sh
gnome-extensions info makora@phucvinh57.dev
```

Look for `State: ACTIVE`. If it is inactive, enable it with the command above.
If GNOME cannot find it after installation, log out and back in so the shell
can discover it.

For development notes and protocol details, see
[`contributing.md`](contributing.md).
