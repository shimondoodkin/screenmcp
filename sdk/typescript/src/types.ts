// ---------------------------------------------------------------------------
// Client options
// ---------------------------------------------------------------------------

export interface ScreenMCPClientOptions {
  /** API key (pk_... format) for authentication */
  apiKey: string;
  /** Base URL of the ScreenMCP API server. Defaults to https://screenmcp.com */
  apiUrl?: string;
  /** Target device ID. If omitted, the server will pick the first available device. */
  deviceId?: string;
  /** Per-command timeout in milliseconds. Defaults to 30 000. */
  commandTimeout?: number;
  /** Automatically reconnect when the worker connection drops. Defaults to true. */
  autoReconnect?: boolean;
}

// ---------------------------------------------------------------------------
// Command result types
// ---------------------------------------------------------------------------

export interface ScreenshotResult {
  /** Base64-encoded image (WebP) */
  image: string;
}

export interface TextResult {
  /** Text content from the focused element */
  text: string;
}

export interface UiTreeResult {
  /** Accessibility tree nodes */
  tree: unknown[];
}

export interface CameraResult {
  /** Base64-encoded image (WebP) */
  image: string;
}

export interface ClipboardResult {
  /** Clipboard text contents */
  text: string;
}

export interface CopyResult {
  /** Copied text (only present when return_text was true) */
  text?: string;
}

export interface CameraInfo {
  /** Camera ID string */
  id: string;
  /** Camera facing direction */
  facing: "back" | "front" | "external" | "unknown";
}

export interface ListCamerasResult {
  /** Available cameras on the device */
  cameras: CameraInfo[];
}

export type ScrollDirection = "up" | "down" | "left" | "right";


// ---------------------------------------------------------------------------
// Wire protocol types (internal)
// ---------------------------------------------------------------------------

/** Version info sent in auth messages for compatibility checking */
export interface ClientVersion {
  major: number;
  minor: number;
  component: string;
}

/** Auth message sent by controller to worker */
export interface AuthMessage {
  type: "auth";
  key: string;
  role: "controller";
  target_device_id?: string;
  last_ack: number;
  version?: ClientVersion;
}

export interface AuthOkMessage {
  type: "auth_ok";
  resume_from: number;
  phone_connected?: boolean;
}

export interface AuthFailMessage {
  type: "auth_fail";
  error: string;
}

export interface CmdAcceptedMessage {
  type: "cmd_accepted";
  id: number;
}

export interface PhoneStatusMessage {
  type: "phone_status";
  connected: boolean;
}

export interface PingMessage {
  type: "ping";
}

export interface ErrorMessage {
  type: "error";
  error: string;
  code?: string;
  message?: string;
  update_url?: string;
}

export interface CommandResponse {
  id: number;
  status: string;
  result?: Record<string, unknown>;
  error?: string;
}

export interface ControllerCommand {
  cmd: string;
  params?: Record<string, unknown>;
}

export type ServerMessage =
  | AuthOkMessage
  | AuthFailMessage
  | CmdAcceptedMessage
  | PhoneStatusMessage
  | PingMessage
  | ErrorMessage
  | CommandResponse;

// ---------------------------------------------------------------------------
// Event map for typed event emitter
// ---------------------------------------------------------------------------

export interface ScreenMCPEvents {
  connected: [];
  disconnected: [];
  error: [error: Error];
  phone_status: [connected: boolean];
  reconnecting: [];
  reconnected: [workerUrl: string];
}
