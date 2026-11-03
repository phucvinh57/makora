#!/usr/bin/env bash
# Runs the checks that cover files changed since the merge base with main
# (committed and uncommitted). Usage: check.sh [--all] [base-ref]
set -uo pipefail

all=false
if [[ "${1:-}" == "--all" ]]; then
  all=true
  shift
fi
base="${1:-main}"

root=$(git rev-parse --show-toplevel)
cd "$root"
"$root/scripts/worktree-setup.sh" "$root"

changed=$({
  git diff --name-only "$(git merge-base "$base" HEAD)"
  git ls-files --others --exclude-standard
} | sort -u)

touches() {
  $all || grep -qE "$1" <<<"$changed"
}

failed=()
ran=()
run() {
  local name="$1"
  shift
  ran+=("$name")
  echo "::: $name: $*"
  if ! "$@"; then
    failed+=("$name")
  fi
}

frontend='^(src/|scripts/|extensions/|index\.html|package\.json|bun\.lock|tsconfig\.json|vite\.config\.ts|biome\.json|components\.json)'
if touches "$frontend"; then
  run biome bunx biome ci src extensions scripts
  run build bun run build
fi

if touches '^extensions/browser/'; then
  run ext-check bun run ext:check
fi

if touches '^src-tauri/'; then
  [[ -f dist/index.html ]] || bun run build >/dev/null
  run clippy bash -c 'cd src-tauri && cargo clippy --all-targets -- -D warnings'
  run cargo-test bash -c 'cd src-tauri && cargo test'
fi

echo
if ((${#ran[@]} == 0)); then
  echo "No checks cover the changed paths."
  exit 0
fi
if ((${#failed[@]} > 0)); then
  echo "FAILED: ${failed[*]} (ran: ${ran[*]})"
  exit 1
fi
echo "PASSED: ${ran[*]}"
