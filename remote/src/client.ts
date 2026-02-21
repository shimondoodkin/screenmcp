import WebSocket from "ws";
import { EventEmitter } from "events";
import type {
  AuthMessage,
  CommandResponse,
  ControllerCommand,
  ServerMessage,
} from "./protocol.js";

export interface PhoneClientOptions {
  /** API server URL for discovery (e.g. https://screenmcp-api.ngrok-free.app) */
  apiUrl: string;
  /** Auth token (Firebase ID token or API key) */
  token: string;
  /** Target device ID to control */
  targetDeviceId: string;
  /** Timeout for individual commands (ms) */
  commandTimeout?: number;
  /** Auto-reconnect on worker disconnect */
  autoReconnect?: boolean;
}

interface PendingCommand {
  resolve: (resp: CommandResponse) => void;
  reject: (err: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export class PhoneClient extends EventEmitter {
  private ws: WebSocket | null = null;
  private readonly options: PhoneClientOptions;
  private readonly commandTimeout: number;
  private pending = new Map<number, PendingCommand>();
  private _phoneConnected = false;
  private _workerUrl: string | null = null;
  private _connected = false;

  constructor(options: PhoneClientOptions) {
    super();
    this.options = options;
    this.commandTimeout = options.commandTimeout ?? 30_000;
  }

  get phoneConnected(): boolean {
    return this._phoneConnected;
  }

  get workerUrl(): string | null {
    return this._workerUrl;
  }

  /** Discover a worker via the API, then connect to it via WebSocket. */
  async connect(): Promise<void> {
    // 1. Call discovery API to get worker URL
    this._workerUrl = await this.discover();

    // 2. Connect WebSocket to the worker
    await this.connectWs(this._workerUrl);
  }

  /** Call the discovery API. Returns the worker WebSocket URL. */
  private async discover(): Promise<string> {
    const resp = await fetch(`${this.options.apiUrl}/api/discover`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this.options.token}`,
        "Content-Type": "application/json",
      },
    });

    if (!resp.ok) {
      const body = await resp.text();
      throw new Error(`discovery failed (${resp.status}): ${body}`);
    }

    const data = (await resp.json()) as { wsUrl: string };
    if (!data.wsUrl) {
      throw new Error("discovery returned no wsUrl");
    }

    return data.wsUrl;
  }

  /** Connect WebSocket to a specific worker URL. */
  private connectWs(workerUrl: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(workerUrl);
      this.ws = ws;

      ws.on("open", () => {
        const auth: AuthMessage = {
          type: "auth",
          token: this.options.token,
          role: "controller",
          target_device_id: this.options.targetDeviceId,
          last_ack: 0,
        };
        ws.send(JSON.stringify(auth));
      });

      ws.on("message", (data) => {
        const text = data.toString();
        let msg: ServerMessage;
        try {
          msg = JSON.parse(text);
        } catch {
          return;
        }
        this.handleMessage(msg, resolve, reject);
      });

      ws.on("close", () => {
        this._connected = false;
        this.emit("close");
        // Reject all pending commands
        for (const [, p] of this.pending) {
          clearTimeout(p.timer);
          p.reject(new Error("connection closed"));
        }
        this.pending.clear();

        // Auto-reconnect: rediscover a new worker
        if (this.options.autoReconnect !== false) {
          this.emit("reconnecting");
          this.reconnect();
        }
      });

      ws.on("error", (err) => {
        this.emit("error", err);
        if (!this._connected) {
          reject(err);
        }
      });
    });
  }

  /** Reconnect by rediscovering a worker. */
  private async reconnect(): Promise<void> {
    const delays = [1000, 2000, 4000, 8000, 16000, 30000];
    for (let attempt = 0; attempt < delays.length; attempt++) {
      await new Promise((r) => setTimeout(r, delays[attempt]));
      try {
        this._workerUrl = await this.discover();
        await this.connectWs(this._workerUrl);
        this.emit("reconnected", this._workerUrl);
        return;
      } catch (e) {
        this.emit("reconnect_failed", attempt + 1, e);
      }
    }
    this.emit("reconnect_exhausted");
  }

  async disconnect(): Promise<void> {
    // Disable auto-reconnect for explicit disconnect
    (this.options as { autoReconnect?: boolean }).autoReconnect = false;
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  // --- High-level commands ---

  async screenshot(opts?: { quality?: number; max_width?: number; max_height?: number }): Promise<string> {
    const params: Record<string, unknown> = {};
    if (opts?.quality !== undefined) params.quality = opts.quality;
    if (opts?.max_width !== undefined) params.max_width = opts.max_width;
    if (opts?.max_height !== undefined) params.max_height = opts.max_height;
    const resp = await this.sendCommand("screenshot", Object.keys(params).length > 0 ? params : undefined);
    return (resp.result as { image: string })?.image ?? "";
  }

  async getUiTree(): Promise<unknown[]> {
    const resp = await this.sendCommand("ui_tree");
    return (resp.result as { tree: unknown[] })?.tree ?? [];
  }

  async click(x: number, y: number, duration?: number): Promise<void> {
    const params: Record<string, unknown> = { x, y };
    if (duration !== undefined) params.duration = duration;
    await this.sendCommand("click", params);
  }

  async longClick(x: number, y: number): Promise<void> {
    await this.sendCommand("long_click", { x, y });
  }

  async drag(
    startX: number,
    startY: number,
    endX: number,
    endY: number,
    duration = 300
  ): Promise<void> {
    await this.sendCommand("drag", { startX, startY, endX, endY, duration });
  }

  async type(text: string): Promise<void> {
    await this.sendCommand("type", { text });
  }

  async getText(): Promise<string | null> {
    const resp = await this.sendCommand("get_text");
    return (resp.result as { text: string })?.text ?? null;
  }

  async back(): Promise<void> {
    await this.sendCommand("back");
  }

  async home(): Promise<void> {
    await this.sendCommand("home");
  }

  async recents(): Promise<void> {
    await this.sendCommand("recents");
  }

  async selectAll(): Promise<void> {
    await this.sendCommand("select_all");
  }

  async copy(): Promise<void> {
    await this.sendCommand("copy");
  }

  async paste(): Promise<void> {
    await this.sendCommand("paste");
  }

  async scroll(x: number, y: number, dx: number, dy: number): Promise<void> {
    await this.sendCommand("scroll", { x, y, dx, dy });
  }

  async rightClick(x: number, y: number): Promise<CommandResponse> {
    return this.sendCommand("right_click", { x, y });
  }

  async middleClick(x: number, y: number): Promise<CommandResponse> {
    return this.sendCommand("middle_click", { x, y });
  }

  async mouseScroll(x: number, y: number, dx: number, dy: number): Promise<CommandResponse> {
    return this.sendCommand("mouse_scroll", { x, y, dx, dy });
  }

  async camera(opts?: { camera?: number; quality?: number; max_width?: number; max_height?: number }): Promise<string> {
    const params: Record<string, unknown> = {};
    if (opts?.camera !== undefined) params.camera = String(opts.camera);
    if (opts?.quality !== undefined) params.quality = opts.quality;
    if (opts?.max_width !== undefined) params.max_width = opts.max_width;
    if (opts?.max_height !== undefined) params.max_height = opts.max_height;
    const resp = await this.sendCommand("camera", Object.keys(params).length > 0 ? params : undefined);
    return (resp.result as { image: string })?.image ?? "";
  }

  // --- Internal ---

  private sendCommand(
    cmd: string,
    params?: Record<string, unknown>
  ): Promise<CommandResponse> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        return reject(new Error("not connected"));
      }

      const msg: ControllerCommand = { cmd };
      if (params) msg.params = params;

      this.ws.send(JSON.stringify(msg));

      const tempId = -(Date.now() + Math.random());
      const timer = setTimeout(() => {
        this.pending.delete(tempId);
        reject(new Error(`command timed out: ${cmd}`));
      }, this.commandTimeout);

      this.pending.set(tempId, { resolve, reject, timer });
      (this as unknown as { _lastTempId: number })._lastTempId = tempId;
    });
  }

  private handleMessage(
    msg: ServerMessage,
    authResolve?: (value: void) => void,
    authReject?: (err: Error) => void
  ): void {
    if ("type" in msg) {
      switch (msg.type) {
        case "auth_ok":
          this._phoneConnected = msg.phone_connected ?? false;
          this._connected = true;
          this.emit("phone_status", this._phoneConnected);
          authResolve?.();
          break;

        case "auth_fail":
          authReject?.(new Error(msg.error));
          break;

        case "cmd_accepted": {
          const tempId = (this as unknown as { _lastTempId: number })
            ._lastTempId;
          const entry = this.pending.get(tempId);
          if (entry) {
            this.pending.delete(tempId);
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
          this.emit("server_error", msg.error);
          break;
      }
    }

    // Command response (has id + status, no type)
    if ("id" in msg && "status" in msg && !("type" in msg)) {
      const resp = msg as CommandResponse;
      const entry = this.pending.get(resp.id);
      if (entry) {
        clearTimeout(entry.timer);
        this.pending.delete(resp.id);
        if (resp.status === "ok") {
          entry.resolve(resp);
        } else {
          entry.reject(
            new Error(resp.error ?? `command failed: ${resp.status}`)
          );
        }
      }
    }
  }
}
