# Model-based Screenshot Sizing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an MCP connection declare its consumer model via `?model=claude|gemini|chatgpt`; the device then auto-sizes screenshots — and scales click coordinates — to that model's vision limits, without the agent passing any size parameter.

**Architecture:** The MCP server reads `model` from the connection URL and injects it (when the caller gave no `max_width`/`max_height`) into every coordinate-bearing command it relays. Each device client routes BOTH its screenshot-output sizing and its input-coordinate scaling through one shared `providerDefaultSize(model, screenW, screenH)` helper, so the image the model sees and the coordinate space its clicks land in always match. The worker is unchanged.

**Tech Stack:** TypeScript (Node `node:test`) for the open-source MCP server; Rust (`cargo test`) for the cloud MCP server and the Windows/Mac/Linux desktop clients; Kotlin (JUnit4) for Android; Next.js/React for the cloud web dashboard.

**Spec:** `screenmcp/docs/superpowers/specs/2026-05-30-model-provider-screenshot-sizing-design.md`

---

## Canonical sizing rules (single source of truth)

Pure function of the device's real screen `(w, h)` → target `(maxW, maxH)`. Aspect-preserving, downscale-only. The ONLY constants are each provider's documented caps.

```
claude(w, h):
    maxPixels = 1_176_000 ; maxEdge = 1568
    s  = min(1, maxEdge / max(w,h), sqrt(maxPixels / (w*h)))
    mw = floor(w*s) ; mh = floor(h*s)
    while mw*mh > maxPixels and (mw>1 or mh>1): shrink the larger of mw,mh by 1
    return (mw, mh)

gemini(w, h):
    shortCap = 1080 ; longCap = 1920
    if w >= h:  s = min(1, longCap/w, shortCap/h)     # landscape
    else:       s = min(1, longCap/h, shortCap/w)     # portrait
    return (round(w*s), round(h*s))

chatgpt(w, h):
    short = min(w,h)
    s = min(1, 768/short)
    if max(w,h)*s > 2048: s = 2048 / max(w,h)
    return (round16(w*s), round16(h*s))

round16(x) = max(16, round(x/16)*16)
unknown model → return nothing (caller falls back to existing default)
```

### Canonical test-vector table (every platform asserts against THIS table)

| screen W×H | claude | gemini | chatgpt |
|---|---|---|---|
| 2560×1440 | 1445×813 | 1920×1080 | 1360×768 |
| 1920×1080 | 1445×813 | 1920×1080 | 1360×768 |
| 3840×2160 | 1445×813 | 1920×1080 | 1360×768 |
| 1080×2400 | 705×1568 | 864×1920 | 768×1712 |
| 1440×3120 | 723×1568 | 886×1920 | 768×1664 |
| 1080×3000 | 564×1568 | 691×1920 | 736×2048 |
| 1000×1000 | 1000×1000 | 1000×1000 | 768×768 |
| 640×480 | 640×480 | 640×480 | 640×480 |

`COORD_TOOLS` (the command names the server injects `model` into):
`screenshot`, `screenshot_region`, `screenshot_window`, `ui_tree`, `click`, `long_click`, `drag`, `scroll`, `double_click`, `right_click`, `middle_click`, `mouse_move`, `mouse_scroll`.

---

## Phases (each independently shippable / its own PR)

1. Open-source MCP server (TypeScript) — parse `?model=`, inject into `COORD_TOOLS`.
2. Cloud MCP server (Rust) — query param → session → inject into `COORD_TOOLS`.
3. Windows desktop client — `provider_sizing` helper + route all 5 scale sites through it.
4. Mac desktop client — same.
5. Linux desktop client — same.
6. Android client — Kotlin helper + route screenshot + `scaleX/scaleY` + `getOutputScale`.
7. Cloud web dashboard — model dropdown that appends `?model=` to the copyable URL.
8. Docs — `model-sizing.md` + `commands.md`.
9. (Optional) SDK `model` pass-through.

Phases 3/4/5 share identical helper code (separate Rust crates, matching the existing windows/mac/linux duplication). All phases' tests assert the one canonical table above.

---

## Phase 1 — Open-source MCP server (TypeScript)

### Task 1.1: Add a test runner

**Files:**
- Modify: `screenmcp/mcp-server/package.json`

- [ ] **Step 1: Ensure `tsx` is a dev dependency (needed for `node --import tsx`).**

Run: `cd screenmcp/mcp-server && npm install -D tsx`

- [ ] **Step 2: Add a `test` script using the built-in Node test runner with an EXPLICIT file path (avoids glob-expansion differences across Node versions / PowerShell).**

In `package.json`, change the `scripts` block (currently lines 5–9):

```json
"scripts": {
  "dev": "npx tsx src/server.ts",
  "build": "npx tsc",
  "start": "node dist/server.js",
  "test": "node --import tsx --test src/model.test.ts"
}
```

