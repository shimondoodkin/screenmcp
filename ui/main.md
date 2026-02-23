# Main Screen

The main screen is shown after login. It displays connection status and provides device controls for testing.

## Layout

```
┌─────────────────────────────────┐
│  shimondev5@gmail.com  [Sign Out]│  ← User info + sign out
│                                 │
│  Service: Connected       [green]│  ← Accessibility service status
│  [Open Accessibility Settings]  │  ← Opens OS accessibility settings
│                                 │
│  API URL (http://10.0.2.2:3000) │  ← Text field, hint shows default
│                                 │     Empty = https://screenmcp.com
│  Phone registered         [green]│  ← Registration status
│  Worker: Connected        [green]│  ← Worker connection status
│                                 │
│  [  Connect  ] [ Disconnect ]   │  ← Manual connect/disconnect
│                                 │
│  ─── Screenshot ────────────── │
│  [  Take Screenshot  ]         │
│  ┌───────────────────────────┐ │
│  │                           │ │  ← Screenshot preview
│  │       (preview)           │ │
│  │                           │ │
│  └───────────────────────────┘ │
│                                 │
│  ─── Click / Tap ──────────── │
│  [ X ] [ Y ]  [Click at (X,Y)]│
│                                 │
│  ─── Drag / Swipe ────────── │
│  [SX][SY]  [EX][EY]  [Drag]   │
│                                 │
│  ─── Type ─────────────────── │
│  [ Text to type     ] [Type]   │
│  [Get Text]                     │
│                                 │
│  ─── Clipboard ────────────── │
│  [Select All] [Copy] [Paste]   │
│                                 │
│  ─── Navigation ───────────── │
│  [Back] [Home] [Recents]       │
│                                 │
│  ─── UI Tree ──────────────── │
│  [Get UI Tree]                  │
│  (tree output text)             │
│                                 │
│  ─── Log ──────────────────── │
│  [08:12:33] Connected           │  ← Scrolling log
│  [08:12:35] Screenshot saved    │
└─────────────────────────────────┘
```

## Status Indicators

Color-coded backgrounds:

| Status | Color |
|--------|-------|
| Connected / Registered | Green (`#C8E6C9`) |
| Connecting / Checking | Yellow (`#FFF9C4`) |
| Disconnected / Error | Red (`#FFCDD2`) |

## Sections

### User Info Bar
- Shows email (cloud) or "Open Source Mode (user-id)" (OSS).
- Sign Out button returns to login screen.

### Service Status
- **Service: Connected/Disconnected**: Whether the Accessibility Service (Android) is running.
- Button opens OS accessibility settings to enable it.
- Desktop clients: not applicable (no accessibility service needed).

### Connection Controls
- **API URL**: Override the cloud API URL. Empty defaults to `https://screenmcp.com`. Disabled in open source mode (pre-filled from config).
- **Phone registered**: Shown in cloud mode. Indicates device is registered with the cloud backend.
- **Worker status**: WebSocket connection status to the relay worker.
- **Connect/Disconnect**: Manual control for testing. In production, FCM handles on-demand connection.

### Device Controls
Testing section for verifying commands work. Each button sends a command through the full pipeline (app → worker → accessibility service → result).

## Platform Notes

- **Android**: Full scrollable Activity with all sections.
- **Desktop**: These controls are not in the tray menu. Desktop clients only show Connect/Disconnect/Status in the tray. The device control testing happens via the SDK CLI or MCP tools.
