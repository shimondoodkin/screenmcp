# @screenmcp/sdk

TypeScript SDK for controlling Android phones remotely via [ScreenMCP](https://screenmcp.com).

## Installation

```bash
npm install @screenmcp/sdk
```

## Quick Start

```typescript
import { ScreenMCPClient } from "@screenmcp/sdk";

const phone = new ScreenMCPClient({
  apiKey: "pk_your_api_key_here",
  deviceId: "a1b2c3d4e5f67890abcdef1234567890", // target device's crypto ID
});

await phone.connect();

// Take a screenshot
const { image } = await phone.screenshot();
// image is a base64-encoded WebP string

// Tap on the screen
await phone.click(540, 1200);

// Type text
await phone.type("Hello world");

await phone.disconnect();
```

## API Reference

### Constructor

```typescript
new ScreenMCPClient(options: {
  apiKey: string;         // Required. Your API key (pk_... format).
  apiUrl?: string;        // API server URL. Defaults to https://server10.doodkin.com
  deviceId?: string;      // Target device's cryptographic ID (32 hex chars). Get from /api/devices/status.
  commandTimeout?: number; // Per-command timeout in ms. Defaults to 30000.
  autoReconnect?: boolean; // Auto-reconnect on disconnect. Defaults to true.
})
```

### Connection

```typescript
await phone.connect();    // Discover worker and open WebSocket
await phone.disconnect(); // Close connection
```

### Commands

| Method | Description |
|--------|-------------|
| `screenshot()` | Returns `{ image: string }` (base64 WebP) |
| `click(x, y)` | Tap at screen coordinates |
| `longClick(x, y)` | Long-press at coordinates |
| `drag(startX, startY, endX, endY)` | Drag gesture |
| `scroll(direction, amount?)` | Scroll `"up"`, `"down"`, `"left"`, or `"right"` |
| `type(text)` | Type text into the focused input |
| `getText()` | Returns `{ text: string }` from focused element |
| `selectAll()` | Select all text |
| `copy()` | Copy selection to clipboard |
| `paste()` | Paste from clipboard |
| `back()` | Press Back button |
| `home()` | Press Home button |
| `recents()` | Open app switcher |
| `uiTree()` | Returns `{ tree: any[] }` accessibility tree |
| `camera(facing?)` | Returns `{ image: string }`. Facing: `"front"` or `"rear"` |
| `sendCommand(cmd, params?)` | Send any command (for future/custom commands) |

### Events

```typescript
phone.on("connected", () => { /* WebSocket connected */ });
phone.on("disconnected", () => { /* WebSocket closed */ });
phone.on("error", (err: Error) => { /* connection or protocol error */ });
phone.on("phone_status", (online: boolean) => { /* phone came online/offline */ });
phone.on("reconnecting", () => { /* attempting reconnect */ });
phone.on("reconnected", (workerUrl: string) => { /* reconnected successfully */ });
```

### Properties

```typescript
phone.connected       // boolean - is WebSocket connected
phone.phoneConnected  // boolean - is the phone online
phone.workerUrl       // string | null - current worker URL
```

## Example: Save a Screenshot to Disk

```typescript
import { ScreenMCPClient } from "@screenmcp/sdk";
import { writeFileSync } from "fs";

const phone = new ScreenMCPClient({ apiKey: "pk_..." });
await phone.connect();

const { image } = await phone.screenshot();
writeFileSync("screenshot.webp", Buffer.from(image, "base64"));

await phone.disconnect();
```

## Example: Monitor Phone Connection

```typescript
const phone = new ScreenMCPClient({ apiKey: "pk_..." });

phone.on("phone_status", (online) => {
  console.log(`Phone is ${online ? "online" : "offline"}`);
});

phone.on("error", (err) => {
  console.error("Connection error:", err.message);
});

await phone.connect();
```

## License

MIT
