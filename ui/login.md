# Login Screen

Uniform login UI specification for all ScreenMCP clients. Each platform renders this as appropriate for its environment (Activity on Android, system tray menu on desktop), but the structure, fields, and behavior are the same.

## Layout

The login screen is a centered card with padding and rounded corners.

```
┌─────────────────────────────────┐
│           ScreenMCP             │  ← App name, large/bold
│   Connect your device to AI     │  ← Subtitle, muted
│                                 │
│  ┌───────────────────────────┐  │
│  │  Sign in with Google  [G] │  │  ← Google sign-in button (Android only)
│  └───────────────────────────┘  │
│                                 │
│  Connection mode:               │  ← Label (Android only)
│  (•) Push (FCM)                 │  ← Radio: phone wakes on demand via push
│  ( ) Always connected (SSE)     │  ← Radio: phone stays connected via SSE
│                                 │
│  ─────────── or ──────────────  │  ← Divider
│                                 │
│  [x] Open Source Server         │  ← Checkbox: toggles OSS mode
│                                 │     When checked, Google sign-in is disabled
│  User ID                        │
│  ┌───────────────────────────┐  │
│  │ local-user                │  │  ← Text field (enabled when checkbox is on)
│  └───────────────────────────┘  │
│                                 │
│  API Server URL                 │
│  ┌───────────────────────────┐  │
│  │ http://localhost:3000     │  │  ← Text field (enabled when checkbox is on)
│  └───────────────────────────┘  │
│                                 │
│  ┌───────────────────────────┐  │
│  │     Connect               │  │  ← Primary action (enabled when OSS fields
│  └───────────────────────────┘  │     are filled, or Google sign-in succeeded)
│                                 │
│  Status: Not connected          │  ← Status line, muted
└─────────────────────────────────┘
```

## After Login

Once authenticated (either via Google or Open Source), the card switches to a connected state:

```
┌─────────────────────────────────┐
│           ScreenMCP             │
│                                 │
│  Signed in as                   │
│  shimondev5@gmail.com           │  ← Google email or "Open Source (user-id)"
│                                 │
│  ┌───────────────────────────┐  │
│  │       Sign Out            │  │  ← Returns to pre-login card
│  └───────────────────────────┘  │
└─────────────────────────────────┘
```

After sign-out the pre-login card is shown again.

## Elements

### Google Sign-In Button

- **Platforms**: Android and desktop (via browser redirect).
- **Disabled when**: Open Source Server checkbox is checked.

#### Android Behavior
Opens Google credential picker in-app, obtains Firebase ID token, exchanges with Firebase Auth.

#### Desktop Behavior (Browser Redirect Flow)
Desktop clients have no webview, so they use a browser-based OAuth flow:

1. Desktop app starts a temporary HTTP server on a random port (e.g. `54321`).
2. Opens the system browser to `https://screenmcp.com/auth/desktop?port=54321`.
3. User signs in with Google on the website.
4. Website creates an API key (`pk_...`) for the user via `POST /api/keys`.
5. Website redirects to `http://localhost:54321/callback?token=pk_...`.
6. Desktop app's HTTP server receives the token, saves it to `config.toml` as `token`.
7. Desktop app closes the HTTP server and transitions to connected state.

**Website page**: `screenmcp-cloud/web/src/app/auth/desktop/page.tsx`
- Shows Google sign-in button (same Firebase popup flow as the main login page).
- If user is already signed in, creates API key and redirects immediately.
- Requires `port` query parameter; shows error if missing.
- Returns a persistent API key (not a short-lived Firebase token).

### Connection Mode (Android only)

Two radio buttons controlling how the phone receives "connect" signals:

| Mode | Description |
|------|-------------|
| **Push (FCM)** | Phone stays disconnected. On remote discovery, server sends FCM push → phone wakes and connects to worker. Disconnects after idle timeout. Battery-efficient. Default. |
| **Always connected (SSE)** | Phone opens a persistent SSE connection to the API server, listening for "connect" events. Stays connected. Uses more battery but works without Google Play Services. |

- **Platforms**: Android only. Desktop clients always use SSE (they are always-on machines).
- **Stored as**: `connection_mode` in SharedPreferences (`"fcm"` or `"sse"`).
- **Default**: `"fcm"` on Android.

### Open Source Server Checkbox

