import { IncomingMessage, ServerResponse } from 'http';
import { Config } from './lib/config.js';
export declare function createMcpHandler(config: Config, verifyToken: (key: string) => string | null): (req: IncomingMessage, res: ServerResponse) => Promise<void>;
