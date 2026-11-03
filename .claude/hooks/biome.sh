#!/usr/bin/env bash
file=$(jq -r '.tool_input.file_path // empty')
[[ -f "$file" ]] || exit 0
root=$(git -C "$(dirname "$file")" rev-parse --show-toplevel 2>/dev/null) || exit 0

case "${file#"$root"/}" in
  src/bindings/* | node_modules/* | demo/* | .claude/worktrees/*) exit 0 ;;
  *.ts | *.tsx | *.js | *.mjs | *.css | *.json) ;;
  *) exit 0 ;;
esac

cd "$root" || exit 0
if ! out=$(bunx biome check --write --no-errors-on-unmatched "$file" 2>&1); then
  echo "$out" >&2
  exit 2
fi
