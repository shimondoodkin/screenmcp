# ui_tree filters and flat output (Windows)

## Overview

Add scoping, filtering, and output-shape control to the Windows `ui_tree` command. Today the call walks the entire UIAutomation tree from the desktop root and returns every visible top-level window with its full descendant subtree (max depth 10). On a normal desktop this routinely produces ~600+ nodes, the vast majority of which are non-actionable container types (Pane, Group, ScrollBar, Custom). LLM-driven agents pay this cost in tokens on every call, then discard 90%+ of the payload.

This change pushes filtering, scoping, and coordinate-pre-computation to the server, and lets the caller pick a flat output shape that returns ready-to-click center coordinates instead of a nested tree.

Scope: **Windows only**. Mac and Linux already use flat window-list APIs (CGWindowList, wmctrl) and don't have the depth problem; Android's AccessibilityService is already scoped to the active app. Those platforms silently ignore the new params.

## Today's behavior (baseline, must not regress)

`handle_ui_tree_raw` in `screenmcp/windows/src/commands.rs:1559` walks `automation.ControlViewWalker()` from `automation.GetRootElement()`. Top-level walk is z-order front-to-back with sibling occlusion culling. Each node gets phase-1 cheap filters (offscreen, viewport, occlusion), phase-2 noise filter (drop leaves with empty name AND empty automationId), then phase-3 full property extraction.

Output: `{ "tree": [...], "os": "windows" }` where each node is sparse JSON (only non-default values present). Field set: `text`, `value`, `controlType`, `className`, `resourceId`, `contentDescription`, `bounds` (`{left, top, right, bottom, width, height}`), `enabled`, `clickable`, `editable`, `scrollable`, `checked`, `focused`, `hwnd`, `children`. Coordinate scaling via `max_width`/`max_height` is applied at the end via `scale_bounds_in_value`.

Calling `ui_tree` with no new params after this change must return byte-for-byte the same JSON as today.

## New parameters

All optional. All silently ignored on non-Windows clients.

| Param | Type | Default | Notes |
|---|---|---|---|
| `window` | string \| number | — | String: case-insensitive title substring. Number: native `hwnd`. Scopes the walk to a single top-level window. |
| `region` | object | — | `{min_x, min_y, max_x, max_y}` in screenshot space (same coordinate scaling as bounds). Drops nodes that don't match `region_mode`. |
| `region_mode` | `"inside"` \| `"intersect"` | `"inside"` | `"inside"` keeps only nodes whose bounds are fully contained in `region`. `"intersect"` keeps any node whose bounds overlap `region`. |
| `types` | string[] | all | Whitelist of `controlType` values. Case-insensitive match against the strings in `control_type_name` (`Button`, `Edit`, `MenuItem`, etc.). LLM-friendly: `"button"`, `"BUTTON"`, and `"Button"` all work. |
| `text_match` | string | — | Filter on the `text` field. |
| `regex` | bool | false | If `true`, `text_match` is treated as a regex. If `false`, plain case-insensitive substring. |
| `max_depth` | int | 10 | Cap recursion depth. Top-level windows are depth 1. |
| `format` | `"nested"` \| `"flat"` | `"nested"` | Output shape (see below). |
| `fields` | string[] | per-format default | Per-node fields to emit. |

### `fields` vocabulary

```
text, value, controlType, className, resourceId, contentDescription,
bounds, cx, cy,
enabled, clickable, editable, scrollable, checked, focused,
hwnd,
path
```

`cx` and `cy` are new precomputed center coordinates (in the same coordinate space as `bounds`, after scaling). `path` is a string like `"Notepad / Document area / Save"` — only meaningful in flat mode.

### Always-implicit fields

- `controlType` is always emitted regardless of `fields`. Filter logic and breadcrumb logic both depend on it.
- `children` is always emitted in nested mode (it is the tree). It is absent in flat mode.

### Default `fields`

