#!/usr/bin/env bash
#
# Installs Makora's GNOME Shell extension for the current user.
#
# Packaged builds (.deb) ship the extension to
# /usr/share/gnome-shell/extensions and you can skip this entirely — this is
# for running from a git checkout.

set -euo pipefail

UUID="makora@phucvinh57.dev"
SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"

if [[ ! -f "$SOURCE/metadata.json" ]]; then
    echo "error: $SOURCE does not look like the extension source" >&2
    exit 1
fi

mkdir -p "$TARGET"
install -m 644 "$SOURCE/metadata.json" "$SOURCE/extension.js" "$TARGET/"
echo "Installed to $TARGET"

if gnome-extensions info "$UUID" >/dev/null 2>&1; then
    # Already known to the running shell, so it can be enabled right now.
    echo
    echo "Enable it with:"
    echo "    gnome-extensions enable $UUID"
else
    # GNOME Shell scans the extensions directory once, at startup, and does
    # not watch it — so a freshly installed extension is invisible until the
    # shell restarts. Under Wayland the shell cannot be restarted in place
    # (that is an X11-only trick), which makes logging out the only option.
    echo
    echo "GNOME Shell only scans for new extensions at startup, so it cannot see"
    echo "this one yet. Log out and back in, then run:"
    echo "    gnome-extensions enable $UUID"
fi

echo
echo "To check it is working:"
echo "    gnome-extensions info $UUID          # expect State: ACTIVE"
echo "    busctl --user call dev.phucvinh57.MakoraShell \\"
echo "        /dev/phucvinh57/MakoraShell \\"
echo "        dev.phucvinh57.MakoraShell.Focus1 GetFocusedWindow"
