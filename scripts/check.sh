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

if touches '^(scripts/|extensions/|demo/|package\.json|bun\.lock|biome\.json)'; then
  run biome bunx biome ci extensions scripts demo
fi

if touches '^(extensions/browser/|package\.json|bun\.lock)'; then
  run ext-check bun run ext:check
fi

if touches '^(demo/|package\.json)'; then
  run demo-install bun install --cwd demo --frozen-lockfile
  run demo-check bun run demo:check
fi

if touches '^(crates/|Cargo\.(toml|lock)|\.cargo/)'; then
  run clippy cargo clippy --workspace --all-targets -- -D warnings
  run cargo-test cargo test --workspace
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
