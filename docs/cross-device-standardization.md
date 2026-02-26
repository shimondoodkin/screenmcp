# Cross-Device Standardization Analysis

Comprehensive analysis of all ScreenMCP device clients, documenting current behavior
and proposing standardization. Based on source code review of all four clients,
the worker relay, and both SDKs.

**Clients analyzed:**
- Android (`/home/user/screenmcp/android/`) -- Kotlin, AccessibilityService
- Windows (`/home/user/screenmcp/windows/`) -- Rust, Win32 APIs
- Linux (`/home/user/screenmcp/linux/`) -- Rust, wmctrl/xdotool
- Mac (`/home/user/screenmcp/mac/`) -- Rust, CoreGraphics/Accessibility

**Supporting components:**
- Worker (`/home/user/screenmcp/worker/`) -- Rust WebSocket relay
- TypeScript SDK (`/home/user/screenmcp/sdk/typescript/`)
- Python SDK (`/home/user/screenmcp/sdk/python/`)

---

## 1. Error Response Formats

### Protocol Specification (from wire-protocol.md)

The canonical error format is:

```json
{ "id": 7, "status": "error", "error": "message string" }
```

### Current State by Client

#### Android (WebSocketClient.kt)

Uses a `sendResponse` helper function (line 537-548):

```kotlin
private fun sendResponse(ws, id, status, result = null, error = null) {
    val response = JSONObject().apply {
        put("id", id)
        put("status", status)
        if (result != null) put("result", result)
        if (error != null) put("error", error)
    }
    ws.send(response.toString())
}
```

**Findings:**
- COMPLIANT with protocol: always includes `id`, `status`, and conditionally `error`.
- Error strings are human-readable: `"phone is locked"`, `"accessibility service not connected"`,
  `"no focused input field"`, `"gesture cancelled"`, `"screenshot failed: <code>"`,
  `"missing x param"`, `"unknown command: <cmd>"`.
- For `hold_key`/`release_key`/`press_key`: does NOT handle these commands, falls through
  to the `else` branch returning `"unknown command: hold_key"` (status `"error"`).
  This differs from the implementations.md matrix which says Android returns "error" for keyboard commands.
- For `right_click`/`middle_click`/`mouse_scroll`: returns `{"status": "ok", "unsupported": true}`
  via `sendResponse(ws, id, "ok", JSONObject().put("unsupported", true))`.
  The `unsupported` flag is inside `result`, not at the top level.

**BUG: `unsupported` placement.** Android puts `unsupported` inside `result`:
```json
{ "id": 7, "status": "ok", "result": { "unsupported": true } }
```
But the wire protocol spec says it should be at the top level:
```json
{ "id": 7, "status": "ok", "unsupported": true }
```

#### Windows (commands.rs)

Uses a central `execute_command` function (line 22-75):

```rust
pub fn execute_command(id, cmd, params, config) -> Value {
    let result = match cmd {
        // ... handlers return Result<Value, String>
        _ => return json!({"id": id, "status": "error", "error": format!("unknown command: {cmd}")})
    };
    match result {
        Ok(v) => json!({"id": id, "status": "ok", "result": v}),
        Err(e) => json!({"id": id, "status": "error", "error": e}),
    }
}
```

**Findings:**
- COMPLIANT with protocol for normal commands.
- Error strings use descriptive format: `"failed to list screens: <e>"`, `"no screens found"`,
  `"missing params"`, `"missing x"`, `"click failed: <e>"`, etc.
- `play_audio`: Fully implemented with rodio. Returns errors like `"unsupported audio format"`.
- **BUG: `play_audio` on Linux/Mac returns early with malformed response.**
  In `linux/src/commands.rs` line 50 and `mac/src/commands.rs` line 50:
  ```rust
  "play_audio" => return json!({"status": "ok", "unsupported": true}),
  ```
  This **skips the id field** because it returns before the `match result` block that
  adds `id`. The response is `{"status": "ok", "unsupported": true}` -- missing `"id"`.

#### Linux (commands.rs)

Identical `execute_command` pattern to Windows.

