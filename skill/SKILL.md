# ScreenMCP — Phone & Desktop Control

Control Android phones and desktop computers remotely. Take screenshots, tap, type, scroll, read the UI tree, use the camera — all through MCP tools.

## Overview

- **What**: Control phones and desktops through a cloud relay
- **MCP Endpoint**: `https://screenmcp.com/mcp` (Streamable HTTP)
- **Auth**: API key (`pk_...`) as Bearer token
- **App**: ScreenMCP Android app connects the phone to the relay

---

## First-Time Setup

If no API key is configured, guide the user through setup:

### Step 1: Check for existing key

```bash
cat ~/.screenmcp/config.json 2>/dev/null
echo $SCREENMCP_API_KEY
```

If a key exists, skip to **Normal Operation**.

### Step 2: Get an API key

Tell the user:

> **To get started:**
> 1. Go to **https://screenmcp.com** and sign in with Google
> 2. Go to **Dashboard → API Keys → Create New Key**
> 3. Copy the key (starts with `pk_...`) and paste it here

### Step 3: Save the key

Once the user provides the key:

```bash
mkdir -p ~/.screenmcp && chmod 700 ~/.screenmcp
cat > ~/.screenmcp/config.json << 'EOF'
{
  "api_key": "<API_KEY>",
  "api_url": "https://screenmcp.com/mcp"
}
EOF
chmod 600 ~/.screenmcp/config.json
```

### Step 4: Connect a device

Tell the user:

> **Connect your phone:**
> 1. Download the **ScreenMCP** app from the Dashboard
> 2. Open it and enable **Open Source Server** mode
> 3. Enter your **User ID** and server URL from the Dashboard
> 4. Grant Accessibility and Screen Capture permissions when prompted
> 5. Tell me when you're done!

### Step 5: Verify connection

Call `list_devices` to confirm the phone appears. If it doesn't, ask the user to check the app is running.

---

## Normal Operation

### The Interaction Loop

Phone control follows an **observe → decide → act → verify** cycle:

1. **Observe** — take a `screenshot` and/or call `ui_tree` to see current screen state
2. **Decide** — identify what to tap/type/scroll based on what you see
3. **Act** — execute the action (`click`, `type`, `scroll`, etc.)
4. **Verify** — take another `screenshot` to confirm it worked

Always screenshot before AND after acting. Never tap blind.

### Using ui_tree for Precision

Instead of guessing coordinates from screenshots, call `ui_tree` to get the accessibility tree with exact element bounds:

```
Node: { className: "Button", text: "Save", bounds: {left: 400, top: 1150, right: 680, bottom: 1250} }
→ Click center: x = (400+680)/2 = 540, y = (1150+1250)/2 = 1200
→ click(device_id=1, x=540, y=1200)
```

Use `ui_tree` when you need exact coordinates. Use `screenshot` when you need to see visual layout or show the user what's happening.

---

## Available MCP Tools

Every phone/desktop tool requires a `device_id` parameter (integer, starts at 1). Call `list_devices` first.

### Device Management

| Tool | Purpose | Parameters |
|------|---------|------------|
| `list_devices` | List registered devices | — |

### Screen & UI

| Tool | Purpose | Parameters |
|------|---------|------------|
| `screenshot` | Capture screen (base64 WebP) | `quality?`, `max_width?`, `max_height?` |
| `ui_tree` | Accessibility tree with bounds, text, roles | — |

### Touch & Gestures

| Tool | Purpose | Parameters |
|------|---------|------------|
| `click` | Tap at coordinates | `x`, `y`, `duration?` |
| `long_click` | Long press (1000ms) | `x`, `y` |
| `scroll` | Finger-drag scroll | `x`, `y`, `dx`, `dy` |
| `drag` | Drag from A to B | `startX`, `startY`, `endX`, `endY`, `duration?` |

### Text

| Tool | Purpose | Parameters |
|------|---------|------------|
| `type` | Type text into focused field | `text` |
| `get_text` | Get text from focused field | — |
| `select_all` | Select all text | — |
| `copy` | Copy selection | `return_text?` |
| `paste` | Paste (optionally set text first) | `text?` |
| `get_clipboard` | Get clipboard text | — |
| `set_clipboard` | Set clipboard text | `text` |

### Navigation

| Tool | Purpose | Parameters |
|------|---------|------------|
| `back` | Press Back | — |
| `home` | Press Home | — |
| `recents` | Open recent apps | — |

### Camera

