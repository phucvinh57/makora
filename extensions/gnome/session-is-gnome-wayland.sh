#!/usr/bin/env bash

set -euo pipefail

is_gnome_wayland() {
    local session_type="$1"
    local current_desktop="$2"
    local desktop
    local -a desktops

    [[ "$session_type" == "wayland" ]] || return 1

    IFS=: read -ra desktops <<< "$current_desktop"
    for desktop in "${desktops[@]}"; do
        [[ "${desktop,,}" == "gnome" ]] && return 0
    done

    return 1
}

if is_gnome_wayland "${XDG_SESSION_TYPE:-}" "${XDG_CURRENT_DESKTOP:-}"; then
    exit 0
fi

(( EUID == 0 )) || exit 1

for process_dir in /proc/[0-9]*; do
    [[ -r "$process_dir/comm" && -r "$process_dir/environ" ]] || continue
    [[ "$(<"$process_dir/comm")" == "gnome-shell" ]] || continue

    process_environment="$(tr '\0' '\n' < "$process_dir/environ" 2>/dev/null || true)"
    session_type="$(sed -n 's/^XDG_SESSION_TYPE=//p' <<< "$process_environment" | head -n 1)"
    current_desktop="$(sed -n 's/^XDG_CURRENT_DESKTOP=//p' <<< "$process_environment" | head -n 1)"

    if is_gnome_wayland "$session_type" "$current_desktop"; then
        exit 0
    fi
done

exit 1