**Findings:**
- Same error format as Windows (compliant).
- `play_audio` returns `{"status": "ok", "unsupported": true}` -- **MISSING `id` FIELD**.
- `handle_ui_tree()` uses wmctrl/xdotool. If neither is installed, returns:
  ```json
  { "tree": [], "note": "install wmctrl or xdotool for window listing" }
  ```
  The extra `note` field is non-standard.

#### Mac (commands.rs)

Identical `execute_command` pattern to Windows.

**Findings:**
- Same error format as Windows (compliant).
- `play_audio` returns `{"status": "ok", "unsupported": true}` -- **MISSING `id` FIELD**.
- `handle_ui_tree()` fallback (non-macOS target) returns:
  ```json
  { "tree": [], "note": "ui_tree requires macOS" }
  ```
  Extra `note` field is non-standard.

### Error Format Inconsistencies Summary

| Issue | Client(s) | Severity |
|-------|-----------|----------|
| `play_audio` response missing `id` field | Linux, Mac | HIGH |
| `unsupported` flag inside `result` vs. top-level | Android vs. spec | MEDIUM |
| Extra `note` field in ui_tree responses | Linux, Mac, Windows (fallback) | LOW |
| Error string formats vary between clients | All | LOW |

---

## 2. UI Tree Format

### Protocol Specification (from commands.md)

Two different schemas are documented:

**Android node fields:** `className`, `resourceId`, `text`, `contentDescription`, `bounds` (`{left, top, right, bottom}`), `clickable`, `editable`, `focused`, `scrollable`, `checkable`, `checked`, `children`.

**Desktop node fields:** `title`, `x`, `y`, `width`, `height`.

### Current State by Client

#### Android (ScreenMcpService.kt, dumpNodeJson method, lines 257-289)

Returns a recursive tree matching the documented Android format:

```json
{
  "className": "FrameLayout",
  "text": "",
  "contentDescription": "",
  "resourceId": "",
  "clickable": false,
  "editable": false,
  "focused": false,
  "scrollable": false,
  "checkable": false,
  "checked": false,
  "bounds": { "left": 0, "top": 0, "right": 1080, "bottom": 1920 },
  "children": [...]
}
```

**Characteristics:**
- Always includes all fields (even empty strings for text/resourceId).
- Bounds format: `{ left, top, right, bottom }` (rectangle edges).
- Recursive nested tree structure.
- Fields match the documented spec exactly.

#### Windows (commands.rs, handle_ui_tree, lines 694-959)

Full UIAutomation accessibility tree with rich properties. Uses sparse JSON (only non-default values):

```json
{
  "text": "Document - Google Chrome",
  "controlType": "Window",
  "className": "Chrome_WidgetWin_1",
  "resourceId": "MainWindow",
  "contentDescription": "tooltip help text",
  "bounds": [[100, 50], [1200, 800]],
  "clickable": true,
  "editable": true,
  "scrollable": true,
  "checked": false,
  "focused": true,
  "hwnd": 12345678,
  "value": "current input value",
  "enabled": false,
  "children": [...]
}
```

**Key differences from Android:**
- **Bounds format differs:** `[[x, y], [width, height]]` instead of `{left, top, right, bottom}`.
- **Sparse output:** Only includes non-default values (empty strings omitted).
- **Extra fields:** `controlType`, `value`, `enabled`, `hwnd` -- not in Android schema.
- **Field mapping:** `text` = UIA Name, `resourceId` = UIA AutomationId, `contentDescription` = UIA HelpText.
- Has full recursive tree like Android (walks UIAutomation ControlView).
- Includes z-order occlusion culling and viewport filtering.

#### Linux (commands.rs, handle_ui_tree, lines 593-702)

Window-level only (no in-window element tree):

```json
{
  "title": "Terminal",
  "x": 100,
  "y": 50,
  "width": 800,
  "height": 600,
  "windowId": "0x04800003"
}
```

**Key differences:**
- **Flat list, not a tree** -- returns `{ "tree": [window, window, ...] }`.
- **No element-level data** -- only window title and geometry.
- **Extra field:** `windowId` (X11 window ID string).
- Falls back to xdotool if wmctrl is not available.
- If neither tool is available, returns `{ "tree": [], "note": "..." }`.

