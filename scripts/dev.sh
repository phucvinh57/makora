#!/usr/bin/env bash
# Builds the app and the browser bridge it registers from its own directory, then runs the app.
# Building the whole workspace at once lets both binaries share compiled dependencies.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
cargo build --workspace --bins
exec cargo run -p makora -- "$@"
