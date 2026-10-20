import init, { Studio } from './pkg/xem_studio';
import wasmPath from './pkg/xem_studio_bg.wasm?url';
import { nativeHost } from '../platform/host';

export interface Backend { command(json: string): string }
export async function loadBackend(): Promise<Backend> {
  const url = new URL(wasmPath, window.location.href);
  // File URLs in the desktop shell use Node bytes; HTTPS uses ordinary fetch.
  const source = nativeHost && url.protocol === 'file:'
    ? await nativeHost.require('node:fs/promises').readFile(nativeHost.require('node:url').fileURLToPath(url.href))
    : url;
  await init({ module_or_path: source });
  return new Studio();
}