- **All platforms**.
- **When checked**:
  - Google Sign-In button becomes disabled/hidden (Android).
  - Connection mode radios are hidden (always SSE in open source mode).
  - User ID and API Server URL fields become enabled.
  - Connect button becomes the primary action.
- **When unchecked**:
  - User ID and API Server URL fields are disabled and grayed out.
  - Google Sign-In is the primary action (Android).
- **Stored as**: `opensource_server_enabled` (bool).

### User ID Field

- **Label**: "User ID"
- **Hint/Placeholder**: `local-user`
- **Description**: The `user.id` value from the self-hosted server's `worker.toml`. Used as the Bearer token for authentication.
- **Enabled when**: Open Source checkbox is checked.
- **Stored as**: `opensource_user_id` (string).
- **Desktop clients**: Editable via text field (Linux: zenity dialog fallback), or by editing `config.toml` directly.

### API Server URL Field

- **Label**: "API Server URL"
- **Hint/Placeholder**: `http://localhost:3000`
- **Description**: The URL of the self-hosted MCP server (not the worker). The client calls `/api/discover` on this URL to find the worker.
- **Enabled when**: Open Source checkbox is checked.
- **Stored as**: `opensource_api_url` (string).

### Connect Button (Open Source Mode)

- **Label**: "Connect" (or "Continue with Open Source Server" on Android for clarity).
- **Enabled when**: Open Source checkbox is checked AND both User ID and API Server URL are non-empty.
- **Behavior**: Saves settings, transitions to the main/connected screen.

### Status Line

- Displays connection feedback: "Not connected", "Connecting...", "Connected", error messages.
- Muted/secondary text color.

## Platform Rendering

### Android

Full graphical card rendered as an `Activity` with Material Design components:
- `MaterialCardView` containing all elements.
- `com.google.android.gms.common.SignInButton` or material button for Google.
- `RadioGroup` with two `RadioButton` for connection mode.
- `CheckBox` for open source toggle.
- `TextInputLayout` + `TextInputEditText` for User ID and API URL.
- `MaterialButton` for Connect/Sign Out.

### Windows / macOS / Linux (Desktop System Tray)

Desktop clients run as system tray apps. The "login" equivalent is a tray menu:

```
Tray Menu:
  ├─ Sign in with Google        ← Opens browser to screenmcp.com/auth/desktop
  ├─ ──────────
  ├─ Connect
  ├─ Disconnect
  ├─ ──────────
  ├─ Status: Connected
  ├─ Signed in as: user@gmail.com   ← Shown after Google auth
  ├─ ──────────
  ├─ Open Source Server  ▸  [submenu]
  │   ├─ [x] Enabled
  │   ├─ User ID: local-user
  │   ├─ API URL: http://localhost:3000
  │   └─ Edit Settings...        ← opens config.toml in editor
  ├─ ──────────
  ├─ Open Config File
  └─ Quit
```

- **Google sign-in**: Opens system browser to `https://screenmcp.com/auth/desktop?port=PORT`. After user authenticates, browser redirects back to localhost with an API key. The tray app saves the key to `config.toml`.
- **Connection mode** is always SSE (no radio buttons needed).
- **Open Source toggle**: Check menu item that enables/disables the submenu items. When enabled, Google sign-in is hidden.
- **Editing fields**: Click to edit inline (Linux: zenity dialog), or "Edit Settings..." opens config file.

## Storage

### Android

`SharedPreferences` file named `"screenmcp"`:

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `opensource_server_enabled` | bool | `false` | Open source mode toggle |
| `opensource_user_id` | string | `""` | Bearer token for OSS auth |
| `opensource_api_url` | string | `""` | MCP server URL |
| `connection_mode` | string | `"fcm"` | `"fcm"` or `"sse"` |
| `device_id` | string | auto | 128-bit hex, generated on first run |

### Desktop (Windows / macOS / Linux)

TOML config file at platform-specific path:

| Platform | Path |
|----------|------|
| Windows | `%LOCALAPPDATA%\screenmcp\config.toml` |
| macOS | `~/Library/Application Support/screenmcp/config.toml` |
| Linux | `~/.config/screenmcp/config.toml` |

```toml
# Cloud mode (default)
api_url = "https://screenmcp.com"
token = ""                          # API key (pk_...) for cloud auth

# Open source mode
opensource_server_enabled = false
opensource_user_id = ""
opensource_api_url = ""

# Device
device_id = "a1b2c3..."            # Auto-generated 128-bit hex
auto_connect = true
screenshot_quality = 80
```