#### Mac (commands.rs, handle_ui_tree, lines 607-768)

Window-level only (no in-window element tree), using CGWindowListCopyWindowInfo:

```json
{
  "title": "Firefox - Google",
  "app": "Firefox",
  "windowName": "Google",
  "x": 0,
  "y": 25,
  "width": 1440,
  "height": 875,
  "windowId": 42
}
```

**Key differences from Linux:**
- **Flat list, not a tree** -- same as Linux.
- **Extra fields:** `app` (owner name), `windowName` (window-specific title), `windowId` (integer).
- Title is formatted as `"<app> - <windowName>"` or just `"<app>"`.
- Filters out non-layer-0 windows (menu bar items, etc.).

#### Windows Non-Windows Fallback (commands.rs, line 1013-1017)

```rust
Ok(json!({ "tree": [], "note": "ui_tree is best supported on Windows" }))
```

### UI Tree Inconsistencies Summary

| Aspect | Android | Windows | Linux | Mac |
|--------|---------|---------|-------|-----|
| **Tree depth** | Full recursive | Full recursive | Window-level only | Window-level only |
| **Bounds format** | `{left,top,right,bottom}` | `[[x,y],[w,h]]` | `x,y,width,height` (flat) | `x,y,width,height` (flat) |
| **Node identity** | `className`, `resourceId` | `controlType`, `className`, `resourceId` | `title`, `windowId` | `title`, `app`, `windowId` |
| **Interaction flags** | `clickable`, `editable`, `focused`, `scrollable`, `checkable`, `checked` | Same + `enabled`, `value` | None | None |
| **Extra fields** | None | `hwnd`, `value`, `controlType`, `enabled` | `windowId` | `app`, `windowName`, `windowId` |
| **Empty values** | Always included | Sparse (omitted) | N/A | N/A |
| **Text field** | `text` | `text` (mapped from UIA Name) | `title` | `title` |

---

## 3. Online/Offline/Availability States

### Android (WebSocketClient.kt)

Status strings reported via `onStatusChange` callback:
- `"Discovering worker..."` -- calling /api/discover
- `"Connecting to <url>..."` -- WebSocket handshake in progress
- `"Connected"` -- auth_ok received
- `"Disconnected"` -- clean disconnect or connection lost
- `"Disconnected (max retries)"` -- gave up after MAX_RECONNECT_ATTEMPTS (5)
- `"Connection failed: <message>"` -- WebSocket onFailure
- `"Discovery failed"` -- HTTP error from /api/discover
- `"Discovery error"` -- exception during discovery
- `"No worker available"` -- discovery returned no wsUrl
- `"Auth failed"` -- received auth_fail message
- `"Reconnecting in Xs (attempt N/5)..."` -- scheduled reconnect

Additional state: `isPhoneLocked()` -- detected via KeyguardManager, returned as
`"phone is locked"` error on screenshot commands.

### Desktop Clients (Windows/Linux/Mac ws.rs)

All three use an identical `ConnectionStatus` enum:
```rust
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error(String),
}
```

These states are shared with the system tray UI via a `watch::Sender<ConnectionStatus>`.

No lock detection equivalent exists for desktop clients.

### Worker (ws.rs)

The worker tracks phone connectivity per device:
- `phone_status` message: `{ "type": "phone_status", "connected": true/false }`
- Sent to controllers when a phone connects/disconnects.
- Controller auth_ok includes `phone_connected` boolean.

Heartbeat: server pings every 30s, disconnects after 60s no pong.

### SDK-side States

**TypeScript SDK events:** `connected`, `disconnected`, `error`, `phone_status`, `reconnecting`, `reconnected`.

**Python SDK:** Same event model (async).

### State Inconsistencies