- [ ] **Step 3: Verify the runner works once a test exists (it will fail to find the file now; that's expected until Task 1.2).**

Run: `cd screenmcp/mcp-server && npm install`
Expected: install succeeds. (`npm test` is exercised in Task 1.2.)

- [ ] **Step 3: Commit**

```bash
git add screenmcp/mcp-server/package.json
git commit -m "test: add node:test runner to open-source mcp-server"
```

### Task 1.2: `model` resolution + injection helpers (pure, tested)

**Files:**
- Create: `screenmcp/mcp-server/src/model.ts`
- Test: `screenmcp/mcp-server/src/model.test.ts`

- [ ] **Step 1: Write the failing test**

Create `screenmcp/mcp-server/src/model.test.ts`:

```typescript
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resolveModel, applyModelDefault, COORD_TOOLS } from './model.ts';

test('resolveModel accepts known providers, rejects everything else', () => {
  assert.equal(resolveModel('claude'), 'claude');
  assert.equal(resolveModel('gemini'), 'gemini');
  assert.equal(resolveModel('chatgpt'), 'chatgpt');
  assert.equal(resolveModel('gpt-5'), null);
  assert.equal(resolveModel(null), null);
  assert.equal(resolveModel(''), null);
});

test('COORD_TOOLS covers screenshot family and pointer commands', () => {
  for (const name of ['screenshot', 'screenshot_region', 'screenshot_window', 'ui_tree',
                      'click', 'long_click', 'drag', 'scroll', 'double_click',
                      'right_click', 'middle_click', 'mouse_move', 'mouse_scroll']) {
    assert.ok(COORD_TOOLS.has(name), `${name} should be in COORD_TOOLS`);
  }
  assert.equal(COORD_TOOLS.has('type'), false);
});

test('applyModelDefault injects model only for coord tools without explicit size', () => {
  assert.deepEqual(applyModelDefault('click', { x: 1 }, 'gemini'), { x: 1, model: 'gemini' });
  // explicit size present → leave untouched
  assert.deepEqual(applyModelDefault('screenshot', { max_width: 800 }, 'gemini'), { max_width: 800 });
  assert.deepEqual(applyModelDefault('screenshot', { max_height: 600 }, 'gemini'), { max_height: 600 });
  // non-coord tool → untouched
  assert.deepEqual(applyModelDefault('type', { text: 'hi' }, 'gemini'), { text: 'hi' });
  // no model → untouched
  assert.deepEqual(applyModelDefault('click', { x: 1 }, null), { x: 1 });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd screenmcp/mcp-server && npm test`
Expected: FAIL — `Cannot find module './model.ts'`.

- [ ] **Step 3: Write minimal implementation**

Create `screenmcp/mcp-server/src/model.ts`:

```typescript
export type ModelProvider = 'claude' | 'gemini' | 'chatgpt';

export const COORD_TOOLS = new Set<string>([
  'screenshot', 'screenshot_region', 'screenshot_window', 'ui_tree',
  'click', 'long_click', 'drag', 'scroll', 'double_click',
  'right_click', 'middle_click', 'mouse_move', 'mouse_scroll',
]);

export function resolveModel(raw: string | null | undefined): ModelProvider | null {
  return raw === 'claude' || raw === 'gemini' || raw === 'chatgpt' ? raw : null;
}

/**
 * For a coordinate-bearing command with no explicit max_width/max_height, inject the
 * connection's model so the device can pick a provider-tuned default size. Returns a new
 * params object; never mutates the input.
 */
export function applyModelDefault(
  toolName: string,
  params: Record<string, unknown>,
  model: ModelProvider | null,
): Record<string, unknown> {
  if (
    model &&
    COORD_TOOLS.has(toolName) &&
    params.max_width == null &&
    params.max_height == null
  ) {
    return { ...params, model };
  }
  return params;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd screenmcp/mcp-server && npm test`
Expected: PASS — all three tests green.

- [ ] **Step 5: Commit**

```bash
git add screenmcp/mcp-server/src/model.ts screenmcp/mcp-server/src/model.test.ts
git commit -m "feat: model resolution + injection helpers for mcp-server"
```

### Task 1.3: Wire model into the per-request tool registration

**Files:**
- Modify: `screenmcp/mcp-server/src/mcp.ts` (handler at 464–571; tool defs 22–35, 36–69, 410–441)

- [ ] **Step 1: Import the helpers and the request-URL parsing.**

At the top of `mcp.ts`, add to the existing imports:

```typescript
import { resolveModel, applyModelDefault } from './model.ts';
```

- [ ] **Step 2: Document `model` on the four screenshot-family tool schemas.**

Add this line to the `inputSchema` of `screenshot` (after `max_height`, line ~33), `screenshot_window` (after its `max_height`, line ~419), and inside `scalingParams` is NOT where it goes — add it directly to each tool. For `screenshot`:

```typescript
    max_height: z.number().optional().describe('Max height for scaling'),
    model: z.enum(['claude', 'gemini', 'chatgpt']).optional()
      .describe('Consumer model; sets a provider-tuned default screenshot size when max_width/max_height are omitted. Normally supplied by the connection ?model= param.'),
```

For `screenshot_region` and `screenshot_window`, add the same `model: z.enum([...]).optional().describe(...)` line into their `inputSchema`. For `ui_tree`, add the same line into its `inputSchema` (alongside `...scalingParams`).

- [ ] **Step 3: Resolve `model` from the request URL inside the handler.**

In `createMcpHandler`'s returned function, immediately after the token check passes (after line 483, before `const server = new McpServer`), add:

```typescript
    // Per-connection consumer model from ?model= on the MCP URL
    const reqUrl = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`);
    const model = resolveModel(reqUrl.searchParams.get('model'));
```

- [ ] **Step 4: Inject into coordinate-bearing commands in the registration wrapper.**

In the `for (const tool of phoneTools)` loop, change the body that builds `phoneParams` (line 546–547) from:

```typescript
            const { device_id: _, ...phoneParams } = params;
            const result = await tool.handler(p, phoneParams);
```

to:

```typescript
            const { device_id: _, ...rest } = params;
            const phoneParams = applyModelDefault(tool.name, rest, model);
            const result = await tool.handler(p, phoneParams);
```

- [ ] **Step 5: Type-check.**

Run: `cd screenmcp/mcp-server && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 6: Add an integration-ish unit test for the wrapper logic via the pure helper (already covered) + a smoke test that the URL parse extracts the param.**

Append to `screenmcp/mcp-server/src/model.test.ts`:

```typescript
test('model is read from a URL query string', () => {
  const u = new URL('http://localhost:3000/api/mcp?model=chatgpt');
  assert.equal(resolveModel(u.searchParams.get('model')), 'chatgpt');
  const u2 = new URL('http://localhost:3000/api/mcp');
  assert.equal(resolveModel(u2.searchParams.get('model')), null);
});
```

Run: `cd screenmcp/mcp-server && npm test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add screenmcp/mcp-server/src/mcp.ts screenmcp/mcp-server/src/model.test.ts
git commit -m "feat: inject connection model into coord commands (open-source mcp-server)"
```

---

## Phase 2 — Cloud MCP server (Rust)

### Task 2.1: `model` injection helper (pure, tested)

**Files:**
- Modify: `screenmcp-cloud/mcp-server/src/mcp.rs`

- [ ] **Step 1: Write the failing test.**

At the bottom of `mcp.rs`, add:

```rust
#[cfg(test)]
mod model_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn injects_model_for_coord_tools_without_explicit_size() {
        let mut p = json!({ "x": 1 }).as_object().unwrap().clone();
        inject_model("click", &mut p, Some("gemini"));
        assert_eq!(p.get("model").and_then(|v| v.as_str()), Some("gemini"));
    }

    #[test]
    fn skips_when_explicit_size_present() {
        let mut p = json!({ "max_width": 800 }).as_object().unwrap().clone();
        inject_model("screenshot", &mut p, Some("gemini"));
        assert!(p.get("model").is_none());
    }

    #[test]
    fn skips_non_coord_tools_and_empty_model() {
        let mut p = json!({ "text": "hi" }).as_object().unwrap().clone();
        inject_model("type", &mut p, Some("gemini"));
        assert!(p.get("model").is_none());

        let mut p2 = json!({ "x": 1 }).as_object().unwrap().clone();
        inject_model("click", &mut p2, None);
        assert!(p2.get("model").is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails.**

Run: `cd screenmcp-cloud/mcp-server && cargo test`
Expected: FAIL — `cannot find function inject_model`.

- [ ] **Step 3: Write minimal implementation.**

Near the top of `mcp.rs` (module scope), add:

```rust
use serde_json::Value;

