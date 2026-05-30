# Design: Per-connection model screenshot sizing

**Date:** 2026-05-30
**Status:** Approved (brainstorming) — ready for implementation planning
**Editions:** open-source (`screenmcp/`) and cloud (`screenmcp-cloud/`)

## Problem

Different AI vision pipelines have different optimal input image sizes. Sending the
wrong size wastes tokens or — for Claude — triggers internal rescaling that makes the
model return coordinates in a *scaled* space, breaking click accuracy (see
`image-to-components/docs/research/2026-05-12-vision-validation-report.md`).

ScreenMCP is currently model-agnostic: screenshots default to **1456×819** and can be
overridden per call via `max_width`/`max_height`. In practice **agents pass these
parameters unreliably** — they frequently omit them and even get confused about which
model they are. So the correct image size cannot depend on the agent asking for it; it
must be a **per-connection default** the mcp connection to server sets *for* the agent, server adds model default to each relevant call.

Note: the current 1456×819 default is ~16k pixels *over* Claude's 1,176,000 pixel cap,
so it already triggers latent rescaling for Claude consumers. The `claude` profile fixes
this.

## Goal

Let the MCP **connection URL** declare which provider consumes the screenshots:

```
…/api/mcp?model=claude        (open-source)
…/mcp?model=gemini            (cloud)
```

The device client then auto-sizes screenshots to that provider's documented vision
limits, computed from the device's **real** screen dimensions — no hardcoded output
sizes, no reliance on the agent.

## Non-goals (YAGNI)

- Per-model Claude profiles (one `claude` profile is safe for all Claude models).
- Image format switching or quality tuning (all providers support WebP; unchanged).
- Prompt / coordinate-hint injection into tool descriptions.
- Persistent config fields (`worker.toml`, cloud account settings). **URL param only.**
- A global pixel ceiling on top of the per-provider rules.

## Source of truth — precedence (high → low)

1. **Per-call** `max_width`/`max_height` on the tool call — explicit agent override.
   Already exists; unchanged. When present, the provider rule is ignored.
2. **URL query param** `?model=claude|gemini|chatgpt` — the only new config
   surface. Per MCP connection.
3. **Nothing set** → existing legacy default (1456×819). Behavior unchanged for current
   users.

Unknown/invalid `model` values are treated as "nothing set" (legacy default),
never an error.

## Data flow

```
MCP client URL: …/api/mcp?model=gemini
        │   TS server: read req query per-request
        │   Rust cloud server: capture query param at session init, store on session
        ▼
MCP server: for EVERY coordinate-bearing command (screenshot family AND
            click / long_click / drag / scroll / double_click / right_click /
            middle_click / mouse_move / mouse_scroll),
            if the caller gave NO max_width/max_height,
            inject  model="gemini"  into the command params
        ▼   worker relays unchanged (generic JSON relay — no changes)
Device client (Android / Windows / Mac / Linux):
            max_width/max_height present?  → use them            (legacy path)
            else model present?   → compute (maxW,maxH) from REAL screen dims
            else                           → legacy default 1456×819
        ▼
Both the screenshot output size AND the input-coordinate scaling resolve their default
through the SAME providerDefaultSize() helper — so the image the model sees and the
coordinate space its clicks are interpreted in always match.
```

**Coordinate consistency (critical).** Screenshot sizing and input-coordinate scaling
(`scale_xy` / `scaleX`/`scaleY`, and `ui_tree` bounds via `get_output_scale`) must resolve
to the *same* `(maxW, maxH)`. Today that holds only because both default to 1456×819. So
`model` is injected into **all** coordinate-bearing commands, not just the screenshot
tools, and every client routes the default through one shared `providerDefaultSize`
helper. Injecting `model` onto commands that don't scale is harmless — the device reads
only the keys it needs.

## Sizing rules (client-side, aspect-preserving, downscale-only)

Each rule is a pure function of the device's real screen `(w, h)` → target `(maxW, maxH)`.
The only constants are each provider's documented caps.

```
claude(w, h):
    maxPixels = 1_176_000          # provider pixel cap (sonnet/haiku-safe; works for all Claude models)
    maxEdge   = 1568               # provider long-edge cap
    s = min(1, maxEdge / max(w, h), sqrt(maxPixels / (w * h)))
    mw, mh = floor(w * s), floor(h * s)
    while mw * mh > maxPixels and (mw > 1 or mh > 1):   # float-overshoot guard
        shrink the longer of (mw, mh) by 1
    return mw, mh

gemini(w, h):
    shortCap, longCap = 1080, 1920     # orientation-aware caps
    if w >= h:  s = min(1, longCap / w,  shortCap / h)   # landscape
    else:       s = min(1, longCap / h,  shortCap / w)   # portrait
    return round(w * s), round(h * s)

chatgpt(w, h):
    short = min(w, h)
    s = min(1, 768 / short)                  # shortest side → 768 (downscale only)
    if max(w, h) * s > 2048:                 # clamp long edge
        s = 2048 / max(w, h)
    return round16(w * s), round16(h * s)     # round each to nearest multiple of 16, min 16

none / unknown:  leave existing legacy default (1456×819)
```

