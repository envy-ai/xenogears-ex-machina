import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Plugin } from 'vite';

export type DiscMiddleware = (
  request: IncomingMessage,
  response: ServerResponse,
  next: (error?: unknown) => void,
) => void;

export declare function createDiscMiddleware(directory?: string): Promise<DiscMiddleware>;
export declare function studioDiscServer(directory?: string): Plugin;
