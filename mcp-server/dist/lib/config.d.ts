export interface DeviceEntry {
    id: string;
    name: string;
}
export interface Config {
    configPath: string;
    user: {
        id: string;
    };
    auth: {
        api_keys: string[];
        notify_secret?: string;
    };
    devices: {
        allowed: DeviceEntry[];
    };
    server: {
        port: number;
        worker_url: string;
    };
}
/**
 * Load config from ~/.screenmcp/worker.toml (same file the worker uses).
 * Extra [server] section for mcp-server specific settings.
 */
export declare function loadConfig(configPath?: string): Config;
/**
 * Save the [devices].allowed list back to the config file.
 * Preserves everything else in the file — only rewrites the allowed = [...] line.
 */
export declare function saveDevices(config: Config): void;
