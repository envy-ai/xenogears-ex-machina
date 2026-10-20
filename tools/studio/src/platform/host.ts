import type * as Fs from 'node:fs/promises';
import type * as Path from 'node:path';
import { openNodeFolder } from './node-files';
import { openBrowserFolder, type DirectoryHandle } from './browser-files';

interface NwHost {
  require(id: 'node:fs/promises'): typeof Fs;
  require(id: 'node:path'): typeof Path;
  require(id: 'node:url'): typeof import('node:url');
  App: { argv: string[]; quit(): void };
}
export const nativeHost = (globalThis as typeof globalThis & { nw?: NwHost }).nw;
export async function selectFolder() {
  if (nativeHost) {
    const folder = await new Promise<string>((resolve, reject) => {
      const input = document.createElement('input');
      input.type = 'file'; input.setAttribute('nwdirectory', '');
      input.addEventListener('change', () => input.value ? resolve(input.value) : reject(new Error('No folder selected')), { once: true });
      input.addEventListener('cancel', () => reject(new DOMException('Folder selection cancelled', 'AbortError')), { once: true });
      input.click();
    });
    return openNodeFolder(folder, nativeHost.require('node:fs/promises'), nativeHost.require('node:path'));
  }
  const picker = (window as typeof window & { showDirectoryPicker?: () => Promise<DirectoryHandle> }).showDirectoryPicker;
  if (!picker) throw new Error('Opening folders requires a browser with the File System Access API in a secure context. Use Chromium over HTTPS.');
  return openBrowserFolder(await picker());
}
