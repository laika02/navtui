#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"

echo "==> Installing navtui with cargo"
cargo install --path "$ROOT_DIR" --locked --force

if command -v navtui >/dev/null 2>&1; then
  echo "==> navtui is available at: $(command -v navtui)"
  exit 0
fi

echo "==> navtui installed to: $CARGO_BIN_DIR/navtui"
if [[ ":$PATH:" != *":$CARGO_BIN_DIR:"* ]]; then
  echo "warning: $CARGO_BIN_DIR is not in PATH"
  echo "add this line to your shell profile and restart the shell:"
  echo "  export PATH=\"$CARGO_BIN_DIR:\$PATH\""
fi
