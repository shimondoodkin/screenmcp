"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.emitEvent = emitEvent;
const http_1 = require("http");
const config_js_1 = require("./lib/config.js");
const mcp_js_1 = require("./mcp.js");
const events_1 = require("events");
const config = (0, config_js_1.loadConfig)();
// Env overrides
if (process.env.WORKER_URL)
    config.server.worker_url = process.env.WORKER_URL;
if (process.env.PORT)
    config.server.port = parseInt(process.env.PORT, 10);
if (process.env.NOTIFY_SECRET)
    config.auth.notify_secret = process.env.NOTIFY_SECRET;
console.log(`Loaded config: user=${config.user.id}, keys=${config.auth.api_keys.length}, devices=${config.devices.allowed.length}`);
// Event bus for SSE notifications
const eventBus = new events_1.EventEmitter();
eventBus.setMaxListeners(100);
function emitEvent(type, data) {
    eventBus.emit('event', { type, ...data, timestamp: Date.now() });
}
/** Strip UUID dashes so device IDs are always compared as raw hex. */
function normalizeDeviceId(id) {
    return id.replace(/-/g, '');
}
/** Verify a token against config. Accepts user_id (phones) or API keys (controllers/MCP). */
function verifyToken(token) {
    if (token === config.user.id) {
        return config.user.id;
    }
    if (config.auth.api_keys.includes(token)) {
        return config.user.id;
    }
    return null;
}
/** Extract bearer token from Authorization header. */
function extractToken(req) {
    const auth = req.headers.authorization;
    if (!auth?.startsWith('Bearer '))
        return null;
    return auth.slice(7);
}
/** Read request body as string. */
function readBody(req) {
    return new Promise((resolve, reject) => {
        const chunks = [];
        req.on('data', (chunk) => chunks.push(chunk));
        req.on('end', () => resolve(Buffer.concat(chunks).toString()));
        req.on('error', reject);
    });
}
/** Send JSON response. */
function json(res, data, status = 200) {
    const body = JSON.stringify(data);
    res.writeHead(status, {
        'Content-Type': 'application/json',
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
        'Access-Control-Allow-Headers': 'Content-Type, Authorization',
    });
    res.end(body);
}
// MCP handler (POST only, stateless like web/)
const handleMcp = (0, mcp_js_1.createMcpHandler)(config, verifyToken);
const server = (0, http_1.createServer)(async (req, res) => {
    const url = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`);
    const path = url.pathname;
    // CORS preflight
    if (req.method === 'OPTIONS') {
        res.writeHead(204, {
            'Access-Control-Allow-Origin': '*',
            'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
            'Access-Control-Allow-Headers': 'Content-Type, Authorization',
        });
        res.end();
        return;
    }
    try {
        // GET / — health check
        if (path === '/' && req.method === 'GET') {
            json(res, { status: 'ok' });
            return;
        }
        // POST /api/auth/verify — worker calls this to verify tokens
        if (path === '/api/auth/verify' && req.method === 'POST') {
            const body = JSON.parse(await readBody(req));
            const userId = verifyToken(body.token);
            if (!userId) {
                json(res, { error: 'Invalid token' }, 401);
                return;
            }
            json(res, { firebase_uid: userId, email: `${userId}@local` });
            return;
        }
        // POST /api/discover — return worker URL, notify target device via SSE
        if (path === '/api/discover' && req.method === 'POST') {
            const token = extractToken(req);
            if (!token || !verifyToken(token)) {
                json(res, { error: 'Unauthorized' }, 401);
                return;
            }
            const body = JSON.parse(await readBody(req) || '{}');
            const rawDeviceId = body.device_id || null;
            if (!rawDeviceId) {
                json(res, { error: 'device_id is required' }, 400);
                return;
            }
            const targetDeviceId = normalizeDeviceId(rawDeviceId);
            emitEvent('connect', { wsUrl: config.server.worker_url, target_device_id: targetDeviceId });
            // Also notify the worker's SSE stream so devices connected there get the event
            const workerHttpUrl = config.server.worker_url
                .replace(/^wss:/, 'https:')
                .replace(/^ws:/, 'http:');
            const notifyBody = JSON.stringify({
                type: 'connect',
                device_id: targetDeviceId,
                target_device_id: targetDeviceId,
                wsUrl: config.server.worker_url,
            });
            const notifyHeaders = { 'Content-Type': 'application/json' };
            if (config.auth.notify_secret) {
                notifyHeaders['Authorization'] = `Bearer ${config.auth.notify_secret}`;
            }
            fetch(`${workerHttpUrl}/notify`, {
                method: 'POST',
                headers: notifyHeaders,
                body: notifyBody,
            }).catch(err => console.error('Failed to notify worker:', err));
            json(res, { wsUrl: config.server.worker_url });
            return;
        }
        // POST /api/devices/register — register a device, persist to config file
        if (path === '/api/devices/register' && req.method === 'POST') {
            const token = extractToken(req);
            if (!token || !verifyToken(token)) {
                json(res, { error: 'Unauthorized' }, 401);
                return;
            }
            const body = JSON.parse(await readBody(req));
            const deviceId = normalizeDeviceId(body.deviceId || body.device_id || '');
            if (!deviceId) {
                json(res, { error: 'deviceId is required' }, 400);
                return;
            }
            const existing = config.devices.allowed.find(d => d.id === deviceId);
            if (!existing) {
                const name = body.deviceName || body.device_name || deviceId;
                config.devices.allowed.push({ id: deviceId, name });
                (0, config_js_1.saveDevices)(config);
                console.log(`Registered device: ${deviceId} (${name})`);
                emitEvent('device_registered', { device_id: deviceId, name });
            }
            json(res, { ok: true, device_number: config.devices.allowed.findIndex(d => d.id === deviceId) + 1 });
            return;
        }
        // GET /api/devices/status — list devices
        if (path === '/api/devices/status' && req.method === 'GET') {
            const token = extractToken(req);
            if (!token || !verifyToken(token)) {
                json(res, { error: 'Unauthorized' }, 401);
                return;
            }
            const deviceList = config.devices.allowed.map((d, i) => ({
                id: d.id,
                device_name: d.name,
                device_number: i + 1,
            }));
            json(res, { registered: deviceList.length > 0, devices: deviceList });
            return;
        }
        // POST /api/devices/check — check if a specific device_id is registered
        if (path === '/api/devices/check' && req.method === 'POST') {
            const token = extractToken(req);
            if (!token || !verifyToken(token)) {
                json(res, { error: 'Unauthorized' }, 401);
                return;
            }
            const body = JSON.parse(await readBody(req));
            const deviceId = normalizeDeviceId(body.deviceId || body.device_id || '');
            if (!deviceId) {
                json(res, { error: 'deviceId is required' }, 400);
                return;
            }
            const idx = config.devices.allowed.findIndex(d => d.id === deviceId);
            if (idx >= 0) {
                json(res, { registered: true, device_number: idx + 1, name: config.devices.allowed[idx].name });
            }
            else {
                json(res, { registered: false });
            }
            return;
        }
        // POST /api/devices/unregister — unregister a device, persist to config file
        if (path === '/api/devices/unregister' && req.method === 'POST') {
            const token = extractToken(req);
            if (!token || !verifyToken(token)) {
                json(res, { error: 'Unauthorized' }, 401);
                return;
            }
            const body = JSON.parse(await readBody(req));
            const deviceId = normalizeDeviceId(body.deviceId || body.device_id || '');
            const idx = config.devices.allowed.findIndex(d => d.id === deviceId);
            if (idx >= 0) {
                config.devices.allowed.splice(idx, 1);
                (0, config_js_1.saveDevices)(config);
                console.log(`Unregistered device: ${deviceId}`);
                emitEvent('device_unregistered', { device_id: deviceId });
            }
            json(res, { ok: true });
            return;
        }
        // POST /api/devices/delete — alias for unregister
        if (path === '/api/devices/delete' && req.method === 'POST') {
            const token = extractToken(req);
            if (!token || !verifyToken(token)) {
                json(res, { error: 'Unauthorized' }, 401);
                return;
            }
            const body = JSON.parse(await readBody(req));
            const deviceId = normalizeDeviceId(body.deviceId || body.device_id || '');
            const idx = config.devices.allowed.findIndex(d => d.id === deviceId);
            if (idx < 0) {
                json(res, { error: 'Device not found' }, 404);
                return;
            }
            config.devices.allowed.splice(idx, 1);
            (0, config_js_1.saveDevices)(config);
            console.log(`Deleted device: ${deviceId}`);
            emitEvent('device_deleted', { device_id: deviceId });
            json(res, { ok: true });
            return;
        }
        // POST /api/mcp — MCP Streamable HTTP
        if (path === '/api/mcp' && req.method === 'POST') {
            await handleMcp(req, res);
            return;
        }
        // GET /api/events — SSE endpoint for notifications
        if (path === '/api/events' && req.method === 'GET') {
            const token = extractToken(req);
            if (!token || !verifyToken(token)) {
                json(res, { error: 'Unauthorized' }, 401);
                return;
            }
            res.writeHead(200, {
                'Content-Type': 'text/event-stream',
                'Cache-Control': 'no-cache',
                'Connection': 'keep-alive',
                'Access-Control-Allow-Origin': '*',
            });
            // Send initial connected event
            res.write(`data: ${JSON.stringify({ type: 'connected', timestamp: Date.now() })}\n\n`);
            // Heartbeat every 30s to keep connection alive
            const heartbeat = setInterval(() => {
                res.write(': heartbeat\n\n');
            }, 30000);
            // Forward events to this SSE client
            const onEvent = (event) => {
                res.write(`data: ${JSON.stringify(event)}\n\n`);
            };
            eventBus.on('event', onEvent);
            // Cleanup on disconnect
            req.on('close', () => {
                clearInterval(heartbeat);
                eventBus.off('event', onEvent);
            });
            return;
        }
        // 404
        json(res, { error: 'Not found' }, 404);
    }
    catch (err) {
        console.error('Request error:', err);
        json(res, { error: 'Internal server error' }, 500);
    }
});
const port = config.server.port;
server.listen(port, () => {
    console.log(`MCP server listening on :${port}`);
    console.log(`Worker URL: ${config.server.worker_url}`);
    console.log(`SSE events: http://localhost:${port}/api/events`);
});
