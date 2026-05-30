export type ModelProvider = 'claude' | 'gemini' | 'chatgpt';

export const COORD_TOOLS = new Set<string>([
  'screenshot', 'screenshot_region', 'screenshot_window', 'ui_tree',
  'get_screen_size',
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
