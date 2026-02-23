# Guide: Adding a New Command to ScreenMCP

This guide lists every file that needs to change when adding a new device command.

## Architecture Overview

```
Controller/SDK → MCP Server → Worker (relay) → Device (Android/Windows/Mac/Linux)
                                                    ↓
                                              Execute command
                                                    ↓
                                              Return result
```

- **Binary data** flows as base64-encoded strings inside JSON messages over WebSocket
- **Worker** is a generic JSON relay — it does NOT need changes for new commands
- **Clients** (Android, desktop) detect format from bytes, not MIME types

## Files to Change (by component)

### 1. Android App — Command Execution

| File | Purpose |
|------|---------|
| `android/app/.../ScreenMcpService.kt` | Add implementation method (e.g. `playAudio()`, `captureCamera()`) |
| `android/app/.../WebSocketClient.kt` | Add case in `when (cmd)` dispatch block to extract params and call the service method |

**Pattern**: WebSocketClient receives JSON `{cmd, params}`, dispatches to ScreenMcpService method, sends response back via `sendResponse(ws, id, status, result, error)`.

### 2. Windows Desktop Client — Command Execution

| File | Purpose |
|------|---------|
| `windows/src/commands.rs` | Add `"new_cmd" => handle_new_cmd(params)` in match + implement handler function |
| `windows/Cargo.toml` | Add crate dependencies if needed (e.g. `rodio` for audio) |

**Pattern**: `execute_command()` matches command name string → calls handler → handler returns `Result<Value, String>` → wrapped in `{status, result/error}` JSON.

### 3. Mac Desktop Client — Command Execution

| File | Purpose |
|------|---------|
| `mac/src/commands.rs` | Add match case. Implement or return `{"status":"ok","unsupported":true}` |

### 4. Linux Desktop Client — Command Execution

| File | Purpose |
|------|---------|
| `linux/src/commands.rs` | Add match case. Implement or return `{"status":"ok","unsupported":true}` |

### 5. Worker (Rust WebSocket Relay) — NO CHANGES NEEDED

The worker is a generic message relay. It does not inspect command names or params. New commands flow through automatically.

### 6. MCP Server — Open Source (TypeScript)

| File | Purpose |
|------|---------|
| `mcp-server/src/mcp.ts` | Add tool object to `phoneTools` array with name, description, inputSchema (zod), and handler |

**Pattern**: Each tool has `{name, description, inputSchema: {device_id, ...params}, handler: async (phone, params) => ...}`. Handler calls `phone.sendCommand(name, params)`.

### 7. MCP Server — Cloud (Rust)

| File | Purpose |
|------|---------|
| `screenmcp-cloud/mcp-server/src/tools.rs` | Add `ToolDef` to `all_tools()` vec with name, description, and JSON Schema `input_schema` |

**Pattern**: `ToolDef { name, description, input_schema: json!({type: "object", properties: {...}, required: [...]}) }`.

### 8. TypeScript SDK

| File | Purpose |
|------|---------|
| `sdk/typescript/src/client.ts` | Add async method to `ScreenMCPClient` class |
| `sdk/typescript/src/types.ts` | Add result type interface if command returns structured data |

### 9. Python SDK

| File | Purpose |
|------|---------|
| `sdk/python/src/screenmcp/client.py` | Add async method to `ScreenMCPClient` class |
| `sdk/python/src/screenmcp/types.py` | Add dataclass if command returns structured data |

### 10. Rust SDK

| File | Purpose |
|------|---------|
| `sdk/rust/src/client.rs` | Add async method to `ScreenMCPClient` impl |
| `sdk/rust/src/types.rs` | Add struct if command returns structured data |

### 11. Cloud Web Playground

| File | Purpose |
|------|---------|
| `screenmcp-cloud/web/src/app/playground/page.tsx` | Add to `CommandType` union, add to command group, add mock response, add state vars, add UI inputs, add to `buildParams()` |

### 12. Documentation

| File | Purpose |
|------|---------|
| `commands.md` | Add command spec (params, types, defaults, response format) |
| `wire-protocol.md` | Add wire message examples |
| `implementations.md` | Add row showing platform support |

### 13. Remote CLI (optional)

| File | Purpose |
|------|---------|
| `remote/src/` | Add command to REPL if interactive mode lists commands |

## Checklist Template

```
[ ] Android: ScreenMcpService.kt — implement method
[ ] Android: WebSocketClient.kt — add dispatch case
[ ] Windows: commands.rs — add match + handler
[ ] Mac: commands.rs — add match (implement or unsupported stub)
[ ] Linux: commands.rs — add match (implement or unsupported stub)
[ ] MCP Server (TS): mcp.ts — add tool definition
[ ] MCP Server (Rust): tools.rs — add ToolDef
[ ] SDK TypeScript: client.ts — add method
[ ] SDK Python: client.py — add method
[ ] SDK Rust: client.rs — add method
[ ] Playground: page.tsx — add command UI
[ ] Docs: commands.md, wire-protocol.md, implementations.md
```