/// Command names that carry coordinates and therefore need the connection's model so the
/// device can pick a consistent provider-tuned size.
const COORD_TOOLS: &[&str] = &[
    "screenshot", "screenshot_region", "screenshot_window", "ui_tree",
    "click", "long_click", "drag", "scroll", "double_click",
    "right_click", "middle_click", "mouse_move", "mouse_scroll",
];

/// Inject the connection's model into a coordinate-bearing command when the caller gave no
/// explicit max_width/max_height. No-op otherwise.
fn inject_model(tool_name: &str, params: &mut serde_json::Map<String, Value>, model: Option<&str>) {
    let model = match model {
        Some(m) if !m.is_empty() => m,
        _ => return,
    };
    if !COORD_TOOLS.contains(&tool_name) {
        return;
    }
    if params.contains_key("max_width") || params.contains_key("max_height") {
        return;
    }
    params.insert("model".to_string(), Value::String(model.to_string()));
}
```

- [ ] **Step 4: Run test to verify it passes.**

Run: `cd screenmcp-cloud/mcp-server && cargo test`
Expected: PASS — 3 tests green.

- [ ] **Step 5: Commit**

```bash
git add screenmcp-cloud/mcp-server/src/mcp.rs
git commit -m "feat: inject_model helper for cloud mcp-server"
```

### Task 2.2: Store `model` on the session and read the query param

**Files:**
- Modify: `screenmcp-cloud/mcp-server/src/main.rs` (route at 47–50, handler 68–82)
- Modify: `screenmcp-cloud/mcp-server/src/mcp.rs` (`McpSession` 17–26, `handle_mcp_request` 125–191, dispatch 204–217, tools/call 328–342)

- [ ] **Step 1: Add a `model` field to the session struct.**

In `mcp.rs`, change `McpSession` (lines 17–26) to add:

```rust
struct McpSession {
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    firebase_uid: String,
    /// Consumer model from the connection ?model= param (validated to a known provider).
    model: Option<String>,
    last_active: Instant,
}
```

- [ ] **Step 2: Accept the query param in the axum handler and forward it.**

In `main.rs`, change `mcp_post` (lines 68–82) to extract the query map and pass it through:

```rust
use axum::extract::Query;
use std::collections::HashMap;

