#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

APP_NAME="ScreenMCP"
BINARY_NAME="screenmcp-mac"
BUNDLE_DIR="${APP_NAME}.app"
DMG_NAME="${APP_NAME}.dmg"
DOCKER_IMAGE="joseluisq/rust-linux-darwin-builder:2.0.0-beta.1"
LIBDMG_DIR="/tmp/libdmg-hfsplus"
# Project root is one level up from mac/
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== ScreenMCP macOS Build + Package ==="

# --- Step 1: Cross-compile release binary via Docker ---
echo ""
echo "[1/5] Cross-compiling release binary..."
docker run --rm \
    --volume "${PROJECT_ROOT}":/root/src \
    --workdir /root/src/mac \
    "$DOCKER_IMAGE" \
    sh -c "rustup install 1.88.0 && rustup target add x86_64-apple-darwin --toolchain 1.88.0 && CC=o64-clang CXX=o64-clang++ cargo +1.88.0 build --release --target x86_64-apple-darwin 2>&1"

# Fix ownership (Docker runs as root)
sudo chown -R "$(id -u):$(id -g)" target/ 2>/dev/null || true

BINARY="target/x86_64-apple-darwin/release/${BINARY_NAME}"
if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi
echo "Binary: $(file "$BINARY")"

# --- Step 2: Create .app bundle ---
echo ""
echo "[2/5] Creating ${BUNDLE_DIR}..."
rm -rf "$BUNDLE_DIR"
mkdir -p "${BUNDLE_DIR}/Contents/MacOS"
mkdir -p "${BUNDLE_DIR}/Contents/Resources"

cp Info.plist "${BUNDLE_DIR}/Contents/Info.plist"
cp "$BINARY" "${BUNDLE_DIR}/Contents/MacOS/${BINARY_NAME}"
chmod +x "${BUNDLE_DIR}/Contents/MacOS/${BINARY_NAME}"

# App icon — build .icns from pre-made PNGs
ICON_PNG_DIR="assets/icon-pngs"
if [ -d "$ICON_PNG_DIR" ]; then
    mkdir -p /tmp/screenmcp.iconset
    cp "$ICON_PNG_DIR/16.png"   /tmp/screenmcp.iconset/icon_16x16.png
    cp "$ICON_PNG_DIR/32.png"   /tmp/screenmcp.iconset/icon_16x16@2x.png
    cp "$ICON_PNG_DIR/32.png"   /tmp/screenmcp.iconset/icon_32x32.png
    cp "$ICON_PNG_DIR/64.png"   /tmp/screenmcp.iconset/icon_32x32@2x.png
    cp "$ICON_PNG_DIR/128.png"  /tmp/screenmcp.iconset/icon_128x128.png
    cp "$ICON_PNG_DIR/256.png"  /tmp/screenmcp.iconset/icon_128x128@2x.png
    cp "$ICON_PNG_DIR/256.png"  /tmp/screenmcp.iconset/icon_256x256.png
    cp "$ICON_PNG_DIR/512.png"  /tmp/screenmcp.iconset/icon_256x256@2x.png
    cp "$ICON_PNG_DIR/512.png"  /tmp/screenmcp.iconset/icon_512x512.png
    cp "$ICON_PNG_DIR/1024.png" /tmp/screenmcp.iconset/icon_512x512@2x.png
    if command -v iconutil &>/dev/null; then
        iconutil -c icns /tmp/screenmcp.iconset -o "${BUNDLE_DIR}/Contents/Resources/AppIcon.icns"
    elif command -v png2icns &>/dev/null; then
        png2icns "${BUNDLE_DIR}/Contents/Resources/AppIcon.icns" /tmp/screenmcp.iconset/icon_*.png
    else
        echo "WARNING: No iconutil/png2icns found, copying 512px PNG as fallback"
        cp "$ICON_PNG_DIR/512.png" "${BUNDLE_DIR}/Contents/Resources/AppIcon.png"
    fi
    rm -rf /tmp/screenmcp.iconset
else
    echo "WARNING: Icon PNGs not found at $ICON_PNG_DIR, skipping app icon"
fi

echo "APPLscmc" > "${BUNDLE_DIR}/Contents/PkgInfo"

# Verify bundle
echo "Bundle created:"
find "$BUNDLE_DIR" -type f | sort

# --- Step 3: Build libdmg-hfsplus (if needed) ---
echo ""
echo "[3/5] Ad-hoc signing with rcodesign..."
if command -v rcodesign &>/dev/null; then
    rcodesign sign "$BUNDLE_DIR"
    echo "Signed: $(rcodesign extract "$BUNDLE_DIR/Contents/MacOS/$BINARY_NAME" 2>&1 | head -1 || echo 'ad-hoc')"
else
    echo "WARNING: rcodesign not found, skipping ad-hoc signing"
    echo "Install with: cargo install apple-codesign --bin rcodesign"
fi

echo ""
echo "[4/5] Preparing DMG tools..."

DMG_TOOL=""
if command -v dmg &>/dev/null; then
    DMG_TOOL="dmg"
elif [ -x "${LIBDMG_DIR}/build/dmg/dmg" ]; then
    DMG_TOOL="${LIBDMG_DIR}/build/dmg/dmg"
else
    echo "Building libdmg-hfsplus from source..."
    if ! command -v cmake &>/dev/null || ! command -v genisoimage &>/dev/null; then
        echo "Installing build dependencies..."
        sudo apt-get update -qq
        sudo apt-get install -y -qq cmake genisoimage zlib1g-dev
    fi

    if [ ! -d "$LIBDMG_DIR" ]; then
        git clone --depth 1 https://github.com/fanquake/libdmg-hfsplus.git "$LIBDMG_DIR"
    fi

    mkdir -p "${LIBDMG_DIR}/build"
    cd "${LIBDMG_DIR}/build"
    cmake .. -DCMAKE_BUILD_TYPE=Release
    make -j"$(nproc)"
    cd "$SCRIPT_DIR"

    if [ -x "${LIBDMG_DIR}/build/dmg/dmg" ]; then
        DMG_TOOL="${LIBDMG_DIR}/build/dmg/dmg"
    fi
fi

# --- Step 4: Create .dmg ---
echo ""
echo "[5/5] Creating ${DMG_NAME}..."

if [ -n "$DMG_TOOL" ] && command -v genisoimage &>/dev/null; then
    rm -f temp.iso "$DMG_NAME"
    genisoimage -D -V "$APP_NAME" -no-pad -r -apple -o temp.iso "$BUNDLE_DIR"
    "$DMG_TOOL" temp.iso "$DMG_NAME"
    rm -f temp.iso

    if [ -f "$DMG_NAME" ]; then
        echo "DMG created: ${DMG_NAME} ($(du -h "$DMG_NAME" | cut -f1))"
    else
        echo "ERROR: DMG creation failed"
        exit 1
    fi
else
    echo "DMG tools not available, falling back to .zip..."
    ZIP_NAME="${APP_NAME}.zip"
    rm -f "$ZIP_NAME"
    zip -r "$ZIP_NAME" "$BUNDLE_DIR"
    echo "ZIP created: ${ZIP_NAME} ($(du -h "$ZIP_NAME" | cut -f1))"
fi

echo ""
echo "=== Build complete ==="
echo "App bundle: ${SCRIPT_DIR}/${BUNDLE_DIR}"
ls -la "$DMG_NAME" 2>/dev/null || ls -la "${APP_NAME}.zip" 2>/dev/null || true
