#!/usr/bin/env bash
# Build ScreenMCP Windows installer from Linux.
# Usage: ./build-installer.sh [version]
# Requires: makensis (apt install nsis), cargo + x86_64-pc-windows-gnu target
#
# Output: screenmcp-setup-<version>-x86_64.exe
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
RUSTUP="$(dirname "$(which cargo)")/rustup"

VERSION="${1:-$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="$SCRIPT_DIR/target/installer"
EXE_NAME="screenmcp-windows.exe"

echo "==> Building ScreenMCP Windows installer v${VERSION}"
echo ""

# ── 1. Cross-compile the Windows binary ─────────────────────────────────────
echo "[1/3] Compiling Windows binary (target: x86_64-pc-windows-gnu)..."
cd "$SCRIPT_DIR"

if ! "$RUSTUP" target list --installed | grep -q "x86_64-pc-windows-gnu"; then
  echo "    Adding x86_64-pc-windows-gnu target..."
  "$RUSTUP" target add x86_64-pc-windows-gnu
fi

VENDOR_DIR="$SCRIPT_DIR/vendor"
CARGO_HOME_TMP=""
if [ -d "$VENDOR_DIR" ]; then
  echo "    Using vendored dependencies from $VENDOR_DIR"
  CARGO_HOME_TMP="$(mktemp -d)"
  mkdir -p "$CARGO_HOME_TMP"
  trap 'rm -rf "$CARGO_HOME_TMP"' EXIT
  cat >"$CARGO_HOME_TMP/config.toml" <<EOF
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "$VENDOR_DIR"
EOF
  export CARGO_HOME="$CARGO_HOME_TMP"
  export CARGO_NET_OFFLINE=true
fi

CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  cargo build --release --target x86_64-pc-windows-gnu

echo "    Binary built: target/x86_64-pc-windows-gnu/release/screenmcp-windows.exe"

# ── 2. Prepare build directory ───────────────────────────────────────────────
echo "[2/3] Preparing installer build directory..."
mkdir -p "$BUILD_DIR"
cp "$SCRIPT_DIR/target/x86_64-pc-windows-gnu/release/screenmcp-windows.exe" "$BUILD_DIR/$EXE_NAME"
cp "$SCRIPT_DIR/installer.nsi" "$BUILD_DIR/installer.nsi"

# Update version in NSI script
sed -i "s/^!define APP_VERSION.*$/!define APP_VERSION   \"${VERSION}\"/" "$BUILD_DIR/installer.nsi"

# Copy icon if it exists
if [ -f "$SCRIPT_DIR/installer-icon.ico" ]; then
  cp "$SCRIPT_DIR/installer-icon.ico" "$BUILD_DIR/installer-icon.ico"
else
  echo "    (No installer-icon.ico found — installer will use default NSIS icon)"
  # Remove icon reference from NSI if no icon
  sed -i 's/^!define APP_ICON.*$/; no icon/' "$BUILD_DIR/installer.nsi"
fi

# ── 3. Build the installer ───────────────────────────────────────────────────
echo "[3/3] Building installer with makensis..."
cd "$BUILD_DIR"
makensis installer.nsi

INSTALLER_FILE="screenmcp-setup-${VERSION}-x86_64.exe"
echo ""
echo "==> Done!"
echo "    Installer: $BUILD_DIR/$INSTALLER_FILE"
echo "    Size: $(du -sh "$BUILD_DIR/$INSTALLER_FILE" | cut -f1)"
