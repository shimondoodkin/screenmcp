import { EventEmitter } from "events";
import type { CameraFacing, CameraResult, CommandResponse, PhoneMCPClientOptions, PhoneMCPEvents, ScreenshotResult, ScrollDirection, TextResult, UiTreeResult } from "./types.js";
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
export declare class PhoneMCPClient extends EventEmitter {
    private ws;
    private readonly apiKey;
    private readonly apiUrl;
    private readonly deviceId?;
    private readonly commandTimeout;
    private autoReconnect;
    private pending;
    private _lastTempId;
    private _phoneConnected;
    private _workerUrl;
    private _connected;
    constructor(options: PhoneMCPClientOptions);
    /** Whether the target phone is currently connected to the worker. */
    get phoneConnected(): boolean;
    /** The worker WebSocket URL currently in use. */
    get workerUrl(): string | null;
    /** Whether the client is connected to the worker. */
    get connected(): boolean;
    on<K extends keyof PhoneMCPEvents>(event: K, listener: (...args: PhoneMCPEvents[K]) => void): this;
    once<K extends keyof PhoneMCPEvents>(event: K, listener: (...args: PhoneMCPEvents[K]) => void): this;
    emit<K extends keyof PhoneMCPEvents>(event: K, ...args: PhoneMCPEvents[K]): boolean;
    /** Discover a worker via the API, then connect to it via WebSocket. */
    connect(): Promise<void>;
    /** Gracefully close the connection. Disables auto-reconnect. */
    disconnect(): Promise<void>;
    /** Take a screenshot. Returns the base64-encoded WebP image. */
    screenshot(): Promise<ScreenshotResult>;
    /** Tap at the given screen coordinates. */
    click(x: number, y: number): Promise<void>;
    /** Long-press at the given screen coordinates. */
    longClick(x: number, y: number): Promise<void>;
    /** Drag from (startX, startY) to (endX, endY). */
    drag(startX: number, startY: number, endX: number, endY: number): Promise<void>;
    /**
     * Scroll the screen.
     * @param direction - "up", "down", "left", or "right"
     * @param amount    - scroll distance in pixels (default: 300)
     */
    scroll(direction: ScrollDirection, amount?: number): Promise<void>;
    /** Type text into the currently focused input field. */
    type(text: string): Promise<void>;
    /** Get text from the currently focused element. */
    getText(): Promise<TextResult>;
    /** Select all text in the focused element. */
    selectAll(): Promise<void>;
    /** Copy selected text to clipboard. */
    copy(): Promise<void>;
    /** Paste from clipboard. */
    paste(): Promise<void>;
    /** Press the Back button. */
    back(): Promise<void>;
    /** Press the Home button. */
    home(): Promise<void>;
    /** Open the Recents / app switcher. */
    recents(): Promise<void>;
    /** Get the UI accessibility tree. */
    uiTree(): Promise<UiTreeResult>;
    /**
     * Take a photo with the device camera.
     * @param facing - "front" or "rear" (default: "rear")
     */
    camera(facing?: CameraFacing): Promise<CameraResult>;
    /**
     * Send an arbitrary command to the phone.
     * Useful for future commands not yet covered by typed methods.
     */
    sendCommand(cmd: string, params?: Record<string, unknown>): Promise<CommandResponse>;
    private discover;
    private connectWs;
    private reconnect;
    private handleMessage;
}
//# sourceMappingURL=client.d.ts.map