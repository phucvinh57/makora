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

cargo_cfg="$root/.cargo/config.toml"
if [[ ! -f "$cargo_cfg" ]]; then
  mkdir -p "$(dirname "$cargo_cfg")"
  printf '[build]\ntarget-dir = "%s/target"\n' "$main" >"$cargo_cfg"
fi

if [[ ! -d "$root/node_modules" ]]; then
  (cd "$root" && bun install --frozen-lockfile) >&2
fi
