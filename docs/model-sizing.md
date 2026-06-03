# Model-based screenshot sizing

Set `?model=claude|gemini|chatgpt` on the MCP connection URL. When the agent calls a
coordinate-bearing command without `max_width`/`max_height`, the device sizes the
screenshot — and the matching click-coordinate space — to that model's vision limits.

```
…/api/mcp?model=claude        (open-source)
…/mcp?model=gemini            (cloud)
```

## How it flows

1. The MCP server reads `model` from the connection URL (open-source: per request; cloud:
   captured on the session at init).
2. For every coordinate-bearing command (`screenshot`, `screenshot_region`,
   `screenshot_window`, `ui_tree`, `get_screen_size`, `click`, `long_click`, `drag`,
   `scroll`, `double_click`, `right_click`, `middle_click`, `mouse_move`, `mouse_scroll`),
   if the caller gave no explicit `max_width`/`max_height`, the server injects
   `model` into the command.
3. The device resolves its effective size through one shared `provider_default_size`
   helper, used by BOTH the screenshot output and the input-coordinate scaling — so the
   image the model sees and the coordinates its clicks land in always share one space.
4. An explicit per-call `max_width`/`max_height` always overrides the model rule.

## Rules (computed from the device's real screen W×H)

- **claude** — `s = min(1, 1568/max(w,h), sqrt(1,176,000/(w·h)))`; floor; shrink if over
  the 1,176,000-pixel cap. Safe for every Claude model.
- **gemini** — orientation-aware caps: shortest ≤ 1080, longest ≤ 1920.
- **chatgpt** — shortest side → 768, longest ≤ 2048, each dimension rounded to the
  nearest multiple of 16.
- Unknown/absent model → legacy default (1456×819). Downscale-only: small screens are
  never upscaled.

## Canonical examples

| screen | claude | gemini | chatgpt |
|---|---|---|---|
| 2560×1440 | 1445×813 | 1920×1080 | 1360×768 |
| 1920×1080 | 1445×813 | 1920×1080 | 1360×768 |
| 1080×2400 | 705×1568 | 864×1920 | 768×1712 |
| 1000×1000 | 1000×1000 | 1000×1000 | 768×768 |
| 640×480 | 640×480 | 640×480 | 640×480 |

Rationale and empirical validation of these caps:
`image-to-components/docs/research/2026-05-12-vision-validation-report.md`.

## Transports without a connection URL

The standalone stdio CLI (`python-cli/`) has no connection URL, so it takes `model`
as a **per-command argument** on each coordinate-bearing tool instead of `?model=`.
The sizing algorithm and canonical table are identical (it ports
`provider_default_size` verbatim).
