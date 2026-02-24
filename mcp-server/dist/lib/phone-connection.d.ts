interface PhoneResponse {
    id: number;
    status: 'ok' | 'error';
    result?: Record<string, unknown>;
    error?: string;
}
export declare class PhoneConnection {
    private ws;
    private commandId;
    private pendingCommands;
    connect(workerUrl: string, token: string, targetDeviceId?: string): Promise<void>;
    sendCommand(cmd: string, params?: Record<string, unknown>): Promise<PhoneResponse>;
    disconnect(): void;
}
export {};
