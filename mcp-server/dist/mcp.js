"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.createMcpHandler = createMcpHandler;
const mcp_js_1 = require("@modelcontextprotocol/sdk/server/mcp.js");
const streamableHttp_js_1 = require("@modelcontextprotocol/sdk/server/streamableHttp.js");
const sdk_1 = require("@screenmcp/sdk");
const model_js_1 = require("./model.js");
const zod_1 = require("zod");
// Documented model param for the screenshot-family tool schemas (input commands accept
// it silently via server-side injection).
const modelParam = zod_1.z.enum(['claude', 'gemini', 'chatgpt']).optional()
    .describe('Consumer model; sets a provider-tuned default screenshot size when max_width/max_height are omitted. Normally supplied by the connection ?model= param.');
// Per-device SDK connections (pooled like cloud MCP server)
const deviceConnections = new Map();
// Common device_id parameter added to every phone tool
const deviceIdParam = zod_1.z.number().int().describe('Device ID number. Use list_devices to see available devices.');
// Optional coordinate scaling params — device applies defaults based on its own screen ratio
const scalingParams = {
    max_width: zod_1.z.number().int().optional().describe('Screenshot width for coordinate auto-scaling (device applies default if omitted, 0 to disable)'),
    max_height: zod_1.z.number().int().optional().describe('Screenshot height for coordinate auto-scaling (device applies default if omitted, 0 to disable)'),
};
// MCP tools for phone control — descriptions match web/ exactly
const phoneTools = [
    {
        name: 'screenshot',
        description: 'Take a screenshot of the phone screen. Returns base64 WebP image.',
        inputSchema: {
            device_id: deviceIdParam,
            quality: zod_1.z.number().min(1).max(100).optional().describe('Image quality 1-100 (default: 100 = lossless)'),
            max_width: zod_1.z.number().optional().describe('Max width for scaling'),
            max_height: zod_1.z.number().optional().describe('Max height for scaling'),
            model: modelParam,
        },
        handler: async (phone, params) => {
            const res = await phone.sendCommand('screenshot', params);
            return res.result;
        },
    },
    {
        name: 'ui_tree',
        description: 'Get the accessibility tree of the current screen. Supports scoping to one window, filtering by control type / text / region, capping depth, and a flat output shape with precomputed center coordinates.',
        inputSchema: {
            device_id: deviceIdParam,
            ...scalingParams,
            model: modelParam,
            window: zod_1.z.union([zod_1.z.string(), zod_1.z.number()]).optional()
                .describe('Title substring (string) or hwnd (number). Scopes to one top-level window. Windows only.'),
            region: zod_1.z.object({
                min_x: zod_1.z.number().int(),
                min_y: zod_1.z.number().int(),
                max_x: zod_1.z.number().int(),
                max_y: zod_1.z.number().int(),
            }).optional().describe('Filter to nodes whose bounds match this rect (in screenshot space). Windows only.'),
            region_mode: zod_1.z.enum(['inside', 'intersect']).optional()
                .describe('"inside" (default): node bounds fully inside region. "intersect": any overlap.'),
            types: zod_1.z.array(zod_1.z.string()).optional()
                .describe('Whitelist of controlType values, case-insensitive (e.g. ["Button","Edit","MenuItem"]). Windows only.'),
            text_match: zod_1.z.string().optional()
                .describe('Filter on text. Substring (case-insensitive) by default; regex if regex=true. Windows only.'),
            regex: zod_1.z.boolean().optional()
                .describe('If true, text_match is a regex. Default false.'),
            max_depth: zod_1.z.number().int().min(1).optional()
                .describe('Cap recursion depth (default 10). Windows only.'),
            format: zod_1.z.enum(['nested', 'flat']).optional()
                .describe('"nested" (default): tree shape, byte-compatible with legacy output. "flat": array of {controlType,text,cx,cy,hwnd,path}.'),
            fields: zod_1.z.array(zod_1.z.string()).optional()
                .describe('Per-node fields to emit. Available: text, value, controlType, className, resourceId, contentDescription, bounds, cx, cy, enabled, clickable, editable, scrollable, checked, focused, hwnd, path. controlType is always included.'),
        },
        handler: async (phone, params) => {
            const res = await phone.sendCommand('ui_tree', params);
            return res.result;
        },
    },
    {
        name: 'click',
        description: 'Tap on the screen at coordinates',
        inputSchema: {
            device_id: deviceIdParam,
            x: zod_1.z.number().describe('X coordinate'),
            y: zod_1.z.number().describe('Y coordinate'),
            duration: zod_1.z.number().optional().describe('Press duration in ms (default: 100)'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('click', params)).result;
        },
    },
    {
        name: 'long_click',
        description: 'Long press at coordinates (1000ms)',
        inputSchema: {
            device_id: deviceIdParam,
            x: zod_1.z.number().describe('X coordinate'),
            y: zod_1.z.number().describe('Y coordinate'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('long_click', params)).result;
        },
    },
    {
        name: 'scroll',
        description: 'Scroll the screen with a finger-drag gesture',
        inputSchema: {
            device_id: deviceIdParam,
            x: zod_1.z.number().describe('Start X'),
            y: zod_1.z.number().describe('Start Y'),
            dx: zod_1.z.number().describe('Horizontal delta'),
            dy: zod_1.z.number().describe('Vertical delta (negative = scroll content up)'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('scroll', params)).result;
        },
    },
    {
        name: 'drag',
        description: 'Drag from one point to another',
        inputSchema: {
            device_id: deviceIdParam,
            startX: zod_1.z.number(),
            startY: zod_1.z.number(),
            endX: zod_1.z.number(),
            endY: zod_1.z.number(),
            duration: zod_1.z.number().optional().describe('Duration in ms (default: 300)'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('drag', params)).result;
        },
    },
    {
        name: 'type',
        description: 'Type text into the currently focused input field',
        inputSchema: {
            device_id: deviceIdParam,
            text: zod_1.z.string().describe('Text to type'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('type', params)).result;
        },
    },
    {
        name: 'get_text',
        description: 'Get text from the currently focused input field',
        inputSchema: { device_id: deviceIdParam },
        handler: async (phone) => {
            return (await phone.sendCommand('get_text')).result;
        },
    },
    {
        name: 'select_all',
        description: 'Select all text in the focused field',
        inputSchema: { device_id: deviceIdParam },
        handler: async (phone) => {
            return (await phone.sendCommand('select_all')).result;
        },
    },
    {
        name: 'copy',
        description: 'Copy selected text. Optionally return the copied text.',
        inputSchema: {
            device_id: deviceIdParam,
            return_text: zod_1.z.boolean().optional().describe('If true, return the copied text in the response (default: false)'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('copy', params)).result;
        },
    },
    {
        name: 'paste',
        description: 'Paste into the focused field. Optionally set clipboard contents before pasting.',
        inputSchema: {
            device_id: deviceIdParam,
            text: zod_1.z.string().optional().describe('Text to set in clipboard before pasting. If omitted, pastes current clipboard contents.'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('paste', params)).result;
        },
    },
    {
        name: 'get_clipboard',
        description: 'Get the current clipboard text contents.',
        inputSchema: { device_id: deviceIdParam },
        handler: async (phone) => {
            return (await phone.sendCommand('get_clipboard')).result;
        },
    },
    {
        name: 'set_clipboard',
        description: 'Set the clipboard to the given text.',
        inputSchema: {
            device_id: deviceIdParam,
            text: zod_1.z.string().describe('Text to put in the clipboard'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('set_clipboard', params)).result;
        },
    },
    {
        name: 'back',
        description: 'Press the back button',
        inputSchema: { device_id: deviceIdParam },
        handler: async (phone) => {
            return (await phone.sendCommand('back')).result;
        },
    },
    {
        name: 'home',
        description: 'Press the home button',
        inputSchema: { device_id: deviceIdParam },
        handler: async (phone) => {
            return (await phone.sendCommand('home')).result;
        },
    },
    {
        name: 'recents',
        description: 'Open the recent apps view',
        inputSchema: { device_id: deviceIdParam },
        handler: async (phone) => {
            return (await phone.sendCommand('recents')).result;
        },
    },
    {
        name: 'list_cameras',
        description: 'List available cameras on the device. Returns camera IDs with facing direction (back/front/external). Use before camera to discover valid IDs.',
        inputSchema: { device_id: deviceIdParam },
        handler: async (phone) => {
            return (await phone.sendCommand('list_cameras')).result;
        },
    },
    {
        name: 'camera',
        description: 'Take a photo with the phone camera',
        inputSchema: {
            device_id: deviceIdParam,
            camera: zod_1.z.string().optional().describe('Camera ID (use list_cameras to discover available IDs). Default: "0"'),
            quality: zod_1.z.number().min(1).max(100).optional().describe('Image quality (default: 80)'),
            max_width: zod_1.z.number().optional(),
            max_height: zod_1.z.number().optional(),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('camera', params)).result;
        },
    },
    {
        name: 'hold_key',
        description: 'Press and hold a key (desktop only). Use with release_key for multi-key sequences like Alt+Tab.',
        inputSchema: {
            device_id: deviceIdParam,
            key: zod_1.z.string().describe('Key name: shift, ctrl, alt, meta/cmd, tab, enter, escape, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, f1-f12, or a single character'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('hold_key', params)).result;
        },
    },
    {
        name: 'release_key',
        description: 'Release a held key (desktop only). Use after hold_key.',
        inputSchema: {
            device_id: deviceIdParam,
            key: zod_1.z.string().describe('Key name to release'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('release_key', params)).result;
        },
    },
    {
        name: 'press_key',
        description: 'Press and release a key in one action (desktop only). For modifier combos, use hold_key/release_key instead.',
        inputSchema: {
            device_id: deviceIdParam,
            key: zod_1.z.string().describe('Key name: shift, ctrl, alt, meta/cmd, tab, enter, escape, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, f1-f12, or a single character'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('press_key', params)).result;
        },
    },
    {
        name: 'right_click',
        description: 'Right-click at coordinates (desktop only). Returns unsupported on Android.',
        inputSchema: {
            device_id: deviceIdParam,
            x: zod_1.z.number().describe('X coordinate'),
            y: zod_1.z.number().describe('Y coordinate'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('right_click', params)).result;
        },
    },
    {
        name: 'middle_click',
        description: 'Middle-click at coordinates (desktop only). Returns unsupported on Android.',
        inputSchema: {
            device_id: deviceIdParam,
            x: zod_1.z.number().describe('X coordinate'),
            y: zod_1.z.number().describe('Y coordinate'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('middle_click', params)).result;
        },
    },
    {
        name: 'mouse_scroll',
        description: 'Raw mouse scroll at coordinates with pixel deltas (desktop only). Returns unsupported on Android.',
        inputSchema: {
            device_id: deviceIdParam,
            x: zod_1.z.number().describe('X coordinate'),
            y: zod_1.z.number().describe('Y coordinate'),
            dx: zod_1.z.number().describe('Horizontal delta'),
            dy: zod_1.z.number().describe('Vertical delta'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('mouse_scroll', params)).result;
        },
    },
    {
        name: 'play_audio',
        description: 'Play an audio file (WAV or MP3) on the device speaker',
        inputSchema: {
            device_id: deviceIdParam,
            audio_data: zod_1.z.string().describe('Base64-encoded audio file (WAV or MP3)'),
            volume: zod_1.z.number().min(0).max(1).optional().describe('Playback volume'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('play_audio', params)).result;
        },
    },
    {
        name: 'mouse_move',
        description: 'Move the mouse cursor without clicking (desktop only)',
        inputSchema: {
            device_id: deviceIdParam,
            x: zod_1.z.number().describe('X coordinate'),
            y: zod_1.z.number().describe('Y coordinate'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('mouse_move', params)).result;
        },
    },
    {
        name: 'double_click',
        description: 'Double-click at coordinates (desktop: two clicks, Android: two rapid taps)',
        inputSchema: {
            device_id: deviceIdParam,
            x: zod_1.z.number().describe('X coordinate'),
            y: zod_1.z.number().describe('Y coordinate'),
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('double_click', params)).result;
        },
    },
    {
        name: 'hotkey',
        description: 'Press a key combination atomically, e.g. ["ctrl","c"] for copy (desktop only)',
        inputSchema: {
            device_id: deviceIdParam,
            keys: zod_1.z.array(zod_1.z.string()).describe('Array of key names to press together'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('hotkey', params)).result;
        },
    },
    {
        name: 'get_screen_size',
        description: 'Get the primary display dimensions. With max_width/max_height, returns scaled dimensions plus originals.',
        inputSchema: {
            device_id: deviceIdParam,
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('get_screen_size', params)).result;
        },
    },
    {
        name: 'list_windows',
        description: 'List all visible windows with titles and positions (desktop only). Coordinates scaled when max_width/max_height set.',
        inputSchema: {
            device_id: deviceIdParam,
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('list_windows', params)).result;
        },
    },
    {
        name: 'focus_window',
        description: 'Bring a window to the foreground by title substring or index (desktop only)',
        inputSchema: {
            device_id: deviceIdParam,
            title: zod_1.z.string().optional().describe('Window title substring (case-insensitive)'),
            index: zod_1.z.number().int().optional().describe('Window index from list_windows'),
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('focus_window', params)).result;
        },
    },
    {
        name: 'active_window',
        description: 'Get information about the currently focused window (desktop only)',
        inputSchema: {
            device_id: deviceIdParam,
            ...scalingParams,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('active_window', params)).result;
        },
    },
    {
        name: 'screenshot_window',
        description: 'Capture a specific window by title or index without focusing it (desktop only)',
        inputSchema: {
            device_id: deviceIdParam,
            title: zod_1.z.string().optional().describe('Window title substring'),
            index: zod_1.z.number().int().optional().describe('Window index from list_windows'),
            max_width: zod_1.z.number().int().optional().describe('Max width in pixels'),
            max_height: zod_1.z.number().int().optional().describe('Max height in pixels'),
            model: modelParam,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('screenshot_window', params)).result;
        },
    },
    {
        name: 'screenshot_region',
        description: 'Capture a region of the screen. Returns base64 WebP image of the specified rectangular area (desktop only).',
        inputSchema: {
            device_id: deviceIdParam,
            min_x: zod_1.z.number().describe('Left edge X coordinate'),
            min_y: zod_1.z.number().describe('Top edge Y coordinate'),
            max_x: zod_1.z.number().describe('Right edge X coordinate'),
            max_y: zod_1.z.number().describe('Bottom edge Y coordinate'),
            quality: zod_1.z.number().int().optional().describe('Image quality 1-100'),
            output_max_width: zod_1.z.number().int().optional().describe('Max output width in pixels'),
            output_max_height: zod_1.z.number().int().optional().describe('Max output height in pixels'),
            ...scalingParams,
            model: modelParam,
        },
        handler: async (phone, params) => {
            return (await phone.sendCommand('screenshot_region', params)).result;
        },
    },
    {
        name: 'is_elevated',
        description: 'Check if the process has elevated/admin privileges (desktop only)',
        inputSchema: {
            device_id: deviceIdParam,
        },
        handler: async (phone) => {
            return (await phone.sendCommand('is_elevated')).result;
        },
    },
    {
        name: 'elevate',
        description: 'Request administrator/root privileges with user confirmation (desktop only)',
        inputSchema: {
            device_id: deviceIdParam,
        },
        handler: async (phone) => {
            return (await phone.sendCommand('elevate')).result;
        },
    },
];
function createMcpHandler(config, verifyToken) {
    return async (req, res) => {
        // Auth
        const authHeader = req.headers.authorization;
        if (!authHeader?.startsWith('Bearer ')) {
            res.writeHead(401, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Missing authorization' }));
            return;
        }
        const token = authHeader.slice(7);
        const userId = verifyToken(token);
        if (!userId) {
            res.writeHead(401, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Invalid token' }));
            return;
        }
        // Per-connection consumer model from ?model= on the MCP URL
        const reqUrl = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`);
        const model = (0, model_js_1.resolveModel)(reqUrl.searchParams.get('model'));
        // Create MCP server per request (stateless, same as web/)
        const server = new mcp_js_1.McpServer({
            name: 'ScreenMCP',
            version: '1.0.0',
        });
        // Resolve device_number (1-based) to hex device ID from config
        const resolveDeviceId = (deviceNumber) => {
            const index = deviceNumber - 1;
            if (index < 0 || index >= config.devices.allowed.length) {
                throw new Error(`Device not found: device_id ${deviceNumber}. Use list_devices to see available devices.`);
            }
            return config.devices.allowed[index].id;
        };
        // Get or create SDK device connection for a device
        const getPhone = async (targetDeviceId) => {
            let conn = deviceConnections.get(targetDeviceId);
            if (conn && conn.connected) {
                return conn;
            }
            const client = new sdk_1.ScreenMCPClient({
                apiKey: config.auth.api_keys[0],
                apiUrl: `http://localhost:${config.server.port}`,
                commandTimeout: 30_000,
                autoReconnect: false,
            });
            conn = await client.connect({ deviceId: targetDeviceId });
            deviceConnections.set(targetDeviceId, conn);
            return conn;
        };
        // list_devices — reads from config file [devices].allowed, numbered by position
        server.tool('list_devices', 'List all devices registered to your account. Returns device_id numbers needed for other tools.', {}, async () => {
            const deviceList = config.devices.allowed.map((dev, i) => ({
                device_id: i + 1,
                name: dev.name,
            }));
            return {
                content: [{ type: 'text', text: JSON.stringify({ devices: deviceList }, null, 2) }],
            };
        });
        // Register all phone tools
        for (const tool of phoneTools) {
            server.tool(tool.name, tool.description, tool.inputSchema, async (params) => {
                try {
                    // Resolve device_id number to hex device ID
                    const deviceNumber = params.device_id;
                    const deviceId = resolveDeviceId(deviceNumber);
                    const p = await getPhone(deviceId);
                    const { device_id: _, ...rest } = params;
                    const phoneParams = (0, model_js_1.applyModelDefault)(tool.name, rest, model);
                    const result = await tool.handler(p, phoneParams);
                    return {
                        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
                    };
                }
                catch (error) {
                    return {
                        content: [{ type: 'text', text: `Error: ${error instanceof Error ? error.message : String(error)}` }],
                        isError: true,
                    };
                }
            });
        }
        // Stateless transport (same as web/ — new transport per request)
        const transport = new streamableHttp_js_1.StreamableHTTPServerTransport({
            sessionIdGenerator: undefined,
        });
        await server.connect(transport);
        await transport.handleRequest(req, res);
        await transport.close();
        await server.close();
    };
}