- **Nested mode, no `fields` set** → today's exact field set, no `cx`/`cy`. Byte-for-byte backward compatible.
- **Flat mode, no `fields` set** → `[controlType, text, cx, cy, hwnd, path]`.
- **`fields` set explicitly** → exactly those fields, plus the always-implicit ones. Sparse rule still applies (don't emit empty/default values within the chosen field set).

## Filter pipeline

Applied in order during the existing recursive walk, before phase-3 property extraction where possible:

1. **`window` scope** — At top-level walk only. If `window` is a string, keep only top-level elements whose name contains it (case-insensitive). If a number, keep only the element whose `CurrentNativeWindowHandle` matches. If `window` is set and zero windows match, return `{tree: []}` (or `{nodes: []}` in flat mode).
2. **`max_depth`** — Replaces the existing depth-10 constant.
3. **`region` filter** — Per-node filter on bounds (display filter, not scope). Always recurse into children even when a container fails `region_mode`: UIA does not guarantee parent bounds enclose child bounds (synthetic Pane elements often report tiny bounds while their children sit elsewhere on screen). Pruning subtrees on parent-region-fail would silently drop visible elements.
4. **`types` filter** — Keep nodes whose `controlType` ∈ `types` (case-insensitive).
5. **`text_match` / `regex` filter** — Keep nodes whose `text` matches.

### Breadcrumb policy (nested mode)

A node that fails any **display** filter (`region`, `types`, `text_match`) is still emitted **if any descendant passes**. The node's full property set is collected so the client can read its title/role; it functions as a container leading to the real match. Without this, filtering by `types=["Button"]` would return orphaned buttons with no idea which window or dialog they live in.

`window` is a **scope** filter, not a display filter: top-level windows that don't match are skipped entirely with no breadcrumb (the desktop root is implicit and never emitted as a breadcrumb either way).

### Flat mode shape

```json
{
  "nodes": [
    {
      "controlType": "Button",
      "text": "Save",
      "cx": 512,
      "cy": 300,
      "hwnd": 1234567,
      "path": "Notepad / File menu / Save"
    },
    ...
  ],
  "os": "windows"
}
```

`path` is built by joining `text` (or `controlType` if no text) of each ancestor with ` / `. **Ancestors only** — the target node's own text is not included in `path` (it's already in the node's `text` field). Top-level window first, immediate parent last. Empty string if the target is itself a top-level window. Ancestors are determined by the walk parent chain, not by post-filter parents.

In flat mode, the breadcrumb policy of nested mode is irrelevant — only nodes that pass all filters appear in `nodes`. The `path` field carries the context that breadcrumb ancestors carry in nested mode.

## Coordinate scaling

`cx`/`cy` are computed from the post-scaled `bounds`:

```
cx = (left + right) / 2
cy = (top + bottom) / 2
```

Existing `scale_bounds_in_value` already handles `bounds` scaling. The flat-mode emitter computes `cx`/`cy` after scaling to keep them consistent with `bounds`.

## Backward compatibility

- All new params optional; none required.
- `ui_tree` with no new params: identical output to today (verified by golden file).
- `ui_tree` with `format="nested"` (explicit) and no other new params: identical output.
- Mac, Linux, Android clients ignore unknown params; their existing output shapes are unchanged.
- MCP server (`mcp-server/src/mcp.ts`) forwards `params` verbatim — no protocol change.

## Component changes

This is a **modification to an existing command**, not a new command. Reference: `screenmcp/docs/adding-new-command.md` lists every area that holds command-related code. The list below walks that same structure and flags which areas need real changes vs. which are no-ops because `ui_tree` already exists everywhere.

### 1. Android app — no change

Android's `ui_tree` runs against the AccessibilityService and is already scoped to the active app. Unknown params are ignored by the existing dispatch; verify no crash on extra keys. No new behavior for these params on Android.

### 2. Windows desktop client — primary implementation

`screenmcp/windows/src/commands.rs` is where the real work happens.

The current `handle_ui_tree_raw` function takes no params. New signature:

```rust
fn handle_ui_tree_raw(opts: &UiTreeOpts) -> Result<Value, String>
```

Where `UiTreeOpts` carries the parsed params. `handle_ui_tree` (the wrapper that does coord scaling) parses params into `UiTreeOpts`, calls `handle_ui_tree_raw`, then applies coordinate scaling.

`walk_element` gains an `opts: &UiTreeOpts` parameter and a `path: &mut Vec<String>` parameter (for flat mode `path` building).

New helpers:
- `parse_ui_tree_opts(params: Option<&Value>) -> Result<UiTreeOpts, String>`
- `node_passes_display_filter(opts, control_type, text, bounds) -> bool` — single per-node predicate covering `types`, `text_match`/`regex`, and `region`.
- `flatten_walk(...)` — alternate walker for flat mode; collects `Vec<FlatNode>` instead of nesting.
- `build_node_value(el, opts, fields) -> Value` — replaces the inline phase-3 emit; honors `fields` selection.

The phase-1 cheap filters (offscreen, viewport, sibling occlusion) are unchanged. The noise filter (skip empty-name empty-id leaves) is unchanged for nested mode and applied to the post-filter set in flat mode. `walk_element` is in `commands.rs` around line 1639.

### 3. Mac desktop client — no change

`mac/src/commands.rs` `ui_tree` uses CGWindowList and returns a flat window list (no UIA-style descendants). New params are silently ignored. Verify no crash on extra keys.

### 4. Linux desktop client — no change

`linux/src/commands.rs` `ui_tree` uses wmctrl. Same story as Mac. No code change.

### 5. Worker — no change

Generic relay; doesn't inspect params.

### 6. MCP server — open source (TypeScript)

`mcp-server/src/mcp.ts`: update the `ui_tree` Zod schema to advertise the new params and update the tool description to mention the new capabilities. The handler stays a pass-through. The schema lift is real — current schema is just `{device_id, ...scalingParams}`; new one needs the eight new optional params.

### 7. MCP server — cloud (Rust)

`screenmcp-cloud/mcp-server/src/tools.rs`: update the `ui_tree` `ToolDef.input_schema` JSON Schema to mirror the open-source MCP server. Description text should match.

### 8. TypeScript SDK

`sdk/typescript/src/client.ts`: if `DeviceConnection.uiTree(...)` has a typed param signature, extend it with the new optional fields. If it takes a generic `Record<string, unknown>`, no signature change needed but consider adding a typed overload.

`sdk/typescript/src/types.ts`: add types for the flat-mode response (`{nodes: FlatNode[]}`) and the new node fields (`cx`, `cy`, `path`). Keep the existing nested response type valid (most fields stay optional).

### 9. Python SDK

`sdk/python/src/screenmcp/client.py`: same treatment as TypeScript — extend the `ui_tree` method signature with new optional kwargs.

`sdk/python/src/screenmcp/types.py`: add dataclasses for flat response if SDK exposes typed responses.

### 10. Rust SDK

`sdk/rust/src/client.rs`: extend `ui_tree` method (likely a builder or struct param) with new fields.

`sdk/rust/src/types.rs`: add structs for flat response and new node fields.

### 11. Cloud web playground

`screenmcp-cloud/web/src/app/playground/page.tsx`: if the playground exposes a `ui_tree` form, add inputs for the new params (window text input, types multi-select, format radio, etc.). Add to `buildParams()` so they get sent. Worth showing flat output as a JSON view distinct from nested.

### 12. Documentation

- `screenmcp/docs/commands.md` — update the `ui_tree` parameter table with all eight new params, and add example calls for each main scenario (window scope, types filter, region filter, flat output).
- `screenmcp/docs/wire-protocol.md` — add wire-message examples for the new params and for both `{tree: ...}` and `{nodes: ...}` response shapes.
- `screenmcp/docs/return-value-windows-ui-tree.md` — document new node fields (`cx`, `cy`, `path`), the flat shape `{ "nodes": [...] }`, and the `fields` vocabulary.
- `screenmcp/docs/implementations.md` — note that the new `ui_tree` params are Windows-only; other platforms accept and ignore them.

### 13. Remote CLI

`remote/src/`: if the REPL has a `ui_tree` command with positional args, extend it. Most likely it just passes JSON params through; verify no hardcoded param list to update.

### 14. Fake device

`fake-device/src/fake_device/commands.py`: the fake `ui_tree` response should respect the new params well enough for SDK tests to exercise them. Minimum bar: when `format="flat"` is sent, return `{nodes: [...]}` instead of `{tree: [...]}`. When `fields=[...]` is sent, the hardcoded response should at least include those fields. Filter behavior can be a stub (return canned data; don't actually filter).

### 15. SDK tests

- `fake-device/test_with_sdk.py` — add Python SDK test block: call `ui_tree` with `format="flat"`, with `types=["Button"]`, with `window="..."`. Verify response shape.
- `sdk/typescript/examples/cli/test_fake_device.ts` — same coverage in TypeScript.
- `sdk/rust/examples/test_fake_device.rs` — same coverage in Rust.

### Checklist

```
[ ] Windows: commands.rs — UiTreeOpts, parse_ui_tree_opts, walk_element, flatten_walk, build_node_value
[ ] Android: WebSocketClient.kt — verify no crash on unknown ui_tree params (likely no-op)
[ ] Mac: commands.rs — verify no crash on unknown params (no-op)
[ ] Linux: commands.rs — verify no crash on unknown params (no-op)
[ ] MCP Server (TS): mcp.ts — extend ui_tree Zod schema + description
[ ] MCP Server (Rust cloud): tools.rs — extend ui_tree input_schema JSON
[ ] SDK TypeScript: client.ts, types.ts — extend uiTree, add flat-mode types
[ ] SDK Python: client.py, types.py — extend ui_tree, add flat-mode types
[ ] SDK Rust: client.rs, types.rs — extend ui_tree, add flat-mode types
[ ] Fake device: commands.py — respect format/fields well enough for SDK tests
[ ] Test Python: test_with_sdk.py — add new param coverage
[ ] Test TypeScript: test_fake_device.ts — add new param coverage
[ ] Test Rust: test_fake_device.rs — add new param coverage
[ ] Playground: page.tsx — add UI inputs for new params
[ ] Docs: commands.md, wire-protocol.md, return-value-windows-ui-tree.md, implementations.md
```

## Error handling

- `window` set but no window matches title/hwnd → return `{tree: []}` (nested) or `{nodes: []}` (flat). Not an error.
- `region` malformed (missing keys, max < min) → return `{status: "error", error: "..."}`.
- `regex` set but `text_match` is an invalid regex → error.
- `fields` containing an unknown name → error listing the unknown name (helps catch typos).
- `format` not in {`"nested"`, `"flat"`} → error.
- `max_depth` < 1 → error.

## Testing

Unit tests in `screenmcp/windows/`:
- `parse_ui_tree_opts` happy paths and each error path.
- `node_passes_display_filter` against `types`/`text_match`/`regex` combos.
- `node_passes_region_filter` for `inside` and `intersect` modes.
- Path-building (`build_path`).

Integration tests against a known UIA target (the existing `test_window.rs` debug window):
- Baseline: no params → matches a golden snapshot.
- `window=` by title and by hwnd.
- `types=["Button"]` → tree contains only Buttons + their breadcrumb ancestors.
- `format="flat"` → returns `{nodes: [...]}` with `path` strings.
- `fields=["text", "cx", "cy"]` in nested mode → only those fields emitted (plus always-implicit `controlType` and `children`).
- `region` inside vs intersect: place test elements partially-outside and verify each mode's behavior.
- `max_depth=1` → only top-level windows, no descendants.
- Coordinate scaling: `max_width=1456` together with `region` and `cx`/`cy` — verify region is interpreted in scaled space and cx/cy are also in scaled space.

## YAGNI list (deliberately not in scope)

- `find_element` as a separate command — caller can use `format="flat"` + filters + take `nodes[0]`. Worth its own design when we know what the failure mode (zero matches, multiple matches) UX should be.
- Set-of-Marks screenshot annotation.
- `wait_for_window` and `click_in_region` — separate designs.
- Mac/Linux UIA-equivalent trees.
- An `exclude_types` blacklist param. `types` whitelist is sufficient for the current pain point; blacklist can be added later if needed.
- A `parent_path` field on each flat node (structured ancestor list). The string `path` field is enough for LLM consumption; structured form is YAGNI until something asks for it.
