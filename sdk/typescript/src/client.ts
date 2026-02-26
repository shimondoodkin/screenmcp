import WebSocket from "ws";
import { EventEmitter } from "events";
import { ElementHandle, findElements } from "./selector.js";
import type { FoundElement } from "./selector.js";
import type {
  AuthMessage,
  CameraResult,
  ClientVersion,
  ClipboardResult,
  CommandResponse,
  ConnectOptions,
  CopyResult,
  ControllerCommand,
  DeviceInfo,
  ListCamerasResult,
  ScreenMCPClientOptions,
  ScreenMCPEvents,
  ScreenshotResult,
  ScrollDirection,
  ServerMessage,
  TextResult,
  UiTreeResult,
} from "./types.js";

const DEFAULT_API_URL = "https://api.screenmcp.com";

/** Current SDK version sent to the worker for compatibility checking. */
export const SDK_VERSION: ClientVersion = { major: 1, minor: 0, component: "sdk-ts" };

interface PendingCommand {
  resolve: (resp: CommandResponse) => void;
  reject: (err: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

// ---------------------------------------------------------------------------
// ScreenMCPClient — API-level, lightweight, does NOT extend EventEmitter
// ---------------------------------------------------------------------------

/**
 * ScreenMCPClient handles API-level operations: device discovery and
 * establishing WebSocket connections to devices.
 *
 * ```ts
 * const client = new ScreenMCPClient({ apiKey: "pk_..." });
 * const devices = await client.listDevices();
 * const phone = await client.connect({ deviceId: "abc123" });
 * await phone.screenshot();
 * await phone.disconnect();
 * ```
 */
export class ScreenMCPClient {
  private readonly apiKey: string;
  private readonly apiUrl: string;
  private readonly commandTimeout: number;
  private readonly autoReconnect: boolean;

  constructor(options: ScreenMCPClientOptions) {
    this.apiKey = options.apiKey;
    this.apiUrl = (options.apiUrl ?? DEFAULT_API_URL).replace(/\/+$/, "");
    this.commandTimeout = options.commandTimeout ?? 30_000;
    this.autoReconnect = options.autoReconnect ?? true;
  }

  /**
   * List all devices and their connection status.
   * Calls `GET /api/devices/status` with Bearer token.
   */
  async listDevices(): Promise<DeviceInfo[]> {
    const resp = await fetch(`${this.apiUrl}/api/devices/status`, {
      method: "GET",
      headers: {
        Authorization: `Bearer ${this.apiKey}`,
      },
    });

    if (!resp.ok) {
      const body = await resp.text();
      throw new Error(`listDevices failed (${resp.status}): ${body}`);
    }

    const data = (await resp.json()) as { devices: DeviceInfo[] };
    return data.devices ?? [];
  }

  /**
   * Discover a worker and connect to a device via WebSocket.
   * Returns a {@link DeviceConnection} that provides all command methods.
   *
   * @param options - Optional connect options. If `deviceId` is omitted the
   *   server picks the first available device.
   */
  async connect(options?: ConnectOptions): Promise<DeviceConnection> {
    const deviceId = options?.deviceId;
    const workerUrl = await this.discover(deviceId);

    const connection = new DeviceConnection({
      apiKey: this.apiKey,
      apiUrl: this.apiUrl,
      deviceId,
      workerUrl,
      commandTimeout: this.commandTimeout,
      autoReconnect: this.autoReconnect,
      discoverFn: (did) => this.discover(did),
    });

    await connection._connectWs(workerUrl, deviceId);
    return connection;
  }

  // -----------------------------------------------------------------------
  // Internal: discovery
  // -----------------------------------------------------------------------

  private async discover(deviceId?: string): Promise<string> {
    const resp = await fetch(`${this.apiUrl}/api/discover`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this.apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ device_id: deviceId }),
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
}

// ---------------------------------------------------------------------------
// DeviceConnectionOptions — internal constructor options
// ---------------------------------------------------------------------------

interface DeviceConnectionOptions {
  apiKey: string;
  apiUrl: string;
  deviceId?: string;
  workerUrl: string;
  commandTimeout: number;
  autoReconnect: boolean;
  discoverFn: (deviceId?: string) => Promise<string>;
}

// ---------------------------------------------------------------------------
// DeviceConnection — device-level, extends EventEmitter
// ---------------------------------------------------------------------------

/**
 * DeviceConnection represents an active WebSocket connection to a single
 * device. All phone/desktop command methods live here.
 *
 * Instances are created by {@link ScreenMCPClient.connect}; do not
 * construct directly.
 */
export class DeviceConnection extends EventEmitter {
  private ws: WebSocket | null = null;
  private readonly apiKey: string;
  private readonly apiUrl: string;
  private readonly deviceId?: string;
  private readonly commandTimeout: number;
  private autoReconnect: boolean;
  private readonly discoverFn: (deviceId?: string) => Promise<string>;

  private pending = new Map<number, PendingCommand>();
  private _lastTempId = 0;
  private _phoneConnected = false;
  private _workerUrl: string | null = null;
  private _connected = false;

  /** @internal — use {@link ScreenMCPClient.connect} instead. */
  constructor(options: DeviceConnectionOptions) {
    super();
    this.apiKey = options.apiKey;
    this.apiUrl = options.apiUrl;
    this.deviceId = options.deviceId;
    this._workerUrl = options.workerUrl;
    this.commandTimeout = options.commandTimeout;
    this.autoReconnect = options.autoReconnect;
    this.discoverFn = options.discoverFn;
  }

  // -----------------------------------------------------------------------
  // Public getters
  // -----------------------------------------------------------------------

  /** Whether the target phone is currently connected to the worker. */
  get phoneConnected(): boolean {
    return this._phoneConnected;
  }

  /** The worker WebSocket URL currently in use. */
  get workerUrl(): string | null {
    return this._workerUrl;
  }

  /** Whether the connection to the worker is active. */
  get connected(): boolean {
    return this._connected;
  }

  // -----------------------------------------------------------------------
  // Typed event emitter overrides
  // -----------------------------------------------------------------------

  override on<K extends keyof ScreenMCPEvents>(
    event: K,
    listener: (...args: ScreenMCPEvents[K]) => void,
  ): this {
    return super.on(event, listener as (...args: unknown[]) => void);
  }

  override once<K extends keyof ScreenMCPEvents>(
    event: K,
    listener: (...args: ScreenMCPEvents[K]) => void,
  ): this {
    return super.once(event, listener as (...args: unknown[]) => void);
  }

  override emit<K extends keyof ScreenMCPEvents>(
    event: K,
    ...args: ScreenMCPEvents[K]
  ): boolean {
    return super.emit(event, ...args);
  }

  // -----------------------------------------------------------------------
  // Connection lifecycle
  // -----------------------------------------------------------------------

  /** Gracefully close the connection. Disables auto-reconnect. */
  async disconnect(): Promise<void> {
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
  async screenshot(): Promise<ScreenshotResult> {
    const resp = await this.sendCommand("screenshot");
    return { image: (resp.result as ScreenshotResult | undefined)?.image ?? "" };
  }

  /** Tap at the given screen coordinates. */
  async click(x: number, y: number): Promise<void> {
    await this.sendCommand("click", { x, y });
  }

  /** Long-press at the given screen coordinates. */
  async longClick(x: number, y: number): Promise<void> {
    await this.sendCommand("long_click", { x, y });
  }

  /** Drag from (startX, startY) to (endX, endY). */
  async drag(
    startX: number,
    startY: number,
    endX: number,
    endY: number,
  ): Promise<void> {
    await this.sendCommand("drag", { startX, startY, endX, endY });
  }

  /**
   * Scroll the screen.
   * @param direction - "up", "down", "left", or "right"
   * @param amount    - scroll distance in pixels (default: 300)
   */
  async scroll(direction: ScrollDirection, amount?: number): Promise<void> {
    const dist = amount ?? 300;
    // Map direction to dx/dy deltas at the center of a typical screen
    const centerX = 540;
    const centerY = 1200;
    const map: Record<ScrollDirection, { dx: number; dy: number }> = {
      up: { dx: 0, dy: -dist },
      down: { dx: 0, dy: dist },
      left: { dx: -dist, dy: 0 },
      right: { dx: dist, dy: 0 },
    };
    const { dx, dy } = map[direction];
    await this.sendCommand("scroll", { x: centerX, y: centerY, dx, dy });
  }

  /** Type text into the currently focused input field. */
  async type(text: string): Promise<void> {
    await this.sendCommand("type", { text });
  }

  /** Get text from the currently focused element. */
  async getText(): Promise<TextResult> {
    const resp = await this.sendCommand("get_text");
    return { text: (resp.result as TextResult | undefined)?.text ?? "" };
  }

  /** Select all text in the focused element. */
  async selectAll(): Promise<void> {
    await this.sendCommand("select_all");
  }

  /** Copy selected text to clipboard. Optionally return the copied text. */
  async copy(options?: { returnText?: boolean }): Promise<CopyResult> {
    const params: Record<string, unknown> = {};
    if (options?.returnText) params.return_text = true;
    const resp = await this.sendCommand("copy", Object.keys(params).length > 0 ? params : undefined);
    return (resp.result as CopyResult | undefined) ?? {};
  }

  /** Paste into the focused field. Optionally set clipboard before pasting. */
  async paste(text?: string): Promise<void> {
    const params: Record<string, unknown> = {};
    if (text !== undefined) params.text = text;
    await this.sendCommand("paste", Object.keys(params).length > 0 ? params : undefined);
  }

  /** Get clipboard text contents. */
  async getClipboard(): Promise<ClipboardResult> {
    const resp = await this.sendCommand("get_clipboard");
    return { text: (resp.result as ClipboardResult | undefined)?.text ?? "" };
  }

  /** Set clipboard to the given text. */
  async setClipboard(text: string): Promise<void> {
    await this.sendCommand("set_clipboard", { text });
  }

  /** Press the Back button. */
  async back(): Promise<void> {
    await this.sendCommand("back");
  }

  /** Press the Home button. */
  async home(): Promise<void> {
    await this.sendCommand("home");
  }

  /** Open the Recents / app switcher. */
  async recents(): Promise<void> {
    await this.sendCommand("recents");
  }

  /** Get the UI accessibility tree. */
  async uiTree(): Promise<UiTreeResult> {
    const resp = await this.sendCommand("ui_tree");
    return { tree: (resp.result as UiTreeResult | undefined)?.tree ?? [] };
  }

  /**
   * List available cameras on the device.
   * Returns camera IDs with facing direction.
   */
  async listCameras(): Promise<ListCamerasResult> {
    const resp = await this.sendCommand("list_cameras");
    return { cameras: (resp.result as ListCamerasResult | undefined)?.cameras ?? [] };
  }

  /**
   * Take a photo with the device camera.
   * @param cameraId - Camera ID string (use listCameras() to discover). Default: "0".
   */
  async camera(cameraId?: string): Promise<CameraResult> {
    const params: Record<string, unknown> = {};
    if (cameraId !== undefined) params.camera = cameraId;
    const resp = await this.sendCommand(
      "camera",
      Object.keys(params).length > 0 ? params : undefined,
    );
    return { image: (resp.result as CameraResult | undefined)?.image ?? "" };
  }

  /**
   * Play audio on the device.
   * @param audioBase64 - Base64-encoded audio data.
   * @param volume      - Optional playback volume (0.0 to 1.0).
   */
  async playAudio(audioBase64: string, volume?: number): Promise<void> {
    const params: Record<string, unknown> = { audio_data: audioBase64 };
    if (volume !== undefined) params.volume = volume;
    await this.sendCommand("play_audio", params);
  }

  // -----------------------------------------------------------------------
  // Keyboard commands (desktop only)
  // -----------------------------------------------------------------------

  /** Press and hold a key (desktop only). Use with releaseKey() for combos like Alt+Tab. */
  async holdKey(key: string): Promise<void> {
    await this.sendCommand("hold_key", { key });
  }

  /** Release a held key (desktop only). */
  async releaseKey(key: string): Promise<void> {
    await this.sendCommand("release_key", { key });
  }

  /** Press and release a key in one action (desktop only). */
  async pressKey(key: string): Promise<void> {
    await this.sendCommand("press_key", { key });
  }

  // -----------------------------------------------------------------------
  // Selector-based element methods
  // -----------------------------------------------------------------------

  /** Find an element by selector. Returns a fluent object with .click(), .type(), .longClick(). */
  find(selector: string, options?: { timeout?: number }): ElementHandle {
    return new ElementHandle(this, selector, options?.timeout ?? 3000);
  }

  /** Find all matching elements. */
  async findAll(
    selector: string,
    options?: { timeout?: number },
  ): Promise<FoundElement[]> {
    const timeout = options?.timeout ?? 3000;
    const deadline = Date.now() + timeout;
    while (true) {
      const { tree } = await this.uiTree();
      const found = findElements(tree, selector);
      if (found.length > 0) return found;
      if (Date.now() >= deadline) return [];
      await new Promise((r) => setTimeout(r, 500));
    }
  }

  /** Check if an element matching the selector exists. */
  async exists(
    selector: string,
    options?: { timeout?: number },
  ): Promise<boolean> {
    const timeout = options?.timeout ?? 0;
    const deadline = Date.now() + timeout;
    while (true) {
      const { tree } = await this.uiTree();
      const found = findElements(tree, selector);
      if (found.length > 0) return true;
      if (Date.now() >= deadline) return false;
      await new Promise((r) => setTimeout(r, 500));
    }
  }

  /** Wait for an element to appear. Throws if not found within timeout. */
  async waitFor(
    selector: string,
    options?: { timeout?: number },
  ): Promise<FoundElement> {
    const timeout = options?.timeout ?? 3000;
    const deadline = Date.now() + timeout;
    while (true) {
      const { tree } = await this.uiTree();
      const found = findElements(tree, selector);
      if (found.length > 0) return found[0];
      if (Date.now() >= deadline) {
        throw new Error(`waitFor timed out: ${selector}`);
      }
      await new Promise((r) => setTimeout(r, 500));
    }
  }

  /** Wait for an element to disappear. Throws if still present after timeout. */
  async waitForGone(
    selector: string,
    options?: { timeout?: number },
  ): Promise<void> {
    const timeout = options?.timeout ?? 3000;
    const deadline = Date.now() + timeout;
    while (true) {
      const { tree } = await this.uiTree();
      const found = findElements(tree, selector);
      if (found.length === 0) return;
      if (Date.now() >= deadline) {
        throw new Error(`waitForGone timed out: ${selector}`);
      }
      await new Promise((r) => setTimeout(r, 500));
    }
  }

  // -----------------------------------------------------------------------
  // Generic command
  // -----------------------------------------------------------------------

  /**
   * Send an arbitrary command to the phone.
   * Useful for future commands not yet covered by typed methods.
   */
  sendCommand(
    cmd: string,
    params?: Record<string, unknown>,
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
      this._lastTempId = tempId;
    });
  }

  // -----------------------------------------------------------------------
  // Internal: WebSocket connection (called by ScreenMCPClient.connect)
  // -----------------------------------------------------------------------

  /** @internal — opens and authenticates the WebSocket. */
  _connectWs(workerUrl: string, deviceId?: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(workerUrl);
      this.ws = ws;
      this._workerUrl = workerUrl;

      ws.on("open", () => {
        const auth: AuthMessage = {
          type: "auth",
          key: this.apiKey,
          role: "controller",
          last_ack: 0,
          version: SDK_VERSION,
        };
        if (deviceId ?? this.deviceId) {
          auth.target_device_id = deviceId ?? this.deviceId;
        }
        ws.send(JSON.stringify(auth));
      });

      ws.on("message", (data) => {
        let msg: ServerMessage;
        try {
          msg = JSON.parse(data.toString());
        } catch {
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

  private async reconnect(): Promise<void> {
    const delays = [1000, 2000, 4000, 8000, 16000, 30000];
    for (let attempt = 0; attempt < delays.length; attempt++) {
      await new Promise((r) => setTimeout(r, delays[attempt]));
      try {
        const newWorkerUrl = await this.discoverFn(this.deviceId);
        await this._connectWs(newWorkerUrl, this.deviceId);
        this.emit("reconnected", newWorkerUrl);
        return;
      } catch {
        // keep retrying
      }
    }
  }

  private handleMessage(
    msg: ServerMessage,
    authResolve?: (value: void) => void,
    authReject?: (err: Error) => void,
  ): void {
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
          if (msg.code && msg.message) {
            // Version error with structured code and message
            const versionErr = new Error(msg.message);
            (versionErr as Error & { code: string; update_url?: string }).code = msg.code;
            if (msg.update_url) {
              (versionErr as Error & { update_url?: string }).update_url = msg.update_url;
            }
            this.emit("error", versionErr);
          } else {
            this.emit("error", new Error(msg.error));
          }
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
            new Error(resp.error ?? `command failed: ${resp.status}`),
          );
        }
      }
    }
  }
}
