import init, { Studio } from './pkg/xem_studio';
import wasmPath from './pkg/xem_studio_bg.wasm?url';
import { nativeHost } from '../platform/host';

export interface Backend { command(json: string): string }

/** Decode the transport envelope; an echoed error-shaped value remains data. */
export function readCommandResult<T = unknown>(reply: string): T {
  const envelope: unknown = JSON.parse(reply);
  if (envelope && typeof envelope === 'object' && !Array.isArray(envelope)
      && Object.keys(envelope).length === 1) {
    const result = envelope as Record<string, unknown>;
    if (Object.hasOwn(result, 'ok')) return result.ok as T;
    if (Object.hasOwn(result, 'error')) {
      const error = result.error;
      if (error && typeof error === 'object' && !Array.isArray(error)
          && Object.keys(error).length === 2 && 'kind' in error && 'message' in error
          && typeof error.kind === 'string' && typeof error.message === 'string') {
        throw new Error(`${error.kind}: ${error.message}`);
      }
    }
  }
  throw new Error('Invalid studio response envelope');
}

export async function loadBackend(): Promise<Backend> {
  const url = new URL(wasmPath, window.location.href);
  // File URLs in the desktop shell use Node bytes; HTTPS uses ordinary fetch.
  const source = nativeHost && url.protocol === 'file:'
    ? await nativeHost.require('node:fs/promises').readFile(nativeHost.require('node:url').fileURLToPath(url.href))
    : url;
  await init({ module_or_path: source });
  return new Studio();
}
