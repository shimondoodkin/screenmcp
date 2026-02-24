"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.loadConfig = loadConfig;
exports.saveDevices = saveDevices;
const fs_1 = require("fs");
const path_1 = require("path");
/**
 * Load config from ~/.screenmcp/worker.toml (same file the worker uses).
 * Extra [server] section for mcp-server specific settings.
 */
function loadConfig(configPath) {
    const path = configPath
        || process.env.SCREENMCP_CONFIG
        || (0, path_1.resolve)(process.env.HOME || '.', '.screenmcp/worker.toml');
    let raw;
    try {
        raw = (0, fs_1.readFileSync)(path, 'utf-8');
    }
    catch {
        console.error(`Config file not found at ${path}`);
        console.error('Create ~/.screenmcp/worker.toml with at least [user] and [auth] sections.');
        process.exit(1);
    }
    return parseTOML(raw, path);
}
/**
 * Save the [devices].allowed list back to the config file.
 * Preserves everything else in the file — only rewrites the allowed = [...] line.
 */
function saveDevices(config) {
    const path = config.configPath;
    let raw;
    try {
        raw = (0, fs_1.readFileSync)(path, 'utf-8');
    }
    catch {
        raw = '';
    }
    // Build the new allowed line
    const entries = config.devices.allowed.map(d => {
        const val = d.name !== d.id ? `${d.id} ${d.name}` : d.id;
        return `"${val}"`;
    });
    const newAllowed = `allowed = [${entries.join(', ')}]`;
    // Try to replace existing allowed = [...] line under [devices]
    const lines = raw.split('\n');
    let inDevices = false;
    let replaced = false;
    for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();
        if (trimmed.match(/^\[([^\]]+)\]$/)) {
            inDevices = trimmed === '[devices]';
        }
        if (inDevices && trimmed.startsWith('allowed')) {
            lines[i] = newAllowed;
            replaced = true;
            break;
        }
    }
    if (!replaced) {
        // No [devices] section or no allowed line — append it
        const hasDevicesSection = lines.some(l => l.trim() === '[devices]');
        if (hasDevicesSection) {
            const idx = lines.findIndex(l => l.trim() === '[devices]');
            lines.splice(idx + 1, 0, newAllowed);
        }
        else {
            lines.push('', '[devices]', newAllowed);
        }
    }
    (0, fs_1.writeFileSync)(path, lines.join('\n'));
    console.log(`Saved ${config.devices.allowed.length} devices to ${path}`);
}
function parseTOML(raw, path) {
    const config = {
        configPath: path,
        user: { id: 'local-user' },
        auth: { api_keys: [] },
        devices: { allowed: [] },
        server: { port: 3000, worker_url: 'ws://localhost:8080' },
    };
    let currentSection = '';
    for (const line of raw.split('\n')) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('#'))
            continue;
        const sectionMatch = trimmed.match(/^\[([^\]]+)\]$/);
        if (sectionMatch) {
            currentSection = sectionMatch[1].trim();
            continue;
        }
        const kvMatch = trimmed.match(/^(\w+)\s*=\s*(.+)$/);
        if (!kvMatch)
            continue;
        const [, key, rawVal] = kvMatch;
        const val = parseValue(rawVal);
        switch (currentSection) {
            case 'user':
                if (key === 'id')
                    config.user.id = String(val);
                break;
            case 'auth':
                if (key === 'api_keys' && Array.isArray(val))
                    config.auth.api_keys = val;
                if (key === 'notify_secret')
                    config.auth.notify_secret = String(val);
                break;
            case 'devices':
                if (key === 'allowed' && Array.isArray(val)) {
                    config.devices.allowed = val.map(entry => {
                        const spaceIdx = entry.indexOf(' ');
                        if (spaceIdx === -1)
                            return { id: entry, name: entry };
                        return { id: entry.slice(0, spaceIdx), name: entry.slice(spaceIdx + 1) };
                    });
                }
                break;
            case 'server':
                if (key === 'port')
                    config.server.port = Number(val);
                if (key === 'worker_url')
                    config.server.worker_url = String(val);
                break;
        }
    }
    if (config.auth.api_keys.length === 0) {
        console.error(`No API keys found in ${path} — add [auth] api_keys = ["pk_..."]`);
        process.exit(1);
    }
    return config;
}
function parseValue(raw) {
    const trimmed = raw.trim();
    // Array
    if (trimmed.startsWith('[')) {
        const inner = trimmed.slice(1, trimmed.lastIndexOf(']'));
        return inner.split(',').map(s => {
            const t = s.trim();
            if ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'")))
                return t.slice(1, -1);
            return t;
        }).filter(s => s.length > 0);
    }
    // String
    if ((trimmed.startsWith('"') && trimmed.endsWith('"')) || (trimmed.startsWith("'") && trimmed.endsWith("'")))
        return trimmed.slice(1, -1);
    // Boolean
    if (trimmed === 'true')
        return true;
    if (trimmed === 'false')
        return false;
    // Number
    if (/^\d+$/.test(trimmed))
        return parseInt(trimmed, 10);
    return trimmed;
}