async fn mcp_post(
    State(state): State<Arc<McpState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string());

    let model = query.get("model").map(|s| s.as_str()).and_then(validate_model);
    mcp::handle_mcp_request(state, headers, client_ip, body, model).await
}
```

- [ ] **Step 3: Add the `validate_model` helper (shared, public) in `mcp.rs`.**

In `mcp.rs`, add at module scope:

```rust
/// Returns the model string only if it is a known provider, else None.
pub fn validate_model(raw: &str) -> Option<String> {
    matches!(raw, "claude" | "gemini" | "chatgpt").then(|| raw.to_string())
}
```

And in `main.rs` import it: `use mcp::validate_model;` (adjust to the existing module path, e.g. `use crate::mcp::validate_model;`).

- [ ] **Step 4: Thread `model` into `handle_mcp_request` and store it on new sessions.**

Change the signature (line 125):

```rust
pub async fn handle_mcp_request(
    state: Arc<McpState>,
    headers: HeaderMap,
    client_ip: String,
    body: String,
    query_model: Option<String>,
) -> Response<Body> {
```

In the session-creation branches (lines 151–191), set `model: query_model.clone()` in BOTH `McpSession { ... }` constructors (the "unknown session ID, create new" branch and the "no session header" branch). Example for the no-header branch:

```rust
        sessions.insert(
            new_id.clone(),
            McpSession {
                api_key: token.to_string(),
                firebase_uid: auth_result.firebase_uid.clone(),
                model: query_model.clone(),
                last_active: Instant::now(),
            },
        );
```

- [ ] **Step 5: Read the session's model before dispatch and pass it to `tools/call`.**

After `session_id` is resolved and before the dispatch `match` (line ~203), add:

```rust
    let session_model = {
        let sessions = state.sessions.lock().await;
        sessions.get(&session_id).and_then(|s| s.model.clone())
    };
```

Change the `tools/call` arm (line 207) to pass it:

```rust
    "tools/call" => handle_tools_call(state.clone(), &token, session_model.as_deref(), rpc_req.id, rpc_req.params).await,
```

- [ ] **Step 6: Inject the model into the relayed command in `handle_tools_call`.**

Update `handle_tools_call`'s signature to accept `model: Option<&str>` and, where it builds `phone_params` (lines 328–342, right after `phone_params.remove("device_id");`), add:

```rust
    inject_model(&tool_name, &mut phone_params, model);
```

(`phone_params` is already a `serde_json::Map<String, Value>` at that point.)

- [ ] **Step 7: Build to verify everything compiles.**

Run: `cd screenmcp-cloud/mcp-server && cargo build`
Expected: compiles (warnings OK).

- [ ] **Step 8: Add a session-model unit test.**

In `mcp.rs` `model_tests` module, append:

```rust
    #[test]
    fn validate_model_filters_unknown() {
        assert_eq!(validate_model("claude").as_deref(), Some("claude"));
        assert_eq!(validate_model("gemini").as_deref(), Some("gemini"));
        assert_eq!(validate_model("chatgpt").as_deref(), Some("chatgpt"));
        assert_eq!(validate_model("gpt-5"), None);
        assert_eq!(validate_model(""), None);
    }
```

Run: `cd screenmcp-cloud/mcp-server && cargo test`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add screenmcp-cloud/mcp-server/src/main.rs screenmcp-cloud/mcp-server/src/mcp.rs
git commit -m "feat: read ?model= and inject into relayed coord commands (cloud mcp-server)"
```

### Task 2.3: Document `model` on the cloud tool schemas

**Files:**
- Modify: `screenmcp-cloud/mcp-server/src/tools.rs` (screenshot 22–35, ui_tree 36–69, screenshot_window 466–480, screenshot_region 481–500)

- [ ] **Step 1: Add a `model` property to the four screenshot-family input schemas.**

In each of the four `input_schema: json!({...})` blocks, add inside `"properties"`:

```rust
                "model": { "type": "string", "enum": ["claude", "gemini", "chatgpt"], "description": "Consumer model; sets a provider-tuned default screenshot size when max_width/max_height are omitted. Normally supplied by the connection ?model= param." }
```

- [ ] **Step 2: Build to verify the JSON macros compile.**

Run: `cd screenmcp-cloud/mcp-server && cargo build`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add screenmcp-cloud/mcp-server/src/tools.rs
git commit -m "docs: advertise model param on cloud screenshot tool schemas"
```

---

## Phase 3 — Windows desktop client

### Task 3.1: `provider_sizing` module with the canonical rules (tested)

**Files:**
- Create: `screenmcp/windows/src/provider_sizing.rs`
- Modify: `screenmcp/windows/src/main.rs` (add `mod provider_sizing;`)

- [ ] **Step 1: Write the failing test.**

Create `screenmcp/windows/src/provider_sizing.rs`:

```rust
//! Provider-tuned default screenshot size, computed from the real screen dimensions.
//! See screenmcp/docs/model-sizing.md. Shared verbatim across windows/mac/linux.

/// Returns (max_width, max_height) for the given model, or None for an unknown model.
pub fn provider_default_size(model: &str, w: u32, h: u32) -> Option<(u32, u32)> {
    let (wf, hf) = (w as f64, h as f64);
    match model {
        "claude" => {
            let max_pixels: f64 = 1_176_000.0;
            let max_edge: f64 = 1568.0;
            let s = 1.0_f64
                .min(max_edge / wf.max(hf))
                .min((max_pixels / (wf * hf)).sqrt());
            let mut mw = (wf * s).floor() as u32;
            let mut mh = (hf * s).floor() as u32;
            while (mw as u64) * (mh as u64) > max_pixels as u64 && (mw > 1 || mh > 1) {
                if mw >= mh { mw -= 1; } else { mh -= 1; }
            }
            Some((mw, mh))
        }
        "gemini" => {
            let (short_cap, long_cap) = (1080.0_f64, 1920.0_f64);
            let s = if w >= h {
                1.0_f64.min(long_cap / wf).min(short_cap / hf)
            } else {
                1.0_f64.min(long_cap / hf).min(short_cap / wf)
            };
            Some(((wf * s).round() as u32, (hf * s).round() as u32))
        }
        "chatgpt" => {
            let short = wf.min(hf);
            let mut s = 1.0_f64.min(768.0 / short);
            let long = wf.max(hf);
            if long * s > 2048.0 {
                s = 2048.0 / long;
            }
            Some((round16(wf * s), round16(hf * s)))
        }
        _ => None,
    }
}

fn round16(x: f64) -> u32 {
    (((x / 16.0).round() as i64) * 16).max(16) as u32
}

#[cfg(test)]
mod tests {
    use super::provider_default_size;

    // Canonical vectors — identical across all clients.
    const VECTORS: &[(u32, u32, (u32, u32), (u32, u32), (u32, u32))] = &[
        // (w, h, claude, gemini, chatgpt)
        (2560, 1440, (1445, 813), (1920, 1080), (1360, 768)),
        (1920, 1080, (1445, 813), (1920, 1080), (1360, 768)),
        (3840, 2160, (1445, 813), (1920, 1080), (1360, 768)),
        (1080, 2400, (705, 1568), (864, 1920), (768, 1712)),
        (1440, 3120, (723, 1568), (886, 1920), (768, 1664)),
        (1080, 3000, (564, 1568), (691, 1920), (736, 2048)),
        (1000, 1000, (1000, 1000), (1000, 1000), (768, 768)),
        (640, 480, (640, 480), (640, 480), (640, 480)),
    ];

    #[test]
    fn matches_canonical_table() {
        for &(w, h, c, g, o) in VECTORS {
            assert_eq!(provider_default_size("claude", w, h), Some(c), "claude {w}x{h}");
            assert_eq!(provider_default_size("gemini", w, h), Some(g), "gemini {w}x{h}");
            assert_eq!(provider_default_size("chatgpt", w, h), Some(o), "chatgpt {w}x{h}");
        }
    }

    #[test]
    fn claude_never_exceeds_caps() {
        for &(w, h, _, _, _) in VECTORS {
            let (mw, mh) = provider_default_size("claude", w, h).unwrap();
            assert!((mw as u64) * (mh as u64) <= 1_176_000, "{w}x{h} -> {mw}x{mh}");
            assert!(mw.max(mh) <= 1568, "{w}x{h} long edge");
        }
    }

    #[test]
    fn chatgpt_outputs_multiples_of_16() {
        for &(w, h, _, _, _) in VECTORS {
            let (mw, mh) = provider_default_size("chatgpt", w, h).unwrap();
            assert_eq!(mw % 16, 0, "{w}x{h} width not /16");
            assert_eq!(mh % 16, 0, "{w}x{h} height not /16");
        }
    }

    #[test]
    fn unknown_model_returns_none() {
        assert_eq!(provider_default_size("gpt-5", 1920, 1080), None);
    }
}
```

- [ ] **Step 2: Register the module.**

In `screenmcp/windows/src/main.rs`, add near the other `mod` declarations:

```rust
mod provider_sizing;
```

- [ ] **Step 3: Run test to verify it passes.**

Run: `cd screenmcp/windows && cargo test provider_sizing`
Expected: PASS — 4 tests green. (If `matches_canonical_table` fails, the helper math diverged from the table — fix the helper, not the table.)

- [ ] **Step 4: Commit**

```bash
git add screenmcp/windows/src/provider_sizing.rs screenmcp/windows/src/main.rs
git commit -m "feat: provider_sizing rules for windows client"
```

### Task 3.2: Route all scale sites through one resolver

**Files:**
- Modify: `screenmcp/windows/src/commands.rs` (screenshot 87–174, region 194–199, window 1312–1323, `get_output_scale` 381–401, `scale_xy` 410–431, constants 407–408)

- [ ] **Step 1: Add a `resolve_scale_dims` helper that factors in model.**

In `commands.rs`, near `get_output_scale`, add:

```rust
use crate::provider_sizing::provider_default_size;

/// Effective (max_width, max_height) for scaling. Precedence:
/// explicit params > model-based provider default > config > legacy constant.
/// Returns f64 with the existing "<= 0 disables" convention.
fn resolve_scale_dims(params: Option<&Value>, config: &Config) -> (f64, f64) {
    let pw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64());
    let ph = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64());
    if pw.is_some() || ph.is_some() {
        return (
            pw.or(config.max_screenshot_width.map(|v| v as f64)).unwrap_or(DEFAULT_SCALE_WIDTH),
            ph.or(config.max_screenshot_height.map(|v| v as f64)).unwrap_or(DEFAULT_SCALE_HEIGHT),
        );
    }
    if let Some(model) = params.and_then(|p| p.get("model")).and_then(|v| v.as_str()) {
        if let Ok((sw, sh)) = get_screen_dimensions() {
            if let Some((mw, mh)) = provider_default_size(model, sw, sh) {
                return (mw as f64, mh as f64);
            }
        }
    }
    (
        config.max_screenshot_width.map(|v| v as f64).unwrap_or(DEFAULT_SCALE_WIDTH),
        config.max_screenshot_height.map(|v| v as f64).unwrap_or(DEFAULT_SCALE_HEIGHT),
    )
}
```

- [ ] **Step 2: Use it in `handle_screenshot`.**

Replace the `let max_w = ...` / `let max_h = ...` block (lines ~106–120) with:

```rust
    let (mw_f, mh_f) = resolve_scale_dims(params, config);
    let max_w = if mw_f > 0.0 { Some(mw_f as u32) } else { None };
    let max_h = if mh_f > 0.0 { Some(mh_f as u32) } else { None };
