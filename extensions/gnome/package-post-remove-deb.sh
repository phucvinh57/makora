#!/bin/sh

case "${1:-}" in
    remove|purge|disappear)
        rm -f /usr/share/gnome-shell/extensions/makora@phucvinh57.dev/metadata.json \
            /usr/share/gnome-shell/extensions/makora@phucvinh57.dev/extension.js
        rmdir /usr/share/gnome-shell/extensions/makora@phucvinh57.dev 2>/dev/null || true
        ;;
esac

exit 0
