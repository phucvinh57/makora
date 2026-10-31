# Makora

**See where your computer time goes. Keep it on your computer.**

Makora is a desktop time tracker for Windows, macOS, and Linux. It quietly
records the apps you use so you can look back at your day, spot patterns, and
understand how you spend your time. No timers to start. No account to create.

<video src="https://github.com/user-attachments/assets/74aae38b-3bff-48b9-adc1-353526fdb4bb" controls></video>
*The demo uses fictional activity data.*

[Get Makora](https://github.com/phucvinh57/makora/releases) · [Contribute](CONTRIBUTING.md)

## Supported platforms

| Platform | Focus tracking support |
| --- | --- |
| Windows | Tracks the foreground app and window title. |
| macOS | Tracks the active app. Allow Makora in **System Settings → Privacy & Security → Accessibility** to include window titles. |
| Linux | GNOME Shell 45–51 with the [Makora GNOME extension](extensions/gnome/README.md), or a Wayland compositor that implements `wlr-foreign-toplevel-management-v1`, such as sway, Hyprland, or river. Other Linux desktop sessions without either focus source cannot track app usage. |

To track your detail browsing history, please install the [browser extension](extensions/browser/README.md).


## Why do I built Makora?

There are two main reasons. First, there are numerous activity tracking applications, such as [Rize](https://rize.io/) and [ActivityWatch](https://activitywatch.net/). However, data privacy is a big concern while using commercial tracking applications. Meanwhile, the categorizations of open-source solutions are not really sophisticated and rule-based only. I need a solution that not only protects my privacy, but also applies AI-based categorization for efficient analyses of my activities. The second reason is that I just want a deadly simple application, without any bloat features.

## Get started

Check [Releases](https://github.com/phucvinh57/makora/releases) for packaged
downloads for your operating system. If a build is not available yet, the
[contributing guide](CONTRIBUTING.md#building-and-running) explains how to run
Makora from source.

### macOS first launch

The macOS release is ad hoc signed and is not notarized by Apple. After moving
Makora to Applications, try opening it once. If macOS blocks it, open **System
Settings → Privacy & Security**, scroll to Security, and choose **Open Anyway**
for Makora.

If macOS still says Makora is damaged, and you downloaded it from the official
Makora release, remove the download quarantine flag in Terminal:

```sh
xattr -dr com.apple.quarantine /Applications/Makora.app
```

After opening Makora:

- **macOS:** Allow Makora in **System Settings → Privacy & Security →
  Accessibility** to include window titles. Without this permission, app time
  is still tracked, but titles are empty.
- **Linux with GNOME on Wayland:** Enable the
  [Makora GNOME extension](extensions/gnome/README.md#installing). DEB and RPM
  packages install it only when a GNOME Wayland session is detected. Other
  desktop environments skip it.
- **Web tracking:** Follow the
  [browser extension setup](extensions/browser/README.md#installing). Snap and
  Flatpak browsers cannot reach Makora, and extension versions before 0.2.0 stop
  reporting until updated; see its
  [limitations](extensions/browser/README.md#limitations).

Found a problem or have an idea? Fell free to [open an issue](https://github.com/phucvinh57/makora/issues).
