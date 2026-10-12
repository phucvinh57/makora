#!/bin/sh

if [ "${1:-0}" -eq 0 ]; then
    rm -f /usr/share/gnome-shell/extensions/makora@phucvinh57.dev/metadata.json \
        /usr/share/gnome-shell/extensions/makora@phucvinh57.dev/extension.js
    rmdir /usr/share/gnome-shell/extensions/makora@phucvinh57.dev 2>/dev/null || true
fi

exit 0
