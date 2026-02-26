# play_audio Feature — Research Report

## Overview

New command `play_audio` lets MCP upload a WAV or MP3 file (base64-encoded) and play it on the device speaker.

## Binary Data Flow

**Existing pattern** (screenshots/camera): Device → base64 in JSON → WebSocket → Worker → Client
**play_audio** (reverse): Client → base64 in JSON → WebSocket → Worker → Device → decode → MediaPlayer

All binary data flows as base64 strings in JSON. No raw binary WebSocket frames.

## Files Requiring Changes

### Android App (HIGH — actual audio playback)
| File | Change |
|------|--------|
| `android/app/.../ScreenMcpService.kt` | Add `playAudio()` method using Android `MediaPlayer` API. Decode base64 → temp file → play. |

### Windows Desktop Client (HIGH — implement playback)
| File | Change |
|------|--------|
| `windows/src/commands.rs` | Add `"play_audio" => handle_play_audio(params)`. Decode base64 → temp file → play via Windows audio API. |

### Mac/Linux Desktop Clients (LOW — unsupported stub)
| File | Change |
|------|--------|
| `mac/src/commands.rs` | Add match case → `{status: "ok", unsupported: true}` |
| `linux/src/commands.rs` | Add match case → `{status: "ok", unsupported: true}` |

### Worker (NO CHANGES)
Worker is a generic JSON relay — no command-specific logic.

### MCP Server — Open Source (HIGH)
| File | Change |
|------|--------|
| `mcp-server/src/mcp.ts` | Add `play_audio` tool definition with `audio_data` (base64 string) and optional `volume` (0-1) params |

### MCP Server — Cloud (HIGH)
| File | Change |
|------|--------|
| `screenmcp-cloud/mcp-server/src/tools.rs` | Add `play_audio` ToolDef with JSON schema |

### Cloud Web Playground (MEDIUM)
| File | Change |
|------|--------|
| `screenmcp-cloud/web/src/app/playground/page.tsx` | Add `play_audio` to CommandType, file upload input (WAV/MP3), convert to base64, volume slider |

### TypeScript SDK (HIGH)
| File | Change |
|------|--------|
| `sdk/typescript/src/client.ts` | Add `playAudio(audioBase64: string, volume?: number)` method |
| `sdk/typescript/src/types.ts` | Add `PlayAudioResult` if needed |

### Python SDK (HIGH)
| File | Change |
|------|--------|
| `sdk/python/src/screenmcp/client.py` | Add `play_audio(audio_base64, volume=None)` method |

### Rust SDK (HIGH)
| File | Change |
|------|--------|
| `sdk/rust/src/client.rs` | Add `play_audio(audio_base64, volume)` method |

### Documentation (MEDIUM)
| File | Change |
|------|--------|
| `commands.md` | Add play_audio command spec |
| `wire-protocol.md` | Add play_audio message examples |
| `implementations.md` | Add row to support matrix |

### Remote CLI (MEDIUM)
| File | Change |
|------|--------|
| `remote/src/` | Add play_audio command if interactive commands are listed |

## Command Specification

```json
{
  "cmd": "play_audio",
  "params": {
    "audio_data": "<base64-encoded WAV or MP3>",
    "volume": 0.8
  }
}
```

**Parameters:**
- `audio_data` (string, required) — Base64-encoded audio file (WAV or MP3)
- `volume` (number, optional, 0.0-1.0, default 1.0) — Playback volume

**Response:**
```json
{
  "status": "ok",
  "result": {}
}
```

**Error:**
```json
{
  "status": "error",
  "error": "Unsupported audio format"
}
```

## Implementation Order

1. Android `ScreenMcpService.kt` — actual playback
2. Windows `commands.rs` — actual playback
3. MCP servers (TS + Rust) — tool definitions
4. SDKs (TS, Python, Rust) — client methods
5. Playground — file upload UI
6. Mac/Linux — unsupported stubs
7. Documentation
