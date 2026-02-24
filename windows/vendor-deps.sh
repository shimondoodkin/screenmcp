#!/usr/bin/env bash
# Vendor Rust dependencies for offline builds.
# Usage: ./vendor-deps.sh
set -euo pipefail

# Source Rust environment if not already in PATH
if ! command -v cargo &>/dev/null; then
  if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
  elif [ -f "/home/user/.cargo/env" ]; then
    source "/home/user/.cargo/env"
  else
    echo "ERROR: cargo not found. Install Rust: https://rustup.rs"
    exit 1
  fi
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR_DIR="$SCRIPT_DIR/vendor"

mkdir -p "$VENDOR_DIR"

# This requires network access the first time. After that, build-installer.sh can run offline.
# We keep output quiet and deterministic by using the lockfile.
cargo vendor "$VENDOR_DIR" >/dev/null

echo "Vendored dependencies to: $VENDOR_DIR"
