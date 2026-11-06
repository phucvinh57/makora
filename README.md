# Makora

**See where your computer time goes. Keep it on your computer.**

Makora is a desktop time tracker for Windows, macOS and Linux. It records the apps you use, with no timers and no account. It is a native Rust app built with [GPUI](https://www.gpui.rs/), and your activity stays in a local SQLite database.

AI sorts your activity into categories, on your machine by default. If you pick a remote AI provider instead, emails, phone numbers, keys, ids and words you list are replaced with placeholders first, and you can keep chosen apps and sites from being sent at all (**Settings → Privacy**).

Statistics show how much and _when_ you work: an hour-by-weekday table, a timeline of each day, and your working habits.

<video src="https://github.com/user-attachments/assets/74aae38b-3bff-48b9-adc1-353526fdb4bb" controls></video>
_The demo uses fictional activity data._

[Get Makora](https://github.com/phucvinh57/makora/releases) · [Contribute](docs/CONTRIBUTING.md)

## Why Makora

Commercial trackers such as [Rize](https://rize.io/) hold your activity on their servers. Open-source ones such as [ActivityWatch](https://activitywatch.net/) categorize by rules only. Makora keeps your data local, categorizes with AI, and stays simple.

## Install

Download a package from [Releases](https://github.com/phucvinh57/makora/releases), or [build from source](docs/CONTRIBUTING.md#build-and-run).

| Platform              | Download               | Install                                                                   |
| --------------------- | ---------------------- | ------------------------------------------------------------------------- |
| Debian, Ubuntu        | `.deb`                 | `sudo apt install ./makora_*.deb`                                         |
| Fedora, openSUSE      | `.rpm`                 | `sudo dnf install ./makora-*.rpm` or `sudo zypper install ./makora-*.rpm` |
| macOS (Apple silicon) | `.dmg`                 | Drag Makora to Applications                                               |
| Windows               | `-setup.exe` or `.msi` | Run the installer                                                         |

Then set up focus tracking for your platform:

| Platform                     | Setup                                                                                                                                    |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Windows                      | None. Tracks the foreground app and window title.                                                                                        |
| macOS                        | Allow Makora in **System Settings → Privacy & Security → Accessibility** to record window titles. Without it, app time is still tracked. |
| Linux, GNOME                 | Enable the [GNOME extension](extensions/gnome/README.md). Supports GNOME Shell 45–51.                                                    |
| Linux, sway, Hyprland, river | None. Works on any compositor with `wlr-foreign-toplevel-management-v1`.                                                                 |

Other Linux sessions cannot track app usage. On Linux you also need a Vulkan driver (`mesa-vulkan-drivers` on Debian, Ubuntu and Fedora; `libvulkan_radeon` or `libvulkan_intel` on openSUSE), and GNOME needs the AppIndicator extension to show the tray icon.

To split browser time by site, install the [browser extension](extensions/browser/README.md).

### macOS first launch

The macOS build is not notarized. If macOS blocks it, open **System Settings → Privacy & Security** and choose **Open Anyway**. If it still says Makora is damaged, and you downloaded it from the official release, run:

```sh
xattr -dr com.apple.quarantine /Applications/Makora.app
```

### Upgrading from a web-view release

Older releases can't see the new app, so both would track at once. Your history and settings are kept.

- **Linux:** the package stops the old copy. Start Makora again from the app menu, and re-pin it if it was pinned to the dock.
- **macOS:** quit the old Makora from its tray menu before replacing it.
- **Windows:** the installer offers to close it.

Found a problem or have an idea? [Open an issue](https://github.com/phucvinh57/makora/issues).
