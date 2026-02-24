# Cross-Compiling screenmcp-mac to macOS on Linux

## Quick Start

Build the `.app` bundle and `.dmg` installer in one command:

```bash
cd mac/
bash build.sh
```

This produces:
- `ScreenMCP.app/` — macOS application bundle
- `ScreenMCP.dmg` — distributable disk image (2-3 MB)

## Prerequisites

- Docker
- `genisoimage`, `cmake`, `zlib1g-dev` (installed automatically by `build.sh` if missing)

### Pull the Docker image (one-time)

```bash
docker pull joseluisq/rust-linux-darwin-builder:2.0.0-beta.1
```

The image bundles:
- osxcross with ld64 (Apple linker)
- macOS 11.3 SDK (headers + frameworks)
- Rust toolchain with darwin targets
- clang configured as `o64-clang` / `o64-clang++`

## What build.sh Does

1. **Cross-compiles** the release binary via Docker targeting `x86_64-apple-darwin`
2. **Creates `.app` bundle** with Info.plist (LSUIElement, permission descriptions)
3. **Builds libdmg-hfsplus** from source if not already present (one-time)
4. **Creates `.dmg`** via `genisoimage` + `dmg` tool chain (falls back to `.zip` if unavailable)

### App bundle structure

```
ScreenMCP.app/Contents/
├── Info.plist          # Bundle config (menu bar agent, permissions)
├── PkgInfo
├── MacOS/
│   └── screenmcp-mac   # Mach-O x86_64 binary
└── Resources/
```

### Info.plist highlights

- `LSUIElement: true` — runs as menu bar agent (no Dock icon)
- `NSAccessibilityUsageDescription` — permission prompt for keyboard/mouse simulation
- `NSScreenCaptureUsageDescription` — permission prompt for screenshots
- Minimum macOS 10.13

## Manual Build (Without build.sh)

### Compile the binary

> **Note:** The Docker image ships Rust 1.87, but eframe 0.33 requires Rust 1.88+.
> The commands below install Rust 1.88 inside the container before building.
> Also note: Rust 1.93+ has a type inference regression that breaks winit 0.30.12,
> so use 1.88 specifically (not `stable`).

```bash
docker run --rm \
  --volume /home/user/screenmcp:/root/src \
  --workdir /root/src/mac \
  joseluisq/rust-linux-darwin-builder:2.0.0-beta.1 \
  sh -c "rustup install 1.88.0 && rustup target add x86_64-apple-darwin --toolchain 1.88.0 && CC=o64-clang CXX=o64-clang++ cargo +1.88.0 build --release --target x86_64-apple-darwin"
```

Output: `target/x86_64-apple-darwin/release/screenmcp-mac`

Fix ownership after Docker build:
```bash
sudo chown -R $(id -u):$(id -g) target/
```

### Build for aarch64 (Apple Silicon)

```bash
docker run --rm \
  --volume /home/user/screenmcp:/root/src \
  --workdir /root/src/mac \
  joseluisq/rust-linux-darwin-builder:2.0.0-beta.1 \
  sh -c "rustup install 1.88.0 && rustup target add aarch64-apple-darwin --toolchain 1.88.0 && CC=o64-clang CXX=o64-clang++ cargo +1.88.0 build --release --target aarch64-apple-darwin"
```

### Environment variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `CC` | `o64-clang` | C compiler for crates with C dependencies |
| `CXX` | `o64-clang++` | C++ compiler for crates with C++ dependencies |

Required when crates link against C/C++ code (native-tls, core-graphics, etc).

### Speed up rebuilds with cargo cache

```bash
docker run --rm \
  --volume /home/user/screenmcp:/root/src \
  --volume $HOME/.cargo/registry:/root/.cargo/registry \
  --volume $HOME/.cargo/git:/root/.cargo/git \
  --workdir /root/src/mac \
  joseluisq/rust-linux-darwin-builder:2.0.0-beta.1 \
  sh -c "CC=o64-clang CXX=o64-clang++ cargo build --release --target x86_64-apple-darwin"
```

## Installing on macOS

1. Transfer `ScreenMCP.dmg` to the Mac
2. Double-click to mount
3. Drag `ScreenMCP.app` to `/Applications`
4. Remove the Gatekeeper quarantine flag (required for unsigned apps):
   ```bash
   xattr -cr /Applications/ScreenMCP.app
   ```
5. Launch — the app appears as a menu bar icon (no Dock icon)
6. Grant Accessibility and Screen Recording permissions when prompted

## Code Signing

The build script **ad-hoc signs** the app using [`rcodesign`](https://github.com/indygreg/apple-platform-rs/tree/main/apple-codesign) — a Rust reimplementation of Apple's `codesign` that runs on Linux.

### Ad-hoc signing (current setup, done automatically)

Ad-hoc signing embeds a code signature without an Apple identity. This is done during `build.sh` via:

```bash
rcodesign sign ScreenMCP.app
```

**What ad-hoc signing gives you:**
- Valid Mach-O code signature (required by macOS on Apple Silicon)
- Avoids "killed" errors on M1/M2 Macs (unsigned binaries are killed immediately)
- No Apple Developer account needed

**What it doesn't give you:**
- Gatekeeper still blocks the app — user must `xattr -cr` or right-click > Open
- Not notarizable — can't pass Apple's notarization check

Install rcodesign (one-time):
```bash
cargo install apple-codesign --bin rcodesign
```

### Full signing + notarization (for distribution)

Requires an Apple Developer account ($99/year). Can be done on Linux with `rcodesign` or on macOS with `codesign`:

```bash
# With rcodesign on Linux (using a .p12 certificate exported from Keychain)
rcodesign sign --p12-file developer-id.p12 --p12-password-file password.txt ScreenMCP.app

# Or with codesign on macOS
codesign --force --deep --sign "Developer ID Application: Your Name (TEAM_ID)" ScreenMCP.app

# Notarize for distribution (macOS 10.15+, requires macOS or rcodesign)
rcodesign notarize --api-key-file key.json ScreenMCP.dmg --wait
# or
xcrun notarytool submit ScreenMCP.dmg --apple-id you@example.com --team-id TEAM_ID --password app-specific-password --wait
xcrun stapler staple ScreenMCP.dmg
```

## Why a .app Bundle Is Needed

A bare binary works for CLI tools, but macOS requires a `.app` bundle for:
- **Menu bar / tray icon** — the system won't show a tray icon for a raw binary
- **Permission prompts** — Accessibility and Screen Recording permissions are granted per-app bundle ID
- **LSUIElement** — the `Info.plist` flag that hides the app from the Dock

## Troubleshooting

- **Permission errors on target/**: Docker runs as root. Fix with `sudo chown -R $(id -u):$(id -g) target/`
- **Linker not found**: Make sure `CC=o64-clang` is set
- **Missing framework**: SDK may be too old for the crate
- **Slow first build**: Crates download inside the container. Use the cargo cache mount above
- **DMG creation fails**: Falls back to `.zip` automatically. To fix, ensure `genisoimage` and `cmake` are installed
- **No tray icon on macOS**: Make sure you're launching `ScreenMCP.app`, not the bare binary
