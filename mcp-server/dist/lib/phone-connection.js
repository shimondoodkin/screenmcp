"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.PhoneConnection = void 0;
const ws_1 = __importDefault(require("ws"));
class PhoneConnection {
    ws = null;
    commandId = 0;
    pendingCommands = new Map();
    async connect(workerUrl, token, targetDeviceId) {
        return new Promise((resolve, reject) => {
            this.ws = new ws_1.default(workerUrl);
            this.ws.on('open', () => {
                const authMsg = targetDeviceId
                    ? { type: 'auth', key: token, role: 'controller', target_device_id: targetDeviceId, last_ack: 0 }
                    : { type: 'auth', key: token, role: 'controller', last_ack: 0 };
                this.ws.send(JSON.stringify(authMsg));
            });
            this.ws.on('message', (data) => {
                const msg = JSON.parse(data.toString());
                if (msg.type === 'auth_ok') {
                    resolve();
                }
                else if (msg.type === 'auth_fail') {
                    reject(new Error(`Auth failed: ${msg.error}`));
                }
                else if (msg.id !== undefined && msg.status !== undefined) {
                    const pending = this.pendingCommands.get(msg.id);
                    if (pending) {
                        this.pendingCommands.delete(msg.id);
                        if (msg.status === 'ok') {
                            pending.resolve(msg);
                        }
                        else {
                            pending.reject(new Error(msg.error || 'Command failed'));
                        }
                    }
                }
            });
            this.ws.on('error', reject);
        });
    }
    async sendCommand(cmd, params) {
        if (!this.ws) {
            throw new Error('Not connected');
        }
        const id = ++this.commandId;
        const command = { id, cmd, params };
        return new Promise((resolve, reject) => {
            this.pendingCommands.set(id, { resolve, reject });
            this.ws.send(JSON.stringify(command));
            setTimeout(() => {
                if (this.pendingCommands.has(id)) {
                    this.pendingCommands.delete(id);
                    reject(new Error('Command timeout'));
                }
            }, 30000);
        });
    }
    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }
}
exports.PhoneConnection = PhoneConnection;
