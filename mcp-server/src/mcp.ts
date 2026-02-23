import { IncomingMessage, ServerResponse } from 'http';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/streamableHttp.js';
import { PhoneConnection } from './lib/phone-connection.js';
import { Config } from './lib/config.js';
import { z } from 'zod';

// Per-device phone connections
const phoneConnections = new Map<string, PhoneConnection>();

// Common device_id parameter added to every phone tool
const deviceIdParam = z.number().int().describe('Device ID number. Use list_devices to see available devices.');

// MCP tools for phone control — descriptions match web/ exactly
const phoneTools = [
  {
    name: 'screenshot',
    description: 'Take a screenshot of the phone screen. Returns base64 WebP image.',
    inputSchema: {
      device_id: deviceIdParam,
      quality: z.number().min(1).max(100).optional().describe('Image quality 1-100 (default: 100 = lossless)'),
      max_width: z.number().optional().describe('Max width for scaling'),
      max_height: z.number().optional().describe('Max height for scaling'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      const res = await phone.sendCommand('screenshot', params);
      return res.result;
    },
  },
  {
    name: 'ui_tree',
    description: 'Get the accessibility tree of the current screen. Returns array of UI nodes with bounds, text, clickable state, etc.',
    inputSchema: { device_id: deviceIdParam },
    handler: async (phone: PhoneConnection) => {
      const res = await phone.sendCommand('ui_tree');
      return res.result;
    },
  },
  {
    name: 'click',
    description: 'Tap on the screen at coordinates',
    inputSchema: {
      device_id: deviceIdParam,
      x: z.number().int().describe('X coordinate'),
      y: z.number().int().describe('Y coordinate'),
      duration: z.number().optional().describe('Press duration in ms (default: 100)'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('click', params)).result;
    },
  },
  {
    name: 'long_click',
    description: 'Long press at coordinates (1000ms)',
    inputSchema: {
      device_id: deviceIdParam,
      x: z.number().int().describe('X coordinate'),
      y: z.number().int().describe('Y coordinate'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('long_click', params)).result;
    },
  },
  {
    name: 'scroll',
    description: 'Scroll the screen with a finger-drag gesture',
    inputSchema: {
      device_id: deviceIdParam,
      x: z.number().int().describe('Start X'),
      y: z.number().int().describe('Start Y'),
      dx: z.number().int().describe('Horizontal delta'),
      dy: z.number().int().describe('Vertical delta (negative = scroll content up)'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('scroll', params)).result;
    },
  },
  {
    name: 'drag',
    description: 'Drag from one point to another',
    inputSchema: {
      device_id: deviceIdParam,
      startX: z.number().int(),
      startY: z.number().int(),
      endX: z.number().int(),
      endY: z.number().int(),
      duration: z.number().optional().describe('Duration in ms (default: 300)'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('drag', params)).result;
    },
  },
  {
    name: 'type',
    description: 'Type text into the currently focused input field',
    inputSchema: {
      device_id: deviceIdParam,
      text: z.string().describe('Text to type'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('type', params)).result;
    },
  },
  {
    name: 'get_text',
    description: 'Get text from the currently focused input field',
    inputSchema: { device_id: deviceIdParam },
    handler: async (phone: PhoneConnection) => {
      return (await phone.sendCommand('get_text')).result;
    },
  },
  {
    name: 'select_all',
    description: 'Select all text in the focused field',
    inputSchema: { device_id: deviceIdParam },
    handler: async (phone: PhoneConnection) => {
      return (await phone.sendCommand('select_all')).result;
    },
  },
  {
    name: 'copy',
    description: 'Copy selected text. Optionally return the copied text.',
    inputSchema: {
      device_id: deviceIdParam,
      return_text: z.boolean().optional().describe('If true, return the copied text in the response (default: false)'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('copy', params)).result;
    },
  },
  {
    name: 'paste',
    description: 'Paste into the focused field. Optionally set clipboard contents before pasting.',
    inputSchema: {
      device_id: deviceIdParam,
      text: z.string().optional().describe('Text to set in clipboard before pasting. If omitted, pastes current clipboard contents.'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('paste', params)).result;
    },
  },
  {
    name: 'get_clipboard',
    description: 'Get the current clipboard text contents.',
    inputSchema: { device_id: deviceIdParam },
    handler: async (phone: PhoneConnection) => {
      return (await phone.sendCommand('get_clipboard')).result;
    },
  },
  {
    name: 'set_clipboard',
    description: 'Set the clipboard to the given text.',
    inputSchema: {
      device_id: deviceIdParam,
      text: z.string().describe('Text to put in the clipboard'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('set_clipboard', params)).result;
    },
  },
  {
    name: 'back',
    description: 'Press the back button',
    inputSchema: { device_id: deviceIdParam },
    handler: async (phone: PhoneConnection) => {
      return (await phone.sendCommand('back')).result;
    },
  },
  {
    name: 'home',
    description: 'Press the home button',
    inputSchema: { device_id: deviceIdParam },
    handler: async (phone: PhoneConnection) => {
      return (await phone.sendCommand('home')).result;
    },
  },
  {
    name: 'recents',
    description: 'Open the recent apps view',
    inputSchema: { device_id: deviceIdParam },
    handler: async (phone: PhoneConnection) => {
      return (await phone.sendCommand('recents')).result;
    },
  },
  {
    name: 'list_cameras',
    description: 'List available cameras on the device. Returns camera IDs with facing direction (back/front/external). Use before camera to discover valid IDs.',
    inputSchema: { device_id: deviceIdParam },
    handler: async (phone: PhoneConnection) => {
      return (await phone.sendCommand('list_cameras')).result;
    },
  },
  {
    name: 'camera',
    description: 'Take a photo with the phone camera',
    inputSchema: {
      device_id: deviceIdParam,
      camera: z.string().optional().describe('Camera ID (use list_cameras to discover available IDs). Default: "0"'),
      quality: z.number().min(1).max(100).optional().describe('Image quality (default: 80)'),
      max_width: z.number().optional(),
      max_height: z.number().optional(),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('camera', params)).result;
    },
  },
  {
    name: 'hold_key',
    description: 'Press and hold a key (desktop only). Use with release_key for multi-key sequences like Alt+Tab.',
    inputSchema: {
      device_id: deviceIdParam,
      key: z.string().describe('Key name: shift, ctrl, alt, meta/cmd, tab, enter, escape, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, f1-f12, or a single character'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('hold_key', params)).result;
    },
  },
  {
    name: 'release_key',
    description: 'Release a held key (desktop only). Use after hold_key.',
    inputSchema: {
      device_id: deviceIdParam,
      key: z.string().describe('Key name to release'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('release_key', params)).result;
    },
  },
  {
    name: 'press_key',
    description: 'Press and release a key in one action (desktop only). For modifier combos, use hold_key/release_key instead.',
    inputSchema: {
      device_id: deviceIdParam,
      key: z.string().describe('Key name: shift, ctrl, alt, meta/cmd, tab, enter, escape, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, f1-f12, or a single character'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('press_key', params)).result;
    },
  },
  {
    name: 'right_click',
    description: 'Right-click at coordinates (desktop only). Returns unsupported on Android.',
    inputSchema: {
      device_id: deviceIdParam,
      x: z.number().int().describe('X coordinate'),
      y: z.number().int().describe('Y coordinate'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('right_click', params)).result;
    },
  },
  {
    name: 'middle_click',
    description: 'Middle-click at coordinates (desktop only). Returns unsupported on Android.',
    inputSchema: {
      device_id: deviceIdParam,
      x: z.number().int().describe('X coordinate'),
      y: z.number().int().describe('Y coordinate'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('middle_click', params)).result;
    },
  },
  {
    name: 'mouse_scroll',
    description: 'Raw mouse scroll at coordinates with pixel deltas (desktop only). Returns unsupported on Android.',
    inputSchema: {
      device_id: deviceIdParam,
      x: z.number().int().describe('X coordinate'),
      y: z.number().int().describe('Y coordinate'),
      dx: z.number().int().describe('Horizontal delta'),
      dy: z.number().int().describe('Vertical delta'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('mouse_scroll', params)).result;
    },
  },
  {
    name: 'play_audio',
    description: 'Play an audio file (WAV or MP3) on the device speaker',
    inputSchema: {
      device_id: deviceIdParam,
      audio_data: z.string().describe('Base64-encoded audio file (WAV or MP3)'),
      volume: z.number().min(0).max(1).optional().describe('Playback volume'),
    },
    handler: async (phone: PhoneConnection, params: Record<string, unknown>) => {
      return (await phone.sendCommand('play_audio', params)).result;
    },
  },
];

