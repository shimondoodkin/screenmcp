// Auth message sent by controller to worker
export interface AuthMessage {
  type: "auth";
  token: string;
  role: "controller";
  target_uid: string;
  last_ack: number;
}

// Server → Controller messages
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

// Command response from phone (relayed by worker)
export interface CommandResponse {
  id: number;
  status: string;
  result?: Record<string, unknown>;
  error?: string;
}

// Controller → Worker command
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