```

(The existing resize `if let (Some(mw), Some(mh)) = (max_w, max_h)` block below is unchanged.)

- [ ] **Step 3: Use it in `handle_screenshot_region`.**

Replace the `let mw = ...` / `let mh = ...` block (lines ~194–199) with:

```rust
    let (mw, mh) = resolve_scale_dims(Some(p), config);
```

- [ ] **Step 4: Use it in `handle_screenshot_window`.**

Replace the `let max_w = ...` / `let max_h = ...` block (lines ~1312–1323) with:

```rust
    let (mw_f, mh_f) = resolve_scale_dims(Some(p), config);
    let max_w = if mw_f > 0.0 { Some(mw_f as u32) } else { None };
    let max_h = if mh_f > 0.0 { Some(mh_f as u32) } else { None };
```

- [ ] **Step 5: Use it in `get_output_scale` and `scale_xy`.**

In `get_output_scale` (381–401), replace the `let mw = ...` / `let mh = ...` lines with `let (mw, mh) = resolve_scale_dims(params, config);`. Do the same in `scale_xy` (410–431). Leave the rest of each function (the `if mw > 0.0 || mh > 0.0` math) unchanged.

- [ ] **Step 6: Build and run the full crate tests.**

Run: `cd screenmcp/windows && cargo build && cargo test`
Expected: compiles; `provider_sizing` tests still PASS.

- [ ] **Step 7: Commit**

```bash
git add screenmcp/windows/src/commands.rs
git commit -m "feat: route windows screenshot + coord scaling through model-aware resolver"
```

---

## Phase 4 — Mac desktop client

### Task 4.1: `provider_sizing` module (copy + test)

**Files:**
- Create: `screenmcp/mac/src/provider_sizing.rs`
- Modify: `screenmcp/mac/src/main.rs` (add `mod provider_sizing;`)

- [ ] **Step 1: Create the file with the IDENTICAL helper and test as Windows.**

Create `screenmcp/mac/src/provider_sizing.rs` with the exact contents of `screenmcp/windows/src/provider_sizing.rs` from Task 3.1 Step 1 (same `provider_default_size`, `round16`, and the full `#[cfg(test)]` block with the canonical `VECTORS` table and all four tests).

- [ ] **Step 2: Register the module** — add `mod provider_sizing;` to `screenmcp/mac/src/main.rs`.

- [ ] **Step 3: Run test to verify it passes.**

Run: `cd screenmcp/mac && cargo test provider_sizing`
Expected: PASS — 4 tests green.

- [ ] **Step 4: Commit**

```bash
git add screenmcp/mac/src/provider_sizing.rs screenmcp/mac/src/main.rs
git commit -m "feat: provider_sizing rules for mac client"
```

### Task 4.2: Route mac scale sites through the resolver

**Files:**
- Modify: `screenmcp/mac/src/commands.rs` (screenshot 87–175, region 194–200, window params, `get_output_scale` 1240–1259, `scale_xy` 380–401, constants 377–378)

