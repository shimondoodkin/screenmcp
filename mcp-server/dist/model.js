"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.COORD_TOOLS = void 0;
exports.resolveModel = resolveModel;
exports.applyModelDefault = applyModelDefault;
exports.COORD_TOOLS = new Set([
    'screenshot', 'screenshot_region', 'screenshot_window', 'ui_tree',
    'get_screen_size',
    'click', 'long_click', 'drag', 'scroll', 'double_click',
    'right_click', 'middle_click', 'mouse_move', 'mouse_scroll',
]);
function resolveModel(raw) {
    return raw === 'claude' || raw === 'gemini' || raw === 'chatgpt' ? raw : null;
}
/**
 * For a coordinate-bearing command with no explicit max_width/max_height, inject the
 * connection's model so the device can pick a provider-tuned default size. Returns a new
 * params object; never mutates the input.
 */
function applyModelDefault(toolName, params, model) {
    if (model &&
        exports.COORD_TOOLS.has(toolName) &&
        params.max_width == null &&
        params.max_height == null) {
        return { ...params, model };
    }
    return params;
}