| Aspect | Android | Desktop Clients | Worker |
|--------|---------|-----------------|--------|
| State type | Free-form strings | Enum (5 values) | Binary connected/not |
| Lock detection | Yes (`isPhoneLocked`) | No | N/A |
| Screen off detection | No | No | N/A |
| Granular reconnect info | Yes (attempt #, delay) | No (just "Reconnecting") | N/A |
| State reported to worker | Implicit (connected/not) | Implicit (connected/not) | Relayed to controllers |

---

## 4. Wait/Timeout/Retry Behavior

### Android (WebSocketClient.kt)

| Parameter | Value | Source |
|-----------|-------|--------|
| Max reconnect delay | 30,000 ms | `MAX_RECONNECT_DELAY_MS` |
| Max reconnect attempts | 5 | `MAX_RECONNECT_ATTEMPTS` |
| Idle timeout | 5 minutes (300,000 ms) | `IDLE_TIMEOUT_MS` |
| Reconnect backoff | Exponential: 1s, 2s, 4s, 8s, 16s, 30s | `1000 * (1 << min(attempt, 5))` |
| OkHttp ping interval | 30 seconds | `pingInterval(30, TimeUnit.SECONDS)` |
| Auth timeout | None (implicit in WS handshake) | N/A |
| Command timeout | None (no client-side timeout) | N/A |
| Connection generation counter | Yes | Prevents stale callback races |

**Reconnect strategy:** Exponential backoff with 5 max attempts, then gives up permanently.
If `apiUrl` is set, re-discovers worker on each reconnect. Idle timer disconnects after
5 minutes of no activity (no commands, no pings).

### Windows (ws.rs)

| Parameter | Value | Source |
|-----------|-------|--------|
| Auth timeout | 10 seconds | `Duration::from_secs(10)` |
| Heartbeat interval | 30 seconds | `interval(Duration::from_secs(30))` |
| Pong timeout | 90 seconds | `last_pong.elapsed() > Duration::from_secs(90)` |
| Reconnect delay (discovery fail) | 5 seconds | `maybe_schedule_reconnect(tx, 5)` |
| Reconnect delay (auth fail) | 10 seconds | `maybe_schedule_reconnect(tx, 10)` |
| Reconnect delay (connection lost) | 3 seconds | `maybe_schedule_reconnect(tx, 3)` |
| Max reconnect attempts | Unlimited | No limit |
| Idle timeout | None | N/A |
| SSE-driven connections | No auto-reconnect | SSE sends new ConnectToWorker |

**Reconnect strategy:** Fixed delays (not exponential), unlimited retries. When connected
via SSE (ConnectToWorker), does NOT auto-reconnect -- waits for SSE to send a new event.

### Linux (ws.rs)

| Parameter | Value | Source |
|-----------|-------|--------|
| Auth timeout | 10 seconds | Same as Windows |
| Heartbeat interval | 30 seconds | Same |
| Pong timeout | 90 seconds | Same |
| Reconnect delay | 3-10 seconds (fixed) | Same as Windows |
| Max reconnect attempts | Unlimited | Same |
| SSE backoff | 1s, 2s, 4s, ... up to 60s | Exponential in `run_sse_listener` |

**Reconnect strategy:** Same as Windows. SSE listener has its own exponential backoff
(1s to 60s) for reconnecting the SSE stream.

### Mac (ws.rs)

| Parameter | Value | Source |
|-----------|-------|--------|
| Auth timeout | 10 seconds | Same |
| Heartbeat/Pong | 30s / 90s | Same |
| Reconnect | 3-10s fixed | Same |
| SSE-driven | No auto-reconnect | Same as Windows approach |

Note: Mac's ws.rs has a separate `run_connection_to_worker` for SSE-driven connections
(different from Linux which inlines it into `run_connection`), and a fully embedded
`run_sse_listener` (not in a separate `sse.rs` file).

### Worker (ws.rs)

| Parameter | Value | Source |
|-----------|-------|--------|
| Auth timeout | 10 seconds | `AUTH_TIMEOUT` |
| Heartbeat interval | 30 seconds | `HEARTBEAT_INTERVAL` |
| Heartbeat timeout | 60 seconds | `HEARTBEAT_TIMEOUT` |
| Reconnect cooldown | 3 seconds | `Connections::RECONNECT_COOLDOWN_SECS` |
| SSE heartbeat | 30 seconds | Same interval |

### SDK Command Timeouts

| SDK | Default timeout | Configurable |
|-----|----------------|--------------|
| TypeScript | 30,000 ms | Yes (`commandTimeout` option) |
| Python | 30.0 seconds | Yes (`command_timeout` option) |
| TypeScript reconnect | 1s, 2s, 4s, 8s, 16s, 30s (6 attempts) | No |

### Timeout/Retry Inconsistencies

| Aspect | Android | Windows | Linux | Mac |
|--------|---------|---------|-------|-----|
| Reconnect backoff | Exponential (1-30s) | Fixed (3-10s) | Fixed (3-10s) | Fixed (3-10s) |
| Max reconnect attempts | 5 | Unlimited | Unlimited | Unlimited |
| Idle timeout | 5 minutes | None | None | None |
| Auth timeout | None | 10 seconds | 10 seconds | 10 seconds |
| Pong timeout | N/A (OkHttp handles) | 90 seconds | 90 seconds | 90 seconds |
| SSE reconnect | Exponential (1-60s) | N/A | Exponential (1-60s) | Exponential (1-60s) |
| SSE in separate file | SseService.kt | N/A | sse.rs | Inline in ws.rs |

---

## 5. Screenshot Image Format

| Client | Format | Quality Support | Notes |
|--------|--------|-----------------|-------|
| Android | WebP (lossy/lossless) | Yes (1-99 lossy, 100 lossless) | Full spec compliance |
| Windows | **PNG** | No (quality param ignored) | Does NOT return WebP |
| Linux | **PNG** | No (quality param ignored) | Does NOT return WebP |
| Mac | **PNG** | No (quality param ignored) | Does NOT return WebP |

Camera images on desktop clients use lossless WebP (`WebPEncoder::new_lossless`), but
screenshots use PNG. The quality parameter is accepted but silently ignored on all desktop clients.

This is a significant inconsistency: the protocol spec says screenshots return "base64 WebP"
but desktop clients return PNG.

---

## 6. `play_audio` Command

| Client | Supported | Implementation | Issues |
|--------|-----------|----------------|--------|
| Android | Yes | MediaPlayer with WAV/MP3 detection | None |
| Windows | Yes | rodio crate with WAV/MP3 detection | None |
| Linux | **Unsupported** | Returns `{"status":"ok","unsupported":true}` | Missing `id` field |
| Mac | **Unsupported** | Returns `{"status":"ok","unsupported":true}` | Missing `id` field |

---

## 7. Proposed Unified Standards

### 7.1 Error Response Format

**Standard:** All responses MUST include the `id` field. All error responses MUST follow:

```json
{ "id": <int>, "status": "error", "error": "<human-readable message>" }
```

**Unsupported command response:** Top-level `unsupported` flag (matching wire-protocol.md):

```json
{ "id": <int>, "status": "ok", "unsupported": true }
```

### 7.2 UI Tree Format

Since Android provides a full accessibility tree and Windows provides a full UIAutomation tree,
but Linux and Mac only provide window lists, the standard should define two levels:

**Level 1 (All platforms) -- Window List:**

```json
{
  "tree": [
    {
      "title": "Window Title",
      "bounds": { "left": 100, "top": 50, "right": 1300, "bottom": 850 },
      "windowId": "<platform-specific-id>",
      "children": []
    }
  ]
}
```

**Level 2 (Android, Windows) -- Element Tree:**

Each node in the tree MAY contain:

```json
{
  "text": "Button Label",
  "className": "Button",
  "resourceId": "submitBtn",
  "contentDescription": "Submit form",
  "controlType": "Button",
  "bounds": { "left": 100, "top": 200, "right": 300, "bottom": 250 },
  "clickable": true,
  "editable": false,
  "focused": false,
  "scrollable": false,
  "checkable": false,
  "checked": false,
  "children": [...]
}
```

**Key standardization decisions:**
1. **Bounds: Use `{left, top, right, bottom}` everywhere.** Windows must convert from
   `[[x,y],[w,h]]` to `{left: x, top: y, right: x+w, bottom: y+h}`.
2. **Sparse output is fine** -- omitting false booleans and empty strings is acceptable.
3. **Extra platform-specific fields are allowed** but must not be relied upon cross-platform.
   Document them as platform extensions: `hwnd` (Windows), `windowId` (Linux/Mac),
   `app` (Mac), `value` (Windows).
4. **Linux/Mac should return `x,y,width,height` as `bounds: {left,top,right,bottom}`** for
   consistency. Converting: `left=x, top=y, right=x+width, bottom=y+height`.

### 7.3 Connection States

**Standard enum for all clients:**

```
Disconnected    -- No connection, not trying to connect
Connecting      -- Discovery or WebSocket handshake in progress
Connected       -- Authenticated and ready for commands
Reconnecting    -- Connection lost, retrying
Error(<msg>)    -- Fatal error requiring user action (e.g., auth failed)
```

Android should map its free-form strings to these states. The detailed reconnect progress
(attempt number, delay) can be logged but the status enum should be one of the five above.

### 7.4 Reconnect Strategy

**Standard reconnect behavior for all clients:**

| Parameter | Standard Value |
|-----------|---------------|
| Backoff type | Exponential |
| Initial delay | 1 second |
| Max delay | 30 seconds |
| Max attempts | 10 (then give up, report Error state) |
| Auth timeout | 10 seconds |
| Pong/heartbeat timeout | 90 seconds |
| Idle timeout | None (remove from Android) |

**Rationale:**
- Exponential backoff is better than fixed delays (reduces server load during outages).
- 10 attempts provides ~3 minutes of retry time (1+2+4+8+16+30+30+30+30+30 = ~181s).
- The 5-minute idle timeout on Android causes unexpected disconnections; the server
  already has heartbeat-based disconnection.

### 7.5 Screenshot Format

**Standard:** All clients should return WebP. Desktop clients should add lossy WebP
encoding support (via libwebp or similar). If not feasible short-term, PNG is acceptable
but the response should indicate the format:

```json
{ "image": "<base64>", "format": "webp" }
```

Or: accept the PNG inconsistency and document it. SDKs should handle both formats.

---

## 8. Migration Steps

### 8.1 Fix: `play_audio` Missing `id` (Linux, Mac) -- HIGH PRIORITY

**Files:**
- `/home/user/screenmcp/linux/src/commands.rs` line 50
- `/home/user/screenmcp/mac/src/commands.rs` line 50

**Change:** Replace early return with proper pattern:
```rust
// Before:
"play_audio" => return json!({"status": "ok", "unsupported": true}),

// After:
"play_audio" => return json!({"id": id, "status": "ok", "unsupported": true}),
```

### 8.2 Fix: Android `unsupported` Flag Placement -- MEDIUM PRIORITY

**File:** `/home/user/screenmcp/android/app/src/main/java/com/doodkin/screenmcp/WebSocketClient.kt` lines 464-466

**Change:** Move `unsupported` from inside `result` to top-level:
```kotlin
// Before:
"right_click", "middle_click", "mouse_scroll" -> {
    sendResponse(ws, id, "ok", JSONObject().put("unsupported", true))
}

// After:
"right_click", "middle_click", "mouse_scroll" -> {
    val response = JSONObject().apply {
        put("id", id)
        put("status", "ok")
        put("unsupported", true)
    }
    ws.send(response.toString())
}
```

### 8.3 Standardize UI Tree Bounds Format (Windows) -- MEDIUM PRIORITY

**File:** `/home/user/screenmcp/windows/src/commands.rs` lines 872-875

**Change:** Convert bounds from `[[x,y],[w,h]]` to `{left,top,right,bottom}`:
```rust
// Before:
let bounds_json = bounds_raw.as_ref()
    .map(|r| json!([[r.left, r.top], [(r.right - r.left), (r.bottom - r.top)]]))

// After:
let bounds_json = bounds_raw.as_ref()
    .map(|r| json!({"left": r.left, "top": r.top, "right": r.right, "bottom": r.bottom}))
```

### 8.4 Standardize UI Tree Bounds Format (Linux) -- MEDIUM PRIORITY

**File:** `/home/user/screenmcp/linux/src/commands.rs` lines 621-628 and 684-690

**Change:** Convert flat fields to bounds object:
```rust
// Before:
windows.push(json!({
    "title": title,
    "x": x, "y": y, "width": width, "height": height,
    "windowId": win_id,
}));

// After:
windows.push(json!({
    "title": title,
    "bounds": { "left": x, "top": y, "right": x + width, "bottom": y + height },
    "windowId": win_id,
}));
```

### 8.5 Standardize UI Tree Bounds Format (Mac) -- MEDIUM PRIORITY

**File:** `/home/user/screenmcp/mac/src/commands.rs` lines 744-753

**Change:** Same conversion as Linux:
```rust
// Before:
windows.push(json!({
    "title": title, "app": owner_name, "windowName": window_name,
    "x": x, "y": y, "width": width, "height": height,
    "windowId": window_id,
}));

// After:
windows.push(json!({
    "title": title, "app": owner_name, "windowName": window_name,
    "bounds": { "left": x, "top": y, "right": x + width, "bottom": y + height },
    "windowId": window_id,
}));
```

### 8.6 Remove Extra `note` Fields -- LOW PRIORITY

**Files:**
- `/home/user/screenmcp/windows/src/commands.rs` line 1016 (fallback)
- `/home/user/screenmcp/linux/src/commands.rs` line 697
- `/home/user/screenmcp/mac/src/commands.rs` line 767

**Change:** Remove `"note"` from responses. Just return `{"tree": []}`.

### 8.7 Standardize Reconnect Strategy (Android) -- LOW PRIORITY

**File:** `/home/user/screenmcp/android/app/src/main/java/com/doodkin/screenmcp/WebSocketClient.kt`

**Changes:**
- Increase `MAX_RECONNECT_ATTEMPTS` from 5 to 10.
- Remove the idle timeout (`IDLE_TIMEOUT_MS` and related `resetIdleTimer`/`cancelIdleTimer`).
  The server heartbeat handles dead connections.

### 8.8 Standardize Reconnect Strategy (Desktop) -- LOW PRIORITY

**Files:**
- `/home/user/screenmcp/windows/src/ws.rs`
- `/home/user/screenmcp/linux/src/ws.rs`
- `/home/user/screenmcp/mac/src/ws.rs`

**Changes:**
- Change fixed reconnect delays to exponential backoff.
- Add a max retry counter (10 attempts) instead of unlimited.
- Add reconnect attempt counter to `ConnectionStatus::Reconnecting` variant.

### 8.9 Desktop Screenshot Format (WebP) -- LOW PRIORITY

**Files:** All three desktop `commands.rs` `handle_screenshot` functions.

**Change:** Switch from PngEncoder to WebP encoding (requires libwebp or a Rust crate
that supports lossy WebP). This is the lowest priority because it is a format/quality
issue rather than a correctness issue, and SDKs can handle both formats.

---

## 9. Summary of All Inconsistencies

### Critical (Breaking/Data Loss)

1. **Missing `id` in `play_audio` unsupported response** (Linux, Mac) -- worker/SDK cannot
   correlate the response to a pending command.

### Significant (Behavior Mismatch)

2. **Android `unsupported` flag inside `result` instead of top-level** -- SDKs that check
   for top-level `unsupported` will miss it.
3. **Windows UI tree bounds format `[[x,y],[w,h]]`** differs from Android `{left,top,right,bottom}`
   -- any cross-platform UI tree consumer must handle both.
4. **Desktop screenshots are PNG, not WebP** -- consumers expecting WebP may fail to decode.

### Minor (Non-standard but Functional)

5. **Extra `note` field** in fallback UI tree responses.
6. **Different reconnect strategies** across clients.
7. **Android idle timeout** has no desktop equivalent.
8. **Mac SSE listener is inline in ws.rs** while Linux has a separate `sse.rs` file
   (code organization, not behavior).
9. **Mac UI tree has extra `app` and `windowName` fields** -- informational, not breaking.
10. **Linux/Mac UI tree uses flat `x/y/width/height`** instead of bounds object.