- [ ] **Step 1: Add `resolve_scale_dims` to `mac/src/commands.rs`** — identical body to Windows Task 3.2 Step 1, including `use crate::provider_sizing::provider_default_size;`. (Mac's `get_screen_dimensions`, `Config`, and `DEFAULT_SCALE_*` exist with the same signatures.)

- [ ] **Step 2: Use it in `handle_screenshot`** — replace mac's `max_w`/`max_h` or-chain (lines ~110–124) with:

```rust
    let (mw_f, mh_f) = resolve_scale_dims(params, config);
    let max_w = if mw_f > 0.0 { Some(mw_f as u32) } else { None };
    let max_h = if mh_f > 0.0 { Some(mh_f as u32) } else { None };
```

- [ ] **Step 3: Use it in `handle_screenshot_region`** — replace mac's `mw`/`mh` block (lines ~194–200) with `let (mw, mh) = resolve_scale_dims(Some(p), config);`.

- [ ] **Step 4: Use it in `handle_screenshot_window`** — replace its `max_w`/`max_h` block with the same three lines as Step 2.

- [ ] **Step 5: Use it in `get_output_scale` (1240–1259) and `scale_xy` (380–401)** — replace each `mw`/`mh` read with `let (mw, mh) = resolve_scale_dims(params, config);`.

- [ ] **Step 6: Build and test.**

Run: `cd screenmcp/mac && cargo build && cargo test`
Expected: compiles; tests PASS.

- [ ] **Step 7: Commit**

```bash
git add screenmcp/mac/src/commands.rs
git commit -m "feat: route mac screenshot + coord scaling through model-aware resolver"
```

---

## Phase 5 — Linux desktop client

### Task 5.1: `provider_sizing` module (copy + test)

**Files:**
- Create: `screenmcp/linux/src/provider_sizing.rs`
- Modify: `screenmcp/linux/src/main.rs` (add `mod provider_sizing;`)

- [ ] **Step 1: Create the file with the IDENTICAL helper and test as Windows** (Task 3.1 Step 1 contents).

- [ ] **Step 2: Register the module** — add `mod provider_sizing;` to `screenmcp/linux/src/main.rs`.

- [ ] **Step 3: Run test.**

Run: `cd screenmcp/linux && cargo test provider_sizing`
Expected: PASS — 4 tests green.

- [ ] **Step 4: Commit**

```bash
git add screenmcp/linux/src/provider_sizing.rs screenmcp/linux/src/main.rs
git commit -m "feat: provider_sizing rules for linux client"
```

### Task 5.2: Route linux scale sites through the resolver

**Files:**
- Modify: `screenmcp/linux/src/commands.rs` (screenshot 87–172, region 192–197, window params, `get_output_scale` 1277–1296, `scale_xy` 377–398, constants 374–375)

- [ ] **Step 1: Add `resolve_scale_dims`** to `linux/src/commands.rs` — identical to Windows Task 3.2 Step 1.

- [ ] **Step 2: Use it in `handle_screenshot`** — replace linux's `max_w`/`max_h` block with the three lines from Phase 3 Task 3.2 Step 2.

- [ ] **Step 3: Use it in `handle_screenshot_region`** — replace linux's `mw`/`mh` (lines ~192–197) with `let (mw, mh) = resolve_scale_dims(Some(p), config);`.

- [ ] **Step 4: Use it in `handle_screenshot_window`** — replace its `max_w`/`max_h` block with the three lines from Phase 3 Task 3.2 Step 2.

- [ ] **Step 5: Use it in `get_output_scale` (1277–1296) and `scale_xy` (377–398)** — replace each `mw`/`mh` read with `let (mw, mh) = resolve_scale_dims(params, config);`.

- [ ] **Step 6: Build and test.**

Run: `cd screenmcp/linux && cargo build && cargo test`
Expected: compiles; tests PASS.

- [ ] **Step 7: Commit**

```bash
git add screenmcp/linux/src/commands.rs
git commit -m "feat: route linux screenshot + coord scaling through model-aware resolver"
```

---

## Phase 6 — Android client

### Task 6.1: Add JVM unit-test support + the Kotlin sizing helper

**Files:**
- Modify: `screenmcp/android/app/build.gradle.kts` (deps 39–58)
- Create: `screenmcp/android/app/src/main/java/com/doodkin/screenmcp/ProviderSizing.kt`
- Test: `screenmcp/android/app/src/test/java/com/doodkin/screenmcp/ProviderSizingTest.kt`

- [ ] **Step 1: Add JUnit4 to the test classpath.**

In `app/build.gradle.kts`, inside the `dependencies { ... }` block, add:

```kotlin
    testImplementation("junit:junit:4.13.2")
```

- [ ] **Step 2: Write the failing test.**

Create `screenmcp/android/app/src/test/java/com/doodkin/screenmcp/ProviderSizingTest.kt`:

```kotlin
package com.doodkin.screenmcp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ProviderSizingTest {
    // (w, h, claude, gemini, chatgpt) — canonical vectors, identical to the desktop clients.
    private val vectors = listOf(
        Triple(2560 to 1440, Triple(1445 to 813, 1920 to 1080, 1360 to 768), Unit),
        Triple(1920 to 1080, Triple(1445 to 813, 1920 to 1080, 1360 to 768), Unit),
        Triple(3840 to 2160, Triple(1445 to 813, 1920 to 1080, 1360 to 768), Unit),
        Triple(1080 to 2400, Triple(705 to 1568, 864 to 1920, 768 to 1712), Unit),
        Triple(1440 to 3120, Triple(723 to 1568, 886 to 1920, 768 to 1664), Unit),
        Triple(1080 to 3000, Triple(564 to 1568, 691 to 1920, 736 to 2048), Unit),
        Triple(1000 to 1000, Triple(1000 to 1000, 1000 to 1000, 768 to 768), Unit),
        Triple(640 to 480, Triple(640 to 480, 640 to 480, 640 to 480), Unit),
    )

    @Test
    fun matchesCanonicalTable() {
        for ((screen, expected, _) in vectors) {
            val (w, h) = screen
            val (c, g, o) = expected
            assertEquals("claude ${w}x$h", c, ProviderSizing.defaultSize("claude", w, h))
            assertEquals("gemini ${w}x$h", g, ProviderSizing.defaultSize("gemini", w, h))
            assertEquals("chatgpt ${w}x$h", o, ProviderSizing.defaultSize("chatgpt", w, h))
        }
    }

    @Test
    fun unknownModelReturnsNull() {
        assertNull(ProviderSizing.defaultSize("gpt-5", 1920, 1080))
    }
}
```

- [ ] **Step 3: Run test to verify it fails.**

Run: `cd screenmcp/android && ./gradlew testDebugUnitTest --tests "*ProviderSizingTest*"`
Expected: FAIL — unresolved reference `ProviderSizing`.

- [ ] **Step 4: Write minimal implementation.**

Create `screenmcp/android/app/src/main/java/com/doodkin/screenmcp/ProviderSizing.kt`:

```kotlin
package com.doodkin.screenmcp

import kotlin.math.floor
import kotlin.math.sqrt

/**
 * Provider-tuned default screenshot size, computed from the real screen dimensions.
 * See screenmcp/docs/model-sizing.md. Mirrors the desktop clients' provider_sizing.rs.
 */
object ProviderSizing {
    fun defaultSize(model: String, w: Int, h: Int): Pair<Int, Int>? {
        val wf = w.toDouble()
        val hf = h.toDouble()
        return when (model) {
            "claude" -> {
                val maxPixels = 1_176_000.0
                val maxEdge = 1568.0
                val s = minOf(1.0, maxEdge / maxOf(wf, hf), sqrt(maxPixels / (wf * hf)))
                var mw = floor(wf * s).toInt()
                var mh = floor(hf * s).toInt()
                while (mw.toLong() * mh.toLong() > maxPixels.toLong() && (mw > 1 || mh > 1)) {
                    if (mw >= mh) mw-- else mh--
                }
                Pair(mw, mh)
            }
            "gemini" -> {
                val shortCap = 1080.0
                val longCap = 1920.0
                val s = if (w >= h) minOf(1.0, longCap / wf, shortCap / hf)
                        else minOf(1.0, longCap / hf, shortCap / wf)
                Pair(Math.round(wf * s).toInt(), Math.round(hf * s).toInt())
            }
            "chatgpt" -> {
                val short = minOf(wf, hf)
                var s = minOf(1.0, 768.0 / short)
                val long = maxOf(wf, hf)
                if (long * s > 2048.0) s = 2048.0 / long
                Pair(round16(wf * s), round16(hf * s))
            }
            else -> null
        }
    }

    private fun round16(x: Double): Int = Math.max(16, (Math.round(x / 16.0) * 16).toInt())
}
```

- [ ] **Step 5: Run test to verify it passes.**

Run: `cd screenmcp/android && ./gradlew testDebugUnitTest --tests "*ProviderSizingTest*"`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add screenmcp/android/app/build.gradle.kts screenmcp/android/app/src/main/java/com/doodkin/screenmcp/ProviderSizing.kt screenmcp/android/app/src/test/java/com/doodkin/screenmcp/ProviderSizingTest.kt
git commit -m "feat: ProviderSizing rules + unit tests for android client"
```

### Task 6.2: Apply the helper to the screenshot and coordinate-scaling paths

**Files:**
- Modify: `screenmcp/android/app/src/main/java/com/doodkin/screenmcp/WebSocketClient.kt` (screenshot 337–382, `getOutputScale` 284–291, `scaleX` 271–276, `scaleY` 277–281)

- [ ] **Step 1: Add a shared resolver that factors in model.**

In `WebSocketClient.kt`, add a private helper (near `getOutputScale`, ~line 284). It returns the effective (maxWidth, maxHeight) as Doubles, mirroring the existing `<=0 disables` convention:

```kotlin
private fun resolveScaleDims(params: JSONObject?, dm: android.util.DisplayMetrics): Pair<Double, Double> {
    // explicit per-call params win
    val hasW = params?.has("max_width") == true
    val hasH = params?.has("max_height") == true
    if (hasW || hasH) {
        val mw = if (hasW) params!!.optDouble("max_width", DEFAULT_SCALE_WIDTH) else DEFAULT_SCALE_WIDTH
        val mh = if (hasH) params!!.optDouble("max_height", DEFAULT_SCALE_HEIGHT) else DEFAULT_SCALE_HEIGHT
        return Pair(mw, mh)
    }
    // model-based provider default
    val model = params?.optString("model", "") ?: ""
    if (model.isNotEmpty()) {
        ProviderSizing.defaultSize(model, dm.widthPixels, dm.heightPixels)?.let {
            return Pair(it.first.toDouble(), it.second.toDouble())
        }
    }
    return Pair(DEFAULT_SCALE_WIDTH, DEFAULT_SCALE_HEIGHT)
}
```

- [ ] **Step 2: Use it in `scaleX` and `scaleY`.**

Replace the body of `scaleX` (271–276) with:

```kotlin
private fun scaleX(x: Double, params: JSONObject?, dm: android.util.DisplayMetrics): Float {
    val (mw, _) = resolveScaleDims(params, dm)
    if (mw > 0.0) return (x * dm.widthPixels / mw).toFloat()
    return x.toFloat()
}
```

and `scaleY` (277–281) with:

```kotlin
private fun scaleY(y: Double, params: JSONObject?, dm: android.util.DisplayMetrics): Float {
    val (_, mh) = resolveScaleDims(params, dm)
    if (mh > 0.0) return (y * dm.heightPixels / mh).toFloat()
    return y.toFloat()
}
```

- [ ] **Step 3: Use it in `getOutputScale` (used by `ui_tree`).**

Replace the body of `getOutputScale` (284–291) with:

```kotlin
private fun getOutputScale(params: JSONObject?, dm: android.util.DisplayMetrics): Pair<Double, Double> {
    val (mw, mh) = resolveScaleDims(params, dm)
    if (mw <= 0.0 && mh <= 0.0) return Pair(1.0, 1.0)
    val sx = if (mw > 0.0) mw / dm.widthPixels else mh / dm.heightPixels
    val sy = if (mh > 0.0) mh / dm.heightPixels else sx
    return Pair(sx, sy)
}
```

- [ ] **Step 4: Use it in the `screenshot` command.**

In the `"screenshot"` case (337–382), replace the `maxWidth`/`maxHeight` lines (342–344) with:

```kotlin
    val quality = params?.optInt("quality", 100) ?: 100
    val dm = service.resources.displayMetrics
    val (mwD, mhD) = resolveScaleDims(params, dm)
    val maxWidth = mwD.toInt()
    val maxHeight = mhD.toInt()
```

(The existing `service.scaleBitmap(softBitmap, maxWidth, maxHeight)` call is unchanged. If `dm` is already defined earlier in the dispatch scope, reuse it instead of redeclaring.)

- [ ] **Step 5: Build the app.**

Run: `cd screenmcp/android && ./gradlew assembleDebug`
Expected: build succeeds.

- [ ] **Step 6: Commit**

```bash
git add screenmcp/android/app/src/main/java/com/doodkin/screenmcp/WebSocketClient.kt
git commit -m "feat: apply model-based default size to android screenshot + coord scaling"
```

---

## Phase 7 — Cloud web dashboard

### Task 7.1: `buildMcpUrl` util (pure, tested)

**Files:**
- Create: `screenmcp-cloud/web/src/lib/mcpUrl.ts`
- Test: `screenmcp-cloud/web/src/lib/mcpUrl.test.ts`
- Modify: `screenmcp-cloud/web/package.json` (add a `test` script)

- [ ] **Step 1: Add a test script** (Node's runner via `tsx`, which Next projects can run without a full test framework):

First ensure `tsx` is available: `cd screenmcp-cloud/web && npm install -D tsx`. Then in `web/package.json` `scripts`, add (explicit file path for portability):

```json
    "test": "node --import tsx --test src/lib/mcpUrl.test.ts"
```

- [ ] **Step 2: Write the failing test.**

Create `screenmcp-cloud/web/src/lib/mcpUrl.test.ts`:

```typescript
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildMcpUrl } from './mcpUrl.ts';

