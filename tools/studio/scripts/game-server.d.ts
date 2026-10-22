import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Plugin } from 'vite';
export function createGameMiddleware(directory?: string): Promise<(request: IncomingMessage, response: ServerResponse, next: () => void) => Promise<void>>;
export function studioGameServer(directory?: string): Plugin;
