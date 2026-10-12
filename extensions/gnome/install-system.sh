#!/usr/bin/env bash

set -euo pipefail

SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UUID="makora@phucvinh57.dev"
TARGET="/usr/share/gnome-shell/extensions/$UUID"

if ! bash "$SOURCE/session-is-gnome-wayland.sh"; then
    exit 0
fi

install -d "$TARGET"
install -m 644 "$SOURCE/metadata.json" "$SOURCE/extension.js" "$TARGET/"
