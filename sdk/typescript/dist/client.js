"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.PhoneMCPClient = void 0;
const ws_1 = __importDefault(require("ws"));
const events_1 = require("events");
const DEFAULT_API_URL = "https://server10.doodkin.com";
/**
 * PhoneMCP SDK client.
 *
 * Connects to the PhoneMCP infrastructure (API server + worker relay) and
 * provides typed methods for every supported phone command.
 *
 * ```ts
 * const phone = new PhoneMCPClient({ apiKey: "pk_..." });
 * await phone.connect();
 * const { image } = await phone.screenshot();
 * await phone.click(540, 1200);
 * await phone.disconnect();
 * ```
 */
class PhoneMCPClient extends events_1.EventEmitter {
    ws = null;
    apiKey;
    apiUrl;
    deviceId;
    commandTimeout;
    autoReconnect;
    pending = new Map();
    _lastTempId = 0;
    _phoneConnected = false;
    _workerUrl = null;
    _connected = false;
    constructor(options) {
        super();
        this.apiKey = options.apiKey;
        this.apiUrl = (options.apiUrl ?? DEFAULT_API_URL).replace(/\/+$/, "");
        this.deviceId = options.deviceId;
        this.commandTimeout = options.commandTimeout ?? 30_000;
        this.autoReconnect = options.autoReconnect ?? true;
    }
    // -----------------------------------------------------------------------
    // Public getters
    // -----------------------------------------------------------------------
    /** Whether the target phone is currently connected to the worker. */
    get phoneConnected() {
        return this._phoneConnected;
    }
    /** The worker WebSocket URL currently in use. */
    get workerUrl() {
        return this._workerUrl;
    }
    /** Whether the client is connected to the worker. */
    get connected() {
        return this._connected;
    }
    // -----------------------------------------------------------------------
    // Typed event emitter overrides
    // -----------------------------------------------------------------------
    on(event, listener) {
        return super.on(event, listener);
    }
    once(event, listener) {
        return super.once(event, listener);
    }
    emit(event, ...args) {
        return super.emit(event, ...args);
    }
    // -----------------------------------------------------------------------
    // Connection lifecycle
    // -----------------------------------------------------------------------
    /** Discover a worker via the API, then connect to it via WebSocket. */
    async connect() {
        this._workerUrl = await this.discover();
        await this.connectWs(this._workerUrl);
    }
    /** Gracefully close the connection. Disables auto-reconnect. */
    async disconnect() {
        this.autoReconnect = false;
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }
    // -----------------------------------------------------------------------
    // Phone commands
    // -----------------------------------------------------------------------
    /** Take a screenshot. Returns the base64-encoded WebP image. */
    async screenshot() {
        const resp = await this.sendCommand("screenshot");
        return { image: resp.result?.image ?? "" };
    }
    /** Tap at the given screen coordinates. */
    async click(x, y) {
        await this.sendCommand("click", { x, y });
    }
    /** Long-press at the given screen coordinates. */
    async longClick(x, y) {
        await this.sendCommand("long_click", { x, y });
    }
    /** Drag from (startX, startY) to (endX, endY). */
    async drag(startX, startY, endX, endY) {
        await this.sendCommand("drag", { startX, startY, endX, endY });
    }
    /**
     * Scroll the screen.
     * @param direction - "up", "down", "left", or "right"
     * @param amount    - scroll distance in pixels (default: 300)
     */
    async scroll(direction, amount) {
        const dist = amount ?? 300;
        // Map direction to dx/dy deltas at the center of a typical screen
        const centerX = 540;
        const centerY = 1200;
        const map = {
            up: { dx: 0, dy: -dist },
            down: { dx: 0, dy: dist },
            left: { dx: -dist, dy: 0 },
            right: { dx: dist, dy: 0 },
        };
        const { dx, dy } = map[direction];
        await this.sendCommand("scroll", { x: centerX, y: centerY, dx, dy });
    }
    /** Type text into the currently focused input field. */
    async type(text) {
        await this.sendCommand("type", { text });
    }
    /** Get text from the currently focused element. */
    async getText() {
        const resp = await this.sendCommand("get_text");
        return { text: resp.result?.text ?? "" };
    }
    /** Select all text in the focused element. */
    async selectAll() {
        await this.sendCommand("select_all");
    }
    /** Copy selected text to clipboard. */
    async copy() {
        await this.sendCommand("copy");
    }
    /** Paste from clipboard. */
    async paste() {
        await this.sendCommand("paste");
    }
    /** Press the Back button. */
    async back() {
        await this.sendCommand("back");
    }
    /** Press the Home button. */
    async home() {
        await this.sendCommand("home");
    }
    /** Open the Recents / app switcher. */
    async recents() {
        await this.sendCommand("recents");
    }
    /** Get the UI accessibility tree. */
    async uiTree() {
        const resp = await this.sendCommand("ui_tree");
        return { tree: resp.result?.tree ?? [] };
    }
    /**
     * Take a photo with the device camera.
     * @param facing - "front" or "rear" (default: "rear")
     */
    async camera(facing) {
        const params = {};
        if (facing !== undefined) {
            params.camera = facing === "front" ? "1" : "0";
        }
        const resp = await this.sendCommand("camera", Object.keys(params).length > 0 ? params : undefined);
        return { image: resp.result?.image ?? "" };
    }
    // -----------------------------------------------------------------------
    // Generic command
    // -----------------------------------------------------------------------
    /**
     * Send an arbitrary command to the phone.
     * Useful for future commands not yet covered by typed methods.
     */
    sendCommand(cmd, params) {
        return new Promise((resolve, reject) => {
            if (!this.ws || this.ws.readyState !== ws_1.default.OPEN) {
                return reject(new Error("not connected"));
            }
            const msg = { cmd };
            if (params)
                msg.params = params;
            this.ws.send(JSON.stringify(msg));
            const tempId = -(Date.now() + Math.random());
            const timer = setTimeout(() => {
                this.pending.delete(tempId);
                reject(new Error(`command timed out: ${cmd}`));
            }, this.commandTimeout);
            this.pending.set(tempId, { resolve, reject, timer });
            this._lastTempId = tempId;
        });
    }
    // -----------------------------------------------------------------------
    // Internal: discovery & WebSocket
    // -----------------------------------------------------------------------
    async discover() {
        const resp = await fetch(`${this.apiUrl}/api/discover`, {
            method: "POST",
            headers: {
                Authorization: `Bearer ${this.apiKey}`,
                "Content-Type": "application/json",
            },
        });
        if (!resp.ok) {
            const body = await resp.text();
            throw new Error(`discovery failed (${resp.status}): ${body}`);
        }
        const data = (await resp.json());
        if (!data.wsUrl) {
            throw new Error("discovery returned no wsUrl");
        }
        return data.wsUrl;
    }
    connectWs(workerUrl) {
        return new Promise((resolve, reject) => {
            const ws = new ws_1.default(workerUrl);
            this.ws = ws;
            ws.on("open", () => {
                const auth = {
                    type: "auth",
                    token: this.apiKey,
                    role: "controller",
                    last_ack: 0,
                };
                if (this.deviceId) {
                    auth.target_device_id = this.deviceId;
                }
                ws.send(JSON.stringify(auth));
            });
            ws.on("message", (data) => {
                let msg;
                try {
                    msg = JSON.parse(data.toString());
                }
                catch {
                    return;
                }
                this.handleMessage(msg, resolve, reject);
            });
            ws.on("close", () => {
                this._connected = false;
                this.emit("disconnected");
                // Reject all pending commands
                for (const [, p] of this.pending) {
                    clearTimeout(p.timer);
                    p.reject(new Error("connection closed"));
                }
                this.pending.clear();
                if (this.autoReconnect) {
                    this.emit("reconnecting");
                    this.reconnect();
                }
            });
            ws.on("error", (err) => {
                this.emit("error", err instanceof Error ? err : new Error(String(err)));
                if (!this._connected) {
                    reject(err);
                }
            });
        });
    }
    async reconnect() {
        const delays = [1000, 2000, 4000, 8000, 16000, 30000];
        for (let attempt = 0; attempt < delays.length; attempt++) {
            await new Promise((r) => setTimeout(r, delays[attempt]));
            try {
                this._workerUrl = await this.discover();
                await this.connectWs(this._workerUrl);
                this.emit("reconnected", this._workerUrl);
                return;
            }
            catch {
                // keep retrying
            }
        }
    }
    handleMessage(msg, authResolve, authReject) {
        if ("type" in msg) {
            switch (msg.type) {
                case "auth_ok":
                    this._phoneConnected = msg.phone_connected ?? false;
                    this._connected = true;
                    this.emit("connected");
                    this.emit("phone_status", this._phoneConnected);
                    authResolve?.();
                    break;
                case "auth_fail":
                    authReject?.(new Error(msg.error));
                    break;
                case "cmd_accepted": {
                    const entry = this.pending.get(this._lastTempId);
                    if (entry) {
                        this.pending.delete(this._lastTempId);
                        this.pending.set(msg.id, entry);
                    }
                    break;
                }
                case "phone_status":
                    this._phoneConnected = msg.connected;
                    this.emit("phone_status", msg.connected);
                    break;
                case "ping":
                    this.ws?.send(JSON.stringify({ type: "pong" }));
                    break;
                case "error":
                    this.emit("error", new Error(msg.error));
                    break;
            }
        }
        // Command response (has id + status, no type)
        if ("id" in msg && "status" in msg && !("type" in msg)) {
            const resp = msg;
            const entry = this.pending.get(resp.id);
            if (entry) {
                clearTimeout(entry.timer);
                this.pending.delete(resp.id);
                if (resp.status === "ok") {
                    entry.resolve(resp);
                }
                else {
                    entry.reject(new Error(resp.error ?? `command failed: ${resp.status}`));
                }
            }
        }
    }
}
exports.PhoneMCPClient = PhoneMCPClient;
//# sourceMappingURL=client.js.map