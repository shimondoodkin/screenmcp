# iPhone Client — Goto Technology

How a real "phone-role" iOS client for ScreenMCP can be built. This is the
implementation playbook: what to build, what to reuse, where each protocol
lives, what Apple lets you do, and what it doesn't.

It assumes the existing ScreenMCP architecture
(`Phone/Desktop ↔ WSS ↔ Worker (Rust) ↔ MCP Server (Node.js)`) and slots iOS
in as a new "phone" device, exactly like Android — just with a much bigger
host-side machinery because Apple does not give us `adb`.

---

## 1. Goal

Add an `iphone` device role to ScreenMCP that supports the full phone command
set (`screenshot`, `click`, `long_click`, `drag`, `scroll`, `type`, `get_text`,
`ui_tree`, `back`, `home`, `recents`, `camera`, clipboard, …) — system-wide,
not just inside one app, on real iPhones and iOS Simulator.

Three personas to support:

1. **User who owns an iPhone + a Mac/PC** — plug in via USB, ScreenMCP host
   process runs locally, iPhone is the controlled "phone" device.
2. **User with only an iPhone, no laptop** — uses our hosted "iphone bridge"
   server (parallel to mobile-use.com's model). Out of scope for v1.
3. **Developer testing in iOS Simulator** — no hardware needed.

Non-goals (for v1):

- Jailbreak tweak path.
- Cross-app automation without an XCTest runner installed.
- App Store distribution of an iOS-side app that does the controlling
  (Apple won't allow it; the controlling agent has to be an XCTest runner
  installed via developer signing).

---

## 2. The Hard Truth About iOS Automation

Apple does not expose:

- A user-facing "tap (x,y)" API to third-party apps.
- A cross-app accessibility tree to third-party apps.
- A "screenshot the whole screen" API to third-party apps.
- An equivalent of Android's AccessibilityService.

The **only** stock-iOS path that gives all of those is **XCTest** —
specifically, an `XCUIApplication`-driven test runner, which Apple permits
because it's a developer/test tool. This is what Appium's WebDriverAgent
uses; what mobile-next/devicekit-ios uses; what Detox uses. There is no
other supported path.

Implication: anything we build will need a small Swift/Obj-C XCTest runner
IPA that lives on the iPhone. That's not optional. The interesting question
is what runs on the host side.

Existing `ios-not-availible/README.md` already documents this; we accept it
and design around it.

---

## 3. Two-Sided Architecture

```
┌──────────────────────────────┐                ┌──────────────────────────┐
│  HOST  (Mac / Linux / Win)   │                │  DEVICE  (iPhone / Sim)  │
│                              │                │                          │
│  screenmcp-iphone-client     │                │  ScreenMCP-iOS-Agent     │
│  (Rust binary)               │                │  (XCTest runner .ipa)    │
│                              │                │                          │
│  ┌────────────────────────┐  │   USB / Wi-Fi  │  ┌────────────────────┐  │
│  │ ScreenMCP WSS client   │  │   lockdownd /  │  │ Local HTTP server  │  │
│  │ (talks to worker)      │  │   usbmuxd /    │  │ on :12004          │  │
│  └────────────┬───────────┘  │   RemoteXPC    │  │                    │  │
│               │              │                │  │ Tap, swipe, type,  │  │
│  ┌────────────▼───────────┐  │  ◄───────────► │  │ screenshot, dump   │  │
│  │ Device-control bridge  │  │   port-fwd     │  │ source, app launch │  │
│  │ (idevice + http)       │  │                │  │                    │  │
│  └────────────────────────┘  │                │  │ MJPEG stream :12005│  │
│                              │                │  └────────────────────┘  │
└──────────────────────────────┘                └──────────────────────────┘
```

- **Host side** = a Rust binary, ships with the rest of ScreenMCP. Connects
  to the existing worker as a phone-role client. Translates ScreenMCP
  commands into HTTP calls against the on-device agent.
- **Device side** = a signed XCTest runner IPA that exposes a tiny HTTP
  server. It does the actual XCUITest work (taps, dump, screenshot).

The host side is the new code; the device-side IPA can largely be reused or
forked from existing OSS (see §6).

---

## 4. Where iOS Stops You — and What Each Wall Costs

| Wall | Why it exists | Workaround |
|---|---|---|
| Sandboxed third-party apps can't tap other apps | Sandbox / entitlements | XCTest runner (developer signed) |
| Sandboxed apps can't screenshot the whole screen | Sandbox | XCTest's `XCUIScreen.main.screenshot()` from inside the test runner |
| Need to install custom code on the device | Code-signing | Apple Developer account, provisioning profile that includes the device's UDID |
| Need to talk to lockdown over USB | Apple's pairing protocol | Pair once over USB, then trust record persists |
| iOS 17+ tightens device communication | Apple moved to RemoteXPC + tunnel | userspace TUN tunnel (go-ios style) |
| Auto-launch on boot | iOS doesn't allow background daemons | Either keep the host running (it relaunches the runner), or leverage XCTest's process-lifetime tricks |

**Net cost of the iOS setup tax**: Apple Developer account ($99/yr), a Mac
to sign with (or a remote signing service), one USB pairing per device, and
re-signing every time the provisioning profile expires. There is no way
around this on stock iOS.

---

## 5. Host-Side Stack (Rust)

This is the new code we'd actually write. It lives in
`iphone-client/host/` (a Rust crate) and is bundled with ScreenMCP the way
the other desktop clients are.

### 5.1 Transport — talking to the iPhone over USB / Wi-Fi

The wire protocol Apple uses is **lockdownd** over **usbmuxd** (USB) or TCP
(Wi-Fi). Above lockdownd, services like `installation_proxy`,
`com.apple.testmanagerd.lockdown`, `instruments`, `mobile.diagnostics_relay`
do the real work. iOS 17+ adds **RemoteXPC** through a userspace **TUN
tunnel** for many services.

The dominant Go implementation (`go-ios`) is what mobilecli wraps. The
dominant Rust implementation is:

- **[`idevice`](https://github.com/jkcoxson/idevice)** — pure Rust, MIT,
  active. Supports lockdown, installation_proxy, instruments, DVT, and is
  adding RemoteXPC + tunnel support.
- Backup option: **`rusty_libimobiledevice`** — bindings to the C
  `libimobiledevice`. Loses single-binary benefit; only consider if
  `idevice` is missing something we need.

What we'll exercise from `idevice`:

- `lockdownd::Client` — handshake, pair record, get device info.
- `installation_proxy` — install/upgrade our agent IPA.
- `instruments` / `dvt` — used to launch the XCTest runner process.
- `port_forward` — forward `localhost:RAND` → device `:12004` (HTTP) and
  `:12005` (MJPEG). This is how mobilecli already wires it
  (`devices/ios.go:1441-1490`).
- (later) `tunnel` — RemoteXPC tunnel for iOS 17+.

For iOS Simulator we skip all of the above and talk directly to
`localhost:<agent-port>` because the simulator runs in the host process
space and the agent listens on a real local port.

### 5.2 Agent client — talking to the on-device runner

Once port-forwarded, the agent is just an HTTP server. Use:

- **`reqwest`** — JSON over HTTP for command calls (tap/swipe/dump/etc).
- **`tokio`** + **`tokio-tungstenite`** — already in screenmcp; reuse for
  any WebSocket calls and for the MJPEG-as-frames consumer if we want
  realtime preview later.
- **`serde` / `serde_json`** — request/response shapes.
- **`bytes` / `image`** — for screenshot processing.

We don't have to invent the agent's protocol. The DeviceKit agent already
has one; if we fork it, we adopt it. If we write our own minimal runner,
keep the surface tiny (see §7).

### 5.3 ScreenMCP integration

The host process registers with the existing ScreenMCP worker exactly like
the Android app does:

- WS connect, auth message: `{"type":"auth","token":"<user.id>","role":"phone","last_ack":0}`
- Heartbeat: respond to `{"type":"ping"}` with `{"type":"pong"}`
- Commands arrive: `{"id":N,"cmd":"...","params":{...}}`
- Responses leave: `{"id":N,"status":"ok"|"error","result":{...},"error":"..."}`

Reuse whatever WSS client code lives in `worker/` or build a thin wrapper.
Same protocol Android/desktop clients speak — that's the contract.

### 5.4 Suggested crate layout

```
iphone-client/
  README.md
  GOTO_TECHNOLOGY.md            (this file)
  host/                          (Rust crate)
    Cargo.toml
    src/
      main.rs                    bin: screenmcp-iphone
      ws_client.rs               ScreenMCP worker WSS client (reuses worker/ types)
      device/
        mod.rs                   DeviceController trait (mirrors Android)
        real.rs                  RealDevice  (USB / Wi-Fi via idevice)
        simulator.rs             SimulatorDevice (no transport)
        agent_client.rs          HTTP client for on-device agent
        port_forward.rs          USB / TUN port forwarding
        tunnel.rs                iOS 17+ RemoteXPC tunnel
      install/
        mod.rs                   "install/upgrade agent" flow
        signing.rs               provisioning profile detection
      commands/
        screenshot.rs
        gesture.rs               click / long_click / drag / scroll
        text.rs                  type / get_text
        nav.rs                   back / home / recents
        clipboard.rs
        ui_tree.rs               source dump → ScreenMCP UiTree shape
        camera.rs                (passthrough; AVFoundation lives in agent)
      config.rs                  reads ~/.screenmcp/worker.toml
  agent/                         (Swift, optional — if we ship our own)
    ScreenMCPAgent/
      *.swift
      ScreenMCPAgent.xcodeproj
```

---

## 6. Reusing or Forking the On-Device Agent

We have three realistic agent strategies:

### Option A — Reuse mobile-next/devicekit-ios (fast)

It's already an XCTest runner with HTTP on 12004 + MJPEG on 12005.
**License is the deciding factor** — verify it's permissive (Apache-2.0 or
MIT). If permissive, fork it, rebrand the bundle ID
(`com.screenmcp.agent`-style), pin our own SHA-256 of the IPA, ship.

### Option B — Reuse appium/WebDriverAgent (battle-tested)

WDA is BSD-licensed, hugely battle-tested, and the protocol is well
documented. Downside: it's a kitchen-sink JSON-WireProtocol surface; you
inherit a lot. Upside: every change Apple ships breaks it eventually,
*and* gets fixed by the Appium community in days.

### Option C — Write our own minimal runner (cleanest, most work)

A tiny Swift XCTest target that exposes only what ScreenMCP needs:

```
POST /tap            { x, y }
POST /swipe          { x1, y1, x2, y2, duration_ms }
POST /long_press     { x, y, duration_ms }
POST /type           { text }
POST /press_button   { button }     // home, volume_up, volume_down, power, lock
GET  /screenshot                    -> PNG bytes
GET  /source                        -> JSON tree
GET  /foreground_app                -> { bundleId, name }
POST /launch_app     { bundleId }
POST /terminate_app  { bundleId }
GET  /clipboard
POST /clipboard      { text }
GET  /healthz
```

Scope is small enough that an experienced iOS engineer can write it in a
week. The thing doing the actual work is `XCUIApplication` and friends —
the runner is just a thin server in front of them. SpringBoard
(`com.apple.springboard`) is treated as an `XCUIApplication` like any
other.

**Recommendation**: start with Option A if license is clean; fall back to
Option C if not. Skip Option B unless we want WDA-protocol compatibility
with other tooling.

---

## 7. The Wire Protocol Inside the Agent

The agent does not need to understand ScreenMCP's protocol. The host
translates. Keep the agent surface:

- Stateless (every request stands alone).
- JSON in, JSON or PNG out.
- A single `Bearer` token check (random per launch, given to the host
  out-of-band when the runner starts).
- Idempotent where possible (taps aren't, but `launch_app` is).

UI tree shape inside the agent should map cleanly onto ScreenMCP's
`ui_tree`:

```json
{
  "type": "Button",
  "label": "Sign in",
  "bounds": { "x": 100, "y": 200, "w": 80, "h": 44 },
  "enabled": true,
  "selected": false,
  "children": [...]
}
```

Strip iOS-specific noise on the way out: drop the `XCUIElementType` prefix,
filter invisible elements (negative bounds, zero size), keep only the node
types ScreenMCP actually uses (Button, StaticText, TextField,
SecureTextField, Switch, Image, Icon, SearchField — same shortlist
mobilecli uses in `devices/wda/source.go`).

---

## 8. Mapping ScreenMCP Commands to iOS

| ScreenMCP cmd       | iOS path |
|---------------------|----------|
| `screenshot`        | `XCUIScreen.main.screenshot()` → PNG |
| `click(x,y)`        | XCUITest `tap` at coordinate |
| `long_click(x,y)`   | XCUITest `press(forDuration:)` |
| `drag(x1,y1,x2,y2)` | XCUITest `press(forDuration:thenDragTo:)` |
| `scroll`            | `swipeUp/Down` in the targeted region |
| `type(text)`        | `typeText` on focused element; for non-ASCII, push to clipboard then synthesize Cmd-V via accessibility (or use the agent's keyboard helpers) |
| `get_text`          | recurse over `XCUIApplication` for text/value/label |
| `ui_tree`           | `XCUIApplication(bundleIdentifier: foreground).snapshot()` — filtered |
| `back`              | per-app gesture (no global back button on iOS); fall back to `swipe from left edge` |
| `home`              | `XCUIDevice.shared.press(.home)` |
| `recents`           | `XCUIDevice.shared.press(.home, forDuration:)` (double-tap on devices with home button), or app-switcher gesture on home-button-less devices |
| `camera`            | `AVFoundation` inside the agent or the host (decide where) |
| `get_clipboard`     | `UIPasteboard.general.string` (via the agent app process — note Apple's clipboard-access banner) |
| `set_clipboard`     | `UIPasteboard.general.string =` |
| `play_audio`        | `AVAudioPlayer` |

What we **cannot** do, even with XCTest:

- Bypass a locked device (must be unlocked for the agent to drive UI).
- Read secure text fields (Apple strips them from accessibility).
- Operate inside Apple Pay sheets, Face ID confirmations.
- Skip the system "<App> wants to access the clipboard" toast on iOS 16+
  (mitigation: avoid clipboard for routine `type`; only fall back to it
  for non-ASCII text).

---

## 9. Connection Paths

The host crate must support all of these — abstract them behind a single
`Transport` trait:

### 9.1 USB

`usbmuxd` is a system service on macOS (built in) and Windows (installed
with iTunes/Apple Mobile Device Support). On Linux, we need
`usbmuxd` running. `idevice` connects to the local socket and multiplexes.

### 9.2 Wi-Fi

After USB pairing, the device's pair record persists. If the device is on
the same network and "Sync over Wi-Fi" is enabled (or in dev mode is just
trusted), `usbmuxd` advertises it the same way. Code path is identical
once the device shows up.

### 9.3 iOS 17+ tunnel

iOS 17 added RemoteXPC; many services moved behind a userspace tunnel.
`go-ios` invented the userspace TUN approach; `idevice` is implementing
the same. Until that lands stably, we either:

- Document iOS ≤16 as supported in v1 and iOS 17+ as best-effort.
- Or bundle a small helper (a Go binary running just the tunnel) — but
  this re-introduces the AGPL surface we wanted to avoid.

### 9.4 iOS Simulator

`SimDevice` API via `xcrun simctl`. Talk to the agent on `localhost:<port>`
directly. No `idevice` involvement. Good for CI and demos.

### 9.5 Cloud / hosted bridge (later)

Same `Transport` trait, just talking JSON-RPC to a remote endpoint that
owns the cable (mobile-use.com pattern). Out of scope for v1, but the
trait shape should not preclude it.

---

## 10. Install / Bootstrap Flow

What `screenmcp-iphone install` does the first time:

1. Detect connected device via `idevice`. Print UDID.
2. Check if our agent is already installed (look for our bundle ID via
   `installation_proxy`).
3. If not: download the signed IPA from a pinned URL (we host releases on
   GitHub like mobilecli does), verify SHA-256 against a hardcoded map,
   install via `installation_proxy.install`.
4. For real devices: confirm the developer profile and provisioning
   profile cover this UDID. If not, point user at instructions to add the
   device to their developer account.
5. Launch the runner via `testmanagerd` (DTX). Wait until the runner's
   HTTP server reports healthy.
6. Forward host port → device :12004 and :12005.
7. Hand control to the daemon mode (`screenmcp-iphone run`) which connects
   to the ScreenMCP worker as a phone.

Pin checksums in code, the way mobilecli does:

```rust
const AGENT_CHECKSUMS: &[(&str, &str)] = &[
    ("screenmcp-ios-runner.ipa",         "sha256:..."),
    ("screenmcp-ios-Sim-arm64.zip",      "sha256:..."),
    ("screenmcp-ios-Sim-x86_64.zip",     "sha256:..."),
];
```

---

## 11. Build, Sign, Distribute

The host side is a normal Rust binary — `cargo build --release` per
target triple. We ship it alongside the existing `windows/`, `mac/`,
`linux/` desktops.

The agent IPA is the gnarly part:

- Built on a Mac with Xcode (cloud signing services exist but a Mac is
  simplest).
- Signed with an Apple Developer Program account ($99/yr).
- Distributed as a `.ipa` for real devices and `.zip` for simulator.
- We host releases on GitHub. Users with a developer account can re-sign
  for their own UDIDs (mobilecli supports `--force-resign` flag — copy
  this UX).

For users without a developer account, the only options are:

- Use the simulator only.
- Use a hosted "iphone bridge" we'd run.
- Use third-party signing services (sideloadly etc.) — out of scope to
  recommend officially.

---

## 12. Licensing Strategy

- **Host crate** — same license as the rest of screenmcp. Ours.
- **Agent IPA** — depends on which option we pick in §6. If we fork
  devicekit-ios or WDA, retain their license headers and re-publish under
  a compatible license. If we write our own, ours.
- **`idevice` crate** — MIT. No contamination.
- **Avoid mobilecli/go-ios** at runtime: they're AGPL-3.0. Linking against
  them or shipping modified versions infects screenmcp-cloud's SaaS code.
  Treating mobilecli as an external unmodified subprocess is *probably*
  safe, but writing our own host with `idevice` removes the question
  entirely.

---

## 13. Phased Plan

### Phase 0 — Decide & spike (1 week)

- Confirm devicekit-ios license. If permissive, agent strategy = fork it.
- Spike: write a 100-line Rust program that uses `idevice` to list
  devices, install an IPA (e.g. a hello-world), and launch it via DTX.
  This validates the `idevice` surface end to end.

### Phase 1 — USB-only, iOS ≤16, Simulator (3–4 weeks)

- `screenmcp-iphone` Rust binary.
- Agent fork with stable HTTP API.
- Commands: screenshot, tap, swipe, type, ui_tree, home, foreground_app,
  launch/terminate_app.
- Connects to existing screenmcp worker as a phone.
- Documented Mac-only setup (because the agent has to be signed on a Mac
  anyway in v1).

### Phase 2 — Production polish (3–4 weeks)

- Wi-Fi support.
- Multi-device.
- Reconnect logic, agent health-checks, automatic re-launch when iOS
  kills the runner.
- Re-sign UX (`--provisioning-profile`).
- Camera, clipboard, full command surface.
- CI on iOS Simulator (`xcrun simctl`).

### Phase 3 — iOS 17+ tunnel (2–4 weeks, partly upstream contribution)

- Either wait for `idevice` to land RemoteXPC + TUN tunnel, or contribute.
- Drop the iOS 16 ceiling once it's stable.

### Phase 4 — Hosted bridge (open ended)

- Run a fleet of Mac minis with cabled iPhones.
- Same wire protocol, controlled through screenmcp-cloud.
- Optional. Only if there's user demand.

---

## 14. Open Questions

- **Agent license** — devicekit-ios shows Apache-2.0 at the top level but
  this needs verification on the actual `devicekit-ios` agent repo before
  forking.
- **iOS 17+ tunnel maturity in `idevice`** — recheck before Phase 3.
- **Where camera lives** — host can capture via Mac webcam, but if we want
  *iPhone* camera, the agent has to expose it. AVFoundation in the agent is
  fine but counts against the runner's lifetime.
- **Background lifetime** — the XCTest runner is not a long-lived daemon
  in Apple's model. The host has to be willing to relaunch it on death.
  Acceptable for v1; design the relaunch loop carefully so we don't churn
  a device that's locked or asleep.
- **Windows host support** — `usbmuxd` is part of Apple Mobile Device
  Support on Windows; need to verify `idevice` works against it cleanly.
- **Notarization / Gatekeeper** for the host binary on macOS — same flow
  as the existing `mac/` desktop client, presumably already solved.

---

## 15. Why Not Just Wrap mobilecli?

We could. As a subprocess it's even legally clean for the open-source
side. But:

- Two languages (Rust + Go) at runtime; two release pipelines.
- Mobilecli's design intent is "general mobile testing CLI", not "screenmcp
  phone client" — its update cadence and surface area aren't ours to
  control.
- The hard part of porting is not the host code (a few thousand lines of
  Rust); it's the on-device agent. We need our own agent anyway for
  branding/trust/version control. Once we have an agent, owning the
  ~3000 lines of Rust around `idevice` is a small marginal cost and
  removes an external dependency.
- AGPL exposure for screenmcp-cloud, even if we believe subprocess use is
  safe, is an ongoing legal worry. Our own Rust + permissive agent is
  worry-free.

So: wrap mobilecli as a 1-week proof-of-concept to de-risk the *product*
question ("do users actually want iPhone control?"). Replace with native
Rust once the answer is yes.

---

## 16. References

- mobile-next/mobilecli — Go-based reference implementation we're copying
  the architecture from.
- mobile-next/devicekit-ios — XCTest runner agent; potential fork base.
- mobile-next/mobilewright — Playwright-style TS API on top of mobilecli;
  shape inspiration if we ever expose a higher-level SDK.
- danielpaulus/go-ios — the canonical Go implementation of lockdown,
  RemoteXPC, tunnel.
- jkcoxson/idevice — the canonical pure-Rust equivalent. **This is our
  load-bearing dependency.**
- appium/WebDriverAgent — the long-running XCTest-based agent that
  proved this whole pattern works.
- libimobiledevice — the C reference; useful when `idevice` docs are thin.
- Apple's XCTest / XCUITest documentation — the only formal source of
  truth for what the runner can and can't do.
