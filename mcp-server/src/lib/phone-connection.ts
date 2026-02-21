import WebSocket from 'ws';

interface PhoneCommand {
  id: number;
  cmd: string;
  params?: Record<string, unknown>;
}

interface PhoneResponse {
  id: number;
  status: 'ok' | 'error';
  result?: Record<string, unknown>;
  error?: string;
}

export class PhoneConnection {
  private ws: WebSocket | null = null;
  private commandId = 0;
  private pendingCommands = new Map<number, { resolve: (value: PhoneResponse) => void; reject: (error: Error) => void }>();

  async connect(workerUrl: string, token: string, targetDeviceId?: string): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(workerUrl);

      this.ws.on('open', () => {
        const authMsg = targetDeviceId
          ? { type: 'auth', key: token, role: 'controller', target_device_id: targetDeviceId, last_ack: 0 }
          : { type: 'auth', key: token, role: 'controller', last_ack: 0 };

        this.ws!.send(JSON.stringify(authMsg));
      });

      this.ws.on('message', (data: WebSocket.RawData) => {
        const msg = JSON.parse(data.toString());

        if (msg.type === 'auth_ok') {
          resolve();
        } else if (msg.type === 'auth_fail') {
          reject(new Error(`Auth failed: ${msg.error}`));
        } else if (msg.id !== undefined && msg.status !== undefined) {
          const pending = this.pendingCommands.get(msg.id);
          if (pending) {
            this.pendingCommands.delete(msg.id);
            if (msg.status === 'ok') {
              pending.resolve(msg);
            } else {
              pending.reject(new Error(msg.error || 'Command failed'));
            }
          }
        }
      });

      this.ws.on('error', reject);
    });
  }

  async sendCommand(cmd: string, params?: Record<string, unknown>): Promise<PhoneResponse> {
    if (!this.ws) {
      throw new Error('Not connected');
    }

    const id = ++this.commandId;
    const command: PhoneCommand = { id, cmd, params };

    return new Promise((resolve, reject) => {
      this.pendingCommands.set(id, { resolve, reject });
      this.ws!.send(JSON.stringify(command));

      setTimeout(() => {
        if (this.pendingCommands.has(id)) {
          this.pendingCommands.delete(id);
          reject(new Error('Command timeout'));
        }
      }, 30000);
    });
  }

  disconnect(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}
