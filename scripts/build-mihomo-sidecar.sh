#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MIHOMO_DIR="$ROOT_DIR/src-tauri/vendor/mihomo"
BIN_DIR="$ROOT_DIR/src-tauri/binaries"

if [[ ! -d "$MIHOMO_DIR/.git" && ! -f "$MIHOMO_DIR/.git" ]]; then
  echo "Mihomo submodule is missing. Run: git submodule update --init --recursive" >&2
  exit 1
fi

mkdir -p "$BIN_DIR"

(
  cd "$MIHOMO_DIR"
  go build -o "$BIN_DIR/mihomo" .
)

chmod +x "$BIN_DIR/mihomo"
echo "Built Mihomo sidecar: $BIN_DIR/mihomo"
