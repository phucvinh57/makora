#!/usr/bin/env bash

set -euo pipefail

UUID="makora@phucvinh57.dev"
SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"

if ! bash "$SOURCE/session-is-gnome-wayland.sh"; then
    echo "Skipping GNOME extension install; this is not a GNOME Wayland session."
    exit 0
fi

if [[ ! -f "$SOURCE/metadata.json" ]]; then
    echo "error: $SOURCE does not look like the extension source" >&2
    exit 1
fi

mkdir -p "$TARGET"
install -m 644 "$SOURCE/metadata.json" "$SOURCE/extension.js" "$TARGET/"
echo "Installed to $TARGET"

if gnome-extensions info "$UUID" >/dev/null 2>&1; then
    echo
    echo "Enable it with:"
    echo "    gnome-extensions enable $UUID"
else
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
