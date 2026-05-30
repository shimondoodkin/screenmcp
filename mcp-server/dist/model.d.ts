export type ModelProvider = 'claude' | 'gemini' | 'chatgpt';
export declare const COORD_TOOLS: Set<string>;
export declare function resolveModel(raw: string | null | undefined): ModelProvider | null;
/**
 * For a coordinate-bearing command with no explicit max_width/max_height, inject the
 * connection's model so the device can pick a provider-tuned default size. Returns a new
 * params object; never mutates the input.
 */
export declare function applyModelDefault(toolName: string, params: Record<string, unknown>, model: ModelProvider | null): Record<string, unknown>;
