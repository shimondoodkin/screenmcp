#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

APP_NAME="ScreenMCP"
BINARY_NAME="screenmcp-linux"
VERSION="0.1.0"
ARCH="amd64"
DEB_NAME="${BINARY_NAME}_${VERSION}_${ARCH}"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== ScreenMCP Linux Build + Package ==="

# --- Step 1: Build release binary ---
echo ""
echo "[1/3] Building release binary..."
cargo build --release
BINARY="target/release/${BINARY_NAME}"
if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi
echo "Binary: $(file "$BINARY") ($(du -h "$BINARY" | cut -f1))"

# --- Step 2: Build .deb package ---
echo ""
echo "[2/3] Creating .deb package..."
DEB_DIR="/tmp/${DEB_NAME}"
rm -rf "$DEB_DIR"

# Binary
mkdir -p "$DEB_DIR/usr/bin"
cp "$BINARY" "$DEB_DIR/usr/bin/${BINARY_NAME}"
chmod 755 "$DEB_DIR/usr/bin/${BINARY_NAME}"
strip "$DEB_DIR/usr/bin/${BINARY_NAME}" 2>/dev/null || true

# Desktop entry
mkdir -p "$DEB_DIR/usr/share/applications"
cp screenmcp.desktop "$DEB_DIR/usr/share/applications/"

# Icon
mkdir -p "$DEB_DIR/usr/share/icons/hicolor/512x512/apps"
cp assets/icon-app.png "$DEB_DIR/usr/share/icons/hicolor/512x512/apps/screenmcp.png"

# Calculate installed size in KB
INSTALLED_SIZE=$(du -sk "$DEB_DIR" | cut -f1)

# DEBIAN control file
mkdir -p "$DEB_DIR/DEBIAN"
cat > "$DEB_DIR/DEBIAN/control" <<EOF
Package: screenmcp
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Installed-Size: ${INSTALLED_SIZE}
Depends: libgtk-3-0, libssl3 | libssl1.1, libxdo3, wmctrl
Maintainer: ScreenMCP <support@screenmcp.com>
Homepage: https://screenmcp.com
Description: AI desktop control via MCP
 ScreenMCP gives AI assistants real-time vision and control
 over desktop computers via the Model Context Protocol.
 System tray app that connects to a ScreenMCP worker.
EOF

# Post-install: update icon cache
cat > "$DEB_DIR/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications 2>/dev/null || true
fi
EOF
chmod 755 "$DEB_DIR/DEBIAN/postinst"

# Post-remove: refresh caches
cat > "$DEB_DIR/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications 2>/dev/null || true
fi
EOF
chmod 755 "$DEB_DIR/DEBIAN/postrm"

# Build the .deb
dpkg-deb --build --root-owner-group "$DEB_DIR" "${SCRIPT_DIR}/${DEB_NAME}.deb"
rm -rf "$DEB_DIR"

# --- Step 3: Verify ---
echo ""
echo "[3/3] Verifying package..."
dpkg-deb --info "${DEB_NAME}.deb"
echo ""
echo "Contents:"
dpkg-deb --contents "${DEB_NAME}.deb"

echo ""
echo "=== Build complete ==="
echo "Package: ${SCRIPT_DIR}/${DEB_NAME}.deb ($(du -h "${DEB_NAME}.deb" | cut -f1))"
echo "Install: sudo dpkg -i ${DEB_NAME}.deb"
