#!/bin/sh

# Makora builds before the GPUI app (WebKitGTK windows) do not take its single-instance lock, so
# one left running across the upgrade would track alongside the new app. Stop those copies.
for exe in /proc/[0-9]*/exe; do
    [ "$(readlink "$exe" 2>/dev/null)" = "/usr/bin/makora (deleted)" ] || continue
    pid=${exe#/proc/}
    pid=${pid%/exe}
    if grep -q libwebkit2gtk "/proc/$pid/maps" 2>/dev/null && kill -TERM "$pid" 2>/dev/null; then
        echo "Stopped the previous Makora (pid $pid). Start Makora again to keep tracking."
    fi
done

exec /bin/bash /usr/share/makora/gnome-extension/install-system.sh
