# Makora

**See where your computer time goes. Keep it on your computer.**

Makora is a desktop time tracker for Windows, macOS, and Linux. It quietly
records the apps you use so you can look back at your day, spot patterns, and
understand how you spend your time. No timers to start. No account to create.

[Get Makora](https://github.com/phucvinh57/makora/releases) · [Contribute](CONTRIBUTING.md)

<video src="https://github.com/user-attachments/assets/74aae38b-3bff-48b9-adc1-353526fdb4bb" controls></video>

*The demo uses fictional activity data.*

## How it works

1. **Open Makora and go about your day.** It tracks the app in the foreground.
2. **Switch apps and Makora follows.** Each change starts a new activity block,
   with the app, window title, start time, and duration.
3. **Look back whenever you want.** History shows individual sessions;
   Statistics shows totals, your most-used apps and sites, and daily trends.

Closing or minimizing the window keeps Makora running in the system tray.
Open it again from the tray to check your activity; choose **Quit** there to
stop the app. You can also set it to launch when your computer starts.

## Make sense of your time

- **See the bigger picture.** Compare today, the past week, or longer periods
  with charts for your categories, apps, websites, and daily activity.
- **Find a specific moment.** Search your history by app, window title, or
  website, and filter by date or category.
- **Organize it your way.** Create categories and subcategories, assign them
  yourself, or use rules to file matching apps, sites, and pages automatically.
  Rules apply to both past and future activity; your manual choices take priority.
- **Get optional help sorting.** Download AI models in Makora to suggest
  categories locally. Review, accept, or correct the suggestions yourself.
- **Choose what gets recorded.** Exclude apps, websites, or pages, and ignore
  very short sessions. These choices affect future activity; existing history
  stays intact.
- **See browser time by website.** Add the optional
  [browser extension](browser-extension/README.md) to separate time on different
  sites. Without it, your browser is tracked as one app.

## Your activity stays yours

Your tracking history is stored on your computer. Makora does not upload it
to a cloud service, and you do not need an account. The browser extension
communicates only with Makora on your computer. Optional AI categorization
also runs on your computer; an internet connection is needed to download the
models first.

## Get started

Check [Releases](https://github.com/phucvinh57/makora/releases) for packaged
downloads for your operating system. If a build is not available yet, the
[contributing guide](CONTRIBUTING.md#building-and-running) explains how to run
Makora from source.

After opening Makora:

- **macOS:** Allow Makora in **System Settings → Privacy & Security →
  Accessibility** to include window titles. Without this permission, app time
  is still tracked, but titles are empty.
- **Linux with GNOME:** Enable the
  [Makora GNOME extension](gnome-extension/README.md#installing). It is required
  for tracking on GNOME. The Debian package includes it; other installations
  need the extension installed separately.
- **Website tracking:** Follow the
  [browser extension setup](browser-extension/README.md#installing).

## A few things to know

Makora measures time in the foreground app, rather than keyboard or mouse
activity. Watching a video without touching the keyboard still counts.

On Linux, tracking stops while your screen is locked or your computer is
asleep, then resumes when you return. This lock and sleep handling is not yet
available on Windows or macOS.

Some Windows Store apps may appear under a generic app name. macOS tracking
still needs broader validation on real hardware.

Found a problem or have an idea? [Open an issue](https://github.com/phucvinh57/makora/issues).