test('appends ?model= for a real provider', () => {
  assert.equal(buildMcpUrl('https://mcp.screenmcp.com/mcp', 'gemini'),
    'https://mcp.screenmcp.com/mcp?model=gemini');
});

test('omits the param for default/empty', () => {
  assert.equal(buildMcpUrl('https://mcp.screenmcp.com/mcp', 'default'),
    'https://mcp.screenmcp.com/mcp');
  assert.equal(buildMcpUrl('https://mcp.screenmcp.com/mcp', ''),
    'https://mcp.screenmcp.com/mcp');
});
```

- [ ] **Step 3: Run test to verify it fails.**

Run: `cd screenmcp-cloud/web && npm test`
Expected: FAIL — cannot find `./mcpUrl.ts`.

- [ ] **Step 4: Write minimal implementation.**

Create `screenmcp-cloud/web/src/lib/mcpUrl.ts`:

```typescript
export function buildMcpUrl(base: string, model: string): string {
  return model && model !== 'default' ? `${base}?model=${model}` : base;
}
```

- [ ] **Step 5: Run test to verify it passes.**

Run: `cd screenmcp-cloud/web && npm test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add screenmcp-cloud/web/src/lib/mcpUrl.ts screenmcp-cloud/web/src/lib/mcpUrl.test.ts screenmcp-cloud/web/package.json
git commit -m "feat: buildMcpUrl util for cloud web"
```

### Task 7.2: Add the model dropdown to the dashboard

**Files:**
- Modify: `screenmcp-cloud/web/src/app/dashboard/page.tsx` (MCP config block 610–646; state near 75)

- [ ] **Step 1: Add state + import.**

Near the other `useState` declarations (~line 75), add:

```tsx
const [selectedModel, setSelectedModel] = useState("default");
```

At the top of the file with the other imports, add:

```tsx
import { buildMcpUrl } from "@/lib/mcpUrl";
```

- [ ] **Step 2: Compute the URL once, above the JSX that renders the config (just before line ~610).**

```tsx
const mcpUrl = buildMcpUrl("https://mcp.screenmcp.com/mcp", selectedModel);
```

- [ ] **Step 3: Add the dropdown before the `<pre>` block (inside the MCP registration `div`, after line 614).**

```tsx
<div className="mb-3 flex items-center gap-2">
  <label className="text-sm font-medium text-green-800 dark:text-green-200">Model:</label>
  <select
    value={selectedModel}
    onChange={(e) => setSelectedModel(e.target.value)}
    className="rounded-lg border border-zinc-300 px-3 py-2 text-sm focus:border-zinc-500 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
  >
    <option value="default">Default</option>
    <option value="claude">Claude</option>
    <option value="gemini">Gemini</option>
    <option value="chatgpt">ChatGPT</option>
  </select>
