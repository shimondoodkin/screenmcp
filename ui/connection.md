# Connection Behavior

How and when devices connect to the worker, and how they handle disconnects. This describes the target behavior for all clients (Android, desktop).

## Lifecycle Overview

```
         ┌─────────┐
         │  Idle    │  ← App is running but not connected to worker
         └────┬─────┘
              │  "connect" signal received (FCM push or SSE event)
              ▼
         ┌─────────┐
         │Connecting│  ← Opening WebSocket to worker
         └────┬─────┘
              │  WebSocket open + auth accepted
              ▼
         ┌─────────┐
         │Connected │  ← Executing commands from controller
         └────┬─────┘
              │  No commands for <inactivity timeout>
              ▼
         ┌─────────┐
         │  Idle    │  ← WebSocket closed, back to waiting
         └─────────┘
```

## On-Demand Connection

Devices do **not** maintain a persistent WebSocket to the worker. Instead they connect only when an AI controller needs them.

### Trigger: Remote Discovery

1. Controller calls `POST /api/discover {device_id}` on the MCP server.
2. MCP server looks up the target device and sends a "connect" signal:
   - **FCM mode (Android default)**: Server sends an FCM data message to the device's FCM token. The phone wakes up (even from Doze) and receives the push.
   - **SSE mode (Android opt-in, all desktop clients)**: Device has an open SSE connection to `GET /api/events`. Server pushes `{type: "connect", wsUrl, target_device_id}` over that stream.
3. Device receives the signal, verifies `target_device_id` matches its own, and opens a WebSocket to the worker at `wsUrl`.
4. Worker authenticates the device (Bearer token from TOML config or cloud auth).
5. Device is now connected. Controller's discover call returns the worker URL and the controller connects too.

### FCM Wake-Up (Android)

FCM is the default connection mode on Android because it is battery-efficient — the phone does not need to hold any connection open while idle.

- The app registers an FCM token on login and sends it to the API server.
- When a "connect" push arrives, the `FirebaseMessagingService` wakes the app and triggers the WebSocket connection sequence.
- If FCM delivery fails (e.g., phone offline, Play Services unavailable), the controller's discover call will time out. The controller can retry or the user can manually connect from the app.

### SSE Fallback

SSE mode keeps a lightweight HTTP connection to the API server, listening for connect events. Used when:
- User selects "Always connected (SSE)" on Android (e.g., no Google Play Services).
- Desktop clients (always SSE — desktops are always-on, no push infrastructure needed).
- Open Source mode (no Firebase, SSE is the only option).

## Inactivity Disconnect

After connecting, if no commands are received for the **inactivity timeout**, the device closes the WebSocket and returns to idle.

| Setting | Value |
|---------|-------|
| Inactivity timeout | 5 minutes (300s) |

- The timer resets on every incoming command.
- The timer resets on every outgoing response.
- When the timer fires, the device sends a WebSocket close frame and transitions to idle.
- The controller receives the close and knows the device went idle. Next discover call will wake it again.

## Disconnect and Retry

### Connection Drop (unexpected close)

If the WebSocket connection drops unexpectedly (network error, worker restart, TCP timeout):

1. Device detects the close (error or missing pong).
2. Device waits **3 seconds** (cooldown to avoid reconnect storms).
3. Device attempts **one** reconnect to the same worker URL.
4. If the reconnect succeeds → resume connected state, reset inactivity timer.
5. If the reconnect fails → transition to idle. Do **not** retry again. The next discover call from a controller will trigger a fresh connection.

### Network Change

When the device detects a network change (Wi-Fi ↔ cellular, new Wi-Fi network, VPN toggle):

1. If currently connected, the existing WebSocket is likely broken.
2. Device waits **3 seconds** for the new network to stabilize.
3. Device attempts **one** reconnect to the same worker URL.
4. If the reconnect succeeds → resume connected state.
5. If the reconnect fails → transition to idle.

### Why Only One Retry

Aggressive reconnect loops cause problems:
- Battery drain on phones.
- Thundering herd on worker after a restart.
- Stale connections piling up.

One retry handles the common transient cases (brief network blip, worker rolling restart). Anything worse than that is resolved by the next on-demand discover call, which is the authoritative "please connect now" signal.

## Heartbeat

The worker sends WebSocket ping frames to detect dead connections.

| Setting | Value |
|---------|-------|
| Ping interval | 30 seconds |
| Pong deadline | 60 seconds (2 missed pings) |

- If the device does not respond with a pong within the deadline, the worker drops the connection.
- If the device does not receive a ping within the deadline, the device considers the connection dead and follows the disconnect-and-retry flow above.

## Connection Generation Counter (Android)

The Android WebSocket client maintains a monotonically increasing **generation counter** to prevent stale callbacks from interfering with current connections.

- Incremented on every new connection attempt.
- All async callbacks (onMessage, onClose, onFailure) check whether their generation matches the current one.
- If a callback's generation is stale (from a previous connection), it is ignored.
- This prevents scenarios where a late `onClose` from connection N triggers a reconnect that races with connection N+1.

## Duplicate Connection Handling (Worker)

If a device connects while it already has an active connection (same `device_id`):

- The worker **replaces** the old connection — drops the old WebSocket channel silently.
- The new connection becomes authoritative.
- This avoids "device already connected" rejections and handles cases where the device reconnected but the worker hasn't detected the old connection's death yet.

## Summary Table

| Event | Action | Retries |
|-------|--------|---------|
| Discover signal (FCM/SSE) | Connect to worker | — |
| Command received | Reset inactivity timer | — |
| Inactivity timeout (5 min) | Disconnect, go idle | 0 |
| Connection drop | Wait 3s, reconnect | 1 |
| Network change | Wait 3s, reconnect | 1 |
| Reconnect fails | Go idle, wait for next discover | 0 |
| Missed heartbeat (60s) | Treat as drop, follow retry flow | 1 |
