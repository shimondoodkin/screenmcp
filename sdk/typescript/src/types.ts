// ---------------------------------------------------------------------------
// Client options
// ---------------------------------------------------------------------------

export interface ScreenMCPClientOptions {
  /** API key (pk_... format) for authentication */
  apiKey: string;
  /** Base URL of the ScreenMCP API server. Defaults to https://server10.doodkin.com */
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

export type ScrollDirection = "up" | "down" | "left" | "right";

export type CameraFacing = "front" | "rear";

// ---------------------------------------------------------------------------
// Wire protocol types (internal)
// ---------------------------------------------------------------------------

/** Auth message sent by controller to worker */
export interface AuthMessage {
  type: "auth";
  key: string;
  role: "controller";
  target_device_id?: string;
  last_ack: number;
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