</div>
```

- [ ] **Step 4: Use `mcpUrl` in BOTH the `<pre>` text (line 619) and the copy handler (line 631).**

In the `<pre>` template literal, replace the hardcoded `"url": "https://mcp.screenmcp.com/mcp"` with `"url": "${mcpUrl}"`. In the `onClick` copy handler's `JSON.stringify`, replace the `url:` value `"https://mcp.screenmcp.com/mcp"` with `url: mcpUrl,`.

- [ ] **Step 5: Type-check + lint.**

Run: `cd screenmcp-cloud/web && npx tsc --noEmit && npm run lint`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add screenmcp-cloud/web/src/app/dashboard/page.tsx
git commit -m "feat: model dropdown appends ?model= to dashboard MCP URL"
```

---

## Phase 8 — Documentation

### Task 8.1: `model-sizing.md` and `commands.md`

**Files:**
- Create: `screenmcp/docs/model-sizing.md`
- Modify: `screenmcp/docs/commands.md` (the `screenshot`, `screenshot_region`, `screenshot_window`, `ui_tree` entries)

- [ ] **Step 1: Write `screenmcp/docs/model-sizing.md`.**

```markdown
# Model-based screenshot sizing

Set `?model=claude|gemini|chatgpt` on the MCP connection URL. When the agent calls a
coordinate command without `max_width`/`max_height`, the device sizes the screenshot —
and the matching click-coordinate space — to that model's vision limits.

## Rules (computed from the device's real screen W×H)

- **claude** — `s = min(1, 1568/max(w,h), sqrt(1,176,000/(w*h)))`; floor; shrink if over
  the 1,176,000-pixel cap. Safe for every Claude model.
- **gemini** — orientation-aware caps: shortest ≤ 1080, longest ≤ 1920.
- **chatgpt** — shortest side → 768, longest ≤ 2048, each dimension rounded to the
  nearest multiple of 16.
- Unknown/absent model → legacy default (1456×819).

## Canonical examples

| screen | claude | gemini | chatgpt |
|---|---|---|---|
| 2560×1440 | 1445×813 | 1920×1080 | 1360×768 |
| 1080×2400 | 705×1568 | 864×1920 | 768×1712 |
| 1000×1000 | 1000×1000 | 1000×1000 | 768×768 |

`model` is injected by the MCP server into all coordinate-bearing commands (screenshot
family + click/drag/scroll/etc.) so the image and its click coordinates always share one
coordinate space. Rationale and validation: see
`image-to-components/docs/research/2026-05-12-vision-validation-report.md`.
```

- [ ] **Step 2: Document the `model` param on the four tools in `commands.md`.**

Under each of `screenshot`, `screenshot_region`, `screenshot_window`, `ui_tree`, add a param row:

```
| model | string | — | `claude`/`gemini`/`chatgpt`. Provider-tuned default size when max_width/max_height omitted. Usually set by the connection ?model= param, not per call. |
```

- [ ] **Step 3: Commit**

```bash
git add screenmcp/docs/model-sizing.md screenmcp/docs/commands.md
git commit -m "docs: model-based screenshot sizing"
```

---

## Phase 9 — (Optional) SDK pass-through

Low priority; only if SDK callers want to set `model` explicitly. Add an optional `model`
field to the screenshot methods' params in `sdk/typescript/src/client.ts`,
`sdk/python/src/screenmcp/client.py`, `sdk/rust/src/client.rs`. No new behavior — it just
forwards `model` like any other param. Skip unless requested.

---

## Final verification (run before declaring done)

- [ ] `cd screenmcp/mcp-server && npm test && npx tsc --noEmit`
- [ ] `cd screenmcp-cloud/mcp-server && cargo test && cargo build`
- [ ] `cd screenmcp/windows && cargo test` · `cd screenmcp/mac && cargo test` · `cd screenmcp/linux && cargo test`
- [ ] `cd screenmcp/android && ./gradlew testDebugUnitTest assembleDebug`
- [ ] `cd screenmcp-cloud/web && npm test && npx tsc --noEmit`
- [ ] Manual smoke: connect an MCP client with `...?model=gemini`, call `screenshot` with no size args, confirm the returned image is ≤ 1920×1080 oriented correctly; then `click` a known target and confirm it lands.
```
