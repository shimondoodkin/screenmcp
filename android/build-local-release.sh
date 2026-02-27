#!/usr/bin/env bash
set -euo pipefail

# Local-only Android release build helper.
# Keeps CI/GitHub workflow untouched.

SDK_DIR="${SDK_DIR:-/opt/android-sdk}"

if [ ! -d "$SDK_DIR" ]; then
  echo "ERROR: Android SDK dir not found: $SDK_DIR"
  echo "Set SDK_DIR to your local Android SDK path, e.g.:"
  echo "  SDK_DIR=\"$HOME/Android/Sdk\" ./build-local-release.sh"
  exit 1
fi

cat > local.properties <<EOF
sdk.dir=$SDK_DIR
EOF

echo "Using sdk.dir=$SDK_DIR"
./gradlew assembleRelease