export function createMcpHandler(
  config: Config,
  verifyToken: (key: string) => string | null,
) {
  return async (req: IncomingMessage, res: ServerResponse) => {
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

    // Create MCP server per request (stateless, same as web/)
    const server = new McpServer({
      name: 'ScreenMCP',
      version: '1.0.0',
    });

    // Resolve device_number (1-based) to hex device ID from config
    const resolveDeviceId = (deviceNumber: number): string => {
      const index = deviceNumber - 1;
      if (index < 0 || index >= config.devices.allowed.length) {
        throw new Error(`Device not found: device_id ${deviceNumber}. Use list_devices to see available devices.`);
      }
      return config.devices.allowed[index].id;
    };

    // Get or create phone connection for a device
    const getPhone = async (targetDeviceId: string) => {
      let phone = phoneConnections.get(targetDeviceId);
      if (!phone) {
        phone = new PhoneConnection();
        await phone.connect(config.server.worker_url, token, targetDeviceId);
        phoneConnections.set(targetDeviceId, phone);
      }
      return phone;
    };

    // list_devices — reads from config file [devices].allowed, numbered by position
    server.tool(
      'list_devices',
      'List all devices registered to your account. Returns device_id numbers needed for other tools.',
      {},
      async () => {
        const deviceList = config.devices.allowed.map((dev, i) => ({
          device_id: i + 1,
          name: dev.name,
        }));
        return {
          content: [{ type: 'text' as const, text: JSON.stringify({ devices: deviceList }, null, 2) }],
        };
      }
    );

    // Register all phone tools
    for (const tool of phoneTools) {
      server.tool(
        tool.name,
        tool.description,
        tool.inputSchema as unknown as Record<string, z.ZodTypeAny>,
        async (params: Record<string, unknown>) => {
          try {
            // Resolve device_id number to hex device ID
            const deviceNumber = params.device_id as number;
            const deviceId = resolveDeviceId(deviceNumber);

            const p = await getPhone(deviceId);
            const { device_id: _, ...phoneParams } = params;
            const result = await tool.handler(p, phoneParams);
            return {
              content: [{ type: 'text' as const, text: JSON.stringify(result, null, 2) }],
            };
          } catch (error) {
            return {
              content: [{ type: 'text' as const, text: `Error: ${error instanceof Error ? error.message : String(error)}` }],
              isError: true,
            };
          }
        }
      );
    }

    // Stateless transport (same as web/ — new transport per request)
    const transport = new StreamableHTTPServerTransport({
      sessionIdGenerator: undefined as unknown as (() => string),
    });

    await server.connect(transport);
    await transport.handleRequest(req, res);
    await transport.close();
    await server.close();
  };
}