`round16(x) = max(16, round(x / 16) * 16)`.

Downscale-only: `s` is capped at `1`, so small screens are never upscaled.

### Worked examples (illustrative only — not hardcoded)

| Provider | Landscape monitor 2560×1440 | Portrait phone 1080×2400 | Square window 1000×1000 |
|---|---|---|---|
| claude  | 1446×813 (pixel cap binds) | 705×1568 (long-edge cap binds) | 1000×1000 (under caps — unchanged) |
| gemini  | 1920×1080 | 864×1920 | 1000×1000 |
| chatgpt | 1360×768 → round16 → 1360×768 | 768×1712 (round16) | 768×768 |

## Components to change

Mapped onto `screenmcp/docs/adding-new-command.md`.

`COORD_TOOLS` = the set of coordinate-bearing commands the server injects into:
`screenshot`, `screenshot_region`, `screenshot_window`, `ui_tree`, `click`,
`long_click`, `drag`, `scroll`, `double_click`, `right_click`, `middle_click`,
`mouse_move`, `mouse_scroll`.

### MCP servers (resolve provider + inject default)
- **Open-source TS** `mcp-server/src/mcp.ts`
  - Parse `model` from the request URL query string (per request).
  - Add optional `model` to the input schema of the screenshot-family tools
    (`screenshot`, `screenshot_region`, `screenshot_window`, `ui_tree`) so it is
    documented; input commands accept it silently.
  - In the tool-registration wrapper: for any tool in `COORD_TOOLS`, if
    `max_width`/`max_height` are both absent and a valid `model` is resolved, set
    `phoneParams.model` before calling the handler / `sendCommand`.
- **Cloud Rust** `screenmcp-cloud/mcp-server/src/{mcp.rs,tools.rs,main.rs}`
  - Capture the `model` query param on the `/mcp` connection at session init;
    store it on the `McpSession`.
  - Add `model` to the screenshot-family `ToolDef` input schemas.
  - In `handle_tools_call`: for any tool in `COORD_TOOLS`, inject the session's
    `model` into `phone_params` when `max_width`/`max_height` are absent.

### Worker — no changes (generic relay).

### Device clients (compute size from the rule)
A shared helper `providerDefaultSize(model, w, h) -> (maxW, maxH)?` implementing the
rules above (returns nothing for unknown providers). It is consulted **only** when
`model` is present and `max_width`/`max_height` are absent, and it feeds BOTH:
- the **screenshot output** sizing path, and
- the **input-coordinate scaling** path (`scale_xy` / `scaleX`/`scaleY`) and the
  `ui_tree` bounds scaling (`get_output_scale`),

so the picture the model sees and the coordinate space its clicks land in always match.
- **Android** `ScreenMcpService.kt` (helper + screenshot path + `scaleX/scaleY` +
  `getOutputScale`), `WebSocketClient.kt` (no per-key change — already forwards `params`).
- **Windows** `windows/src/commands.rs`
- **Mac** `mac/src/commands.rs`
- **Linux** `linux/src/commands.rs`

Each client reads its own real screen dimensions (already available where the current
scaling happens) and applies the helper. The desktop clients duplicate the helper (they
are separate crates, matching the existing windows/mac/linux duplication). Clients that
can't capture (unsupported) keep returning their existing stubs.

### Cloud web — the "settings input"
- `screenmcp-cloud/web/src/app/...` connection / setup page: a **provider dropdown**
  (`Default` / `Claude` / `Gemini` / `ChatGPT`) that appends `?model=…` to the
  copyable MCP URL. This is the only UI surface.

### SDKs
- `model` pass-through on the screenshot methods (TS/Python/Rust) for
completeness — low priority, not required for the core feature.

### Docs
- `commands.md`: document the new optional `model` param on the four tools.
- New `docs/model-sizing.md`: the rules, the caps, and the rationale (link the
  validation report).

## Testing

1. **Unit tests for the size helper** (one per client language, or a shared reference
   table): feed a matrix of screen sizes — landscape monitor (2560×1440, 3840×2160),
   portrait phone (1080×2400, 1440×3120), square (1000×1000), tiny (640×480) — and assert
   exact `(maxW, maxH)` for each provider, including:
   - Claude pixel-cap shrink loop (no output exceeds 1,176,000 px; long edge ≤ 1568).
   - Gemini orientation handling (caps swap by orientation).
   - ChatGPT ×16 rounding and 2048 long-edge clamp.
   - Downscale-only (a 640×480 screen is returned unchanged for gemini/chatgpt).
2. **Fake-device + SDK round-trip** (`fake-device/`, SDK example tests): connect with
   each `?model=` value, call `screenshot` with **no** size args, assert the
   returned image dimensions match the provider rule for the fake device's screen size.
   Also assert an explicit `max_width` still overrides the provider rule.
3. **Regression**: no `model` → image dims unchanged (1456×819).

## Open implementation questions (resolve in planning)

- Exact location of screen-dimension capture in each desktop client relative to the
  current scaling code.
- Whether the cloud session stores `model` at `initialize` or re-reads it per
  request (Streamable HTTP keeps the query string on every POST, so per-request read is
  also viable and matches the TS side).
