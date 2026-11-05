#!/usr/bin/env bash
# Prepares a worktree: installs JS deps and shares the main checkout's cargo target dir.
set -euo pipefail

if [[ -n "${1:-}" ]]; then
  dir="$1"
elif [[ ! -t 0 ]]; then
  dir=$(jq -r '.cwd // empty' 2>/dev/null || true)
  dir="${dir:-$PWD}"
else
  dir="$PWD"
fi

root=$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null) || exit 0
main=$(dirname "$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)")
[[ "$root" == "$main" ]] && exit 0

mkdir -p "$main/target"
if [[ ! -e "$root/target" && ! -L "$root/target" ]]; then
  ln -s "$main/target" "$root/target"
fi

if [[ ! -d "$root/node_modules" ]]; then
  (cd "$root" && bun install --frozen-lockfile) >&2
fi
