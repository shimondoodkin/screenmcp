# ScreenMCP iOS - Feasibility Analysis & Companion App

## Executive Summary

iOS does **not** allow third-party apps to perform full UI automation the way Android does via AccessibilityService. The Android version of ScreenMCP relies on an AccessibilityService that can:
- Take screenshots of any app
- Perform arbitrary taps, drags, and gestures anywhere on screen
- Read the full UI tree of any app
- Inject text into any input field
- Press system navigation buttons (back, home, recents)

On iOS, **none of these capabilities are available to third-party apps** through public APIs. This document outlines what is possible, what is not, and recommends realistic approaches.

---

## What iOS Can Do (Stock / Non-Jailbroken)

| Capability | Status | Mechanism |
|---|---|---|
| WebSocket connection to ScreenMCP worker | YES | URLSessionWebSocketTask / Starscream |
| Take screenshots of own app | YES | UIGraphicsImageRenderer |
| Take screenshots of entire screen | NO | Prohibited by App Store / sandbox |
| Read notifications | PARTIAL | UNUserNotificationCenter (own app only) |
| Read other apps' UI elements | NO | No accessibility tree access for third-party apps |
| Perform taps/gestures on other apps | NO | No gesture injection API |
| Press Home/Back/Recents | NO | No system navigation API |
| Type text into other apps | NO | No cross-app text injection |
| Open URLs / deep links | YES | UIApplication.shared.open() |
| Open specific apps | YES | Via URL schemes |
| Camera capture | YES | AVCaptureSession |
| Clipboard read/write | YES | UIPasteboard.general |
| Get device info | YES | UIDevice APIs |
| Run Shortcuts | YES | Via URL schemes / SiriKit |
| Background WebSocket keepalive | PARTIAL | Background modes + push notifications |

## What iOS Can Do (Jailbroken)

On a jailbroken device, full automation is possible using private frameworks:
- **IOHIDEvent** for touch injection
- **SpringBoard hooks** for system actions
- **AccessibilityUIServer** for UI tree reading
- **Private screenshot APIs** for full screen capture

This is not suitable for App Store distribution but works for personal/testing use.

## What iOS Can Do (WebDriverAgent / XCTest)

Facebook's [WebDriverAgent](https://github.com/appium/WebDriverAgent) runs as an XCTest bundle and provides:
- Full screenshot capability
- Tap, swipe, and gesture injection
- UI element tree reading
- Text input

This requires a Mac running Xcode with the device connected (USB or network), or a developer-signed WDA app sideloaded onto the device. It is the approach used by Appium for iOS testing.

**This is the recommended path for full automation on iOS.**

---

## Recommended Approaches

### Approach 1: Companion App (included in this directory)

A lightweight iOS app that connects to the ScreenMCP worker via WebSocket. It can:
- Maintain a persistent WebSocket connection (with background keepalive)
- Respond to commands within its own sandbox
- Take screenshots of its own UI (limited usefulness)
- Read/write clipboard
- Open URLs and deep links to other apps
- Capture photos via camera
- Report device info and battery status
- Act as a bridge to iOS Shortcuts for limited automation

Supported commands: `clipboard_get`, `clipboard_set`, `open_url`, `camera`, `device_info`, `run_shortcut`
Unsupported commands: `screenshot` (system-wide), `click`, `long_click`, `drag`, `scroll`, `type`, `get_text`, `select_all`, `copy`, `paste`, `back`, `home`, `recents`, `ui_tree`

### Approach 2: WebDriverAgent Bridge (recommended for full automation)

Run WebDriverAgent on a Mac connected to the iOS device, and create a bridge service that:
1. Connects to the ScreenMCP worker as a "phone" role
2. Translates ScreenMCP commands into WDA HTTP requests
3. Forwards screenshots, UI trees, and gesture results back

This gives feature parity with the Android app but requires a Mac host.

### Approach 3: Jailbroken Device Tweak

For jailbroken iOS devices, a tweak can be written that:
1. Hooks into SpringBoard and UIKit
2. Runs a local WebSocket client
3. Executes full touch injection and screenshot capture

This provides full automation without a Mac but limits the user base.

---

## Companion App Structure (this directory)

```
ios/ScreenMCP/
  ScreenMCP/
    ScreenMCPApp.swift          - App entry point (SwiftUI)
    Models/
      Command.swift            - Command/response models matching worker protocol
      ConnectionState.swift    - Connection state enum
    Services/
      WebSocketService.swift   - WebSocket client (matches Android protocol)
      CommandHandler.swift     - Executes commands within iOS sandbox
      SettingsManager.swift    - Persists server URL and API key
    Views/
      ContentView.swift        - Main screen with connection status
      SettingsView.swift       - Server URL and API key configuration
    Resources/
      Info.plist               - App configuration
  ScreenMCP.xcodeproj/
    project.pbxproj            - Xcode project file
```

## Building

1. Open `ios/ScreenMCP/ScreenMCP.xcodeproj` in Xcode
2. Select your development team in Signing & Capabilities
3. Build and run on a device or simulator

Requirements:
- Xcode 15+
- iOS 16+ deployment target
- Swift 5.9+

## Protocol Compatibility

The iOS app uses the same WebSocket protocol as the Android app:
- Auth: `{"type": "auth", "token": "<token>", "role": "phone", "last_ack": 0}`
- Commands arrive as: `{"id": <num>, "cmd": "<command>", "params": {...}}`
- Responses sent as: `{"id": <num>, "status": "ok"|"error", "result": {...}, "error": "..."}`
- Heartbeat: respond to `{"type": "ping"}` with `{"type": "pong"}`
- Unsupported commands return: `{"id": <num>, "status": "ok", "result": {"unsupported": true, "reason": "..."}}`