| Tool | Purpose | Parameters |
|------|---------|------------|
| `list_cameras` | List cameras (IDs + facing) | — |
| `camera` | Take a photo | `camera?`, `quality?` |

### Keyboard (Desktop Only)

| Tool | Purpose | Parameters |
|------|---------|------------|
| `hold_key` | Hold a key | `key` |
| `release_key` | Release a key | `key` |
| `press_key` | Press and release a key | `key` |
| `right_click` | Right-click | `x`, `y` |
| `middle_click` | Middle-click | `x`, `y` |
| `mouse_scroll` | Raw mouse scroll | `x`, `y`, `dx`, `dy` |

### Audio

| Tool | Purpose | Parameters |
|------|---------|------------|
| `play_audio` | Play audio on device speaker | `audio_data` (base64 WAV/MP3), `volume?` |

---

## Coordinate System

- Pixels from top-left (0, 0)
- Get resolution from `screenshot` response
- Typical phone (1080x2400): status bar y < 100, nav bar y > 2300, center (540, 1200)

### Common Scroll Patterns

```
Scroll down:   scroll(device_id=1, x=540, y=1200, dx=0, dy=-500)
Scroll up:     scroll(device_id=1, x=540, y=1200, dx=0, dy=500)
```

Or use drag for more control:

```
Scroll down:          drag(device_id=1, startX=540, startY=1800, endX=540, endY=600)
Pull notifications:   drag(device_id=1, startX=540, startY=50, endX=540, endY=1000)
```

---

## Multi-Device

1. Call `list_devices` to see all connected devices
2. Each device has a `device_id` number (1, 2, 3...)
3. Pass the correct `device_id` to every tool call
4. If only one device, always use `device_id: 1`

---

## Error Handling

| Error | Action |
|-------|--------|
| 401 Unauthorized | API key is invalid. Ask user to check their key at screenmcp.com. |
| Device not found | Wrong device_id. Call `list_devices`. |
| Command timeout | Phone disconnected. Ask user to open the ScreenMCP app. |
| Screenshot is black | Screen is off. Try `back` to wake it. |
| `unsupported: true` | Command not supported on this device type (phone vs desktop). |

---

## Use Case: "Speak to User" via ScreenMCP + OpenAI TTS

Use this flow when the assistant should talk out loud through the connected phone/desktop speaker.

### 1) Generate speech audio with OpenAI TTS

Use OpenAI TTS API and save as WAV or MP3.

Example (WAV):

```bash
export OPENAI_API_KEY="<YOUR_OPENAI_KEY>"

curl -sS https://api.openai.com/v1/audio/speech \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model":"gpt-4o-mini-tts",
    "voice":"alloy",
    "format":"wav",
    "input":"Your package was delivered. Please check the door."
  }' \
  --output /tmp/screenmcp_tts.wav
```

### 2) Send the audio to device with `play_audio`

Encode the generated file to base64 and call `play_audio`:

```bash
AUDIO_B64=$(base64 -w 0 /tmp/screenmcp_tts.wav)
mcporter call screenmcp.play_audio device_id:1 audio_data:$AUDIO_B64 volume:0.8 --output json
```

### 3) Verify + fallback

- If `play_audio` succeeds but user hears nothing: retry with `volume:1.0`.
- If device is offline/timeouts: call `list_devices`, confirm `connected=true`, then retry.
- Keep messages short for fast response and lower audio latency.

### Security

- Do **not** hardcode API keys in repo files.
- Keep keys in env vars or local secret files only.
- If OpenAI key is needed, read from `OPENAI_API_KEY` at runtime.

### Multi-device fallback for voice delivery

When `play_audio` fails on one device:

1. Call `list_devices`.
2. Build a candidate order:
   - Prefer `connected=true` devices first.
   - Keep original target first if still connected.
3. Retry `play_audio` on the next candidate device.
4. Stop after first success and report which `device_id` worked.
5. If all fail, return a clear error summary with per-device failure reasons.

## Rules

- **Always screenshot before acting** — never tap blind
- **Always screenshot after acting** — verify the result
- **Use ui_tree for precision** — read exact coordinates from the accessibility tree
- **Never expose the API key** — only store in config files
- **Pause between actions** — wait 300-500ms between sequential operations
- **Ask before destructive actions** — deleting, sending messages, purchases, calls
- **Privacy** — don't read or describe personal messages unless the user explicitly asks
- **Desktop keyboard combos** — use hold_key → press_key → release_key for shortcuts like Ctrl+C
