import type * as Fs from 'node:fs/promises';
import type * as Path from 'node:path';
import { openNodeFolder } from './node-files';
import { openBrowserFolder, type DirectoryHandle } from './browser-files';

interface NwHost {
  require(id: 'node:fs/promises'): typeof Fs;
  require(id: 'node:path'): typeof Path;
  require(id: 'node:url'): typeof import('node:url');
  require(id: 'node:child_process'): typeof import('node:child_process');
  require(id: 'node:process'): typeof import('node:process');
  App: { argv: string[]; quit(): void };
}
export const nativeHost = (globalThis as typeof globalThis & { nw?: NwHost }).nw;
export async function selectFolder() {
  if (nativeHost) {
    const folder = await selectNativePath(true);
    return openNodeFolder(folder, nativeHost.require('node:fs/promises'), nativeHost.require('node:path'));
  }
  const picker = (window as typeof window & { showDirectoryPicker?: () => Promise<DirectoryHandle> }).showDirectoryPicker;
  if (!picker) throw new Error('Opening folders requires a browser with the File System Access API in a secure context. Use Chromium over HTTPS.');
  return openBrowserFolder(await picker());
}

/** Native file/directory selection shared by project and owned-game launch. */
export async function selectNativePath(directory = false): Promise<string> {
  if (!nativeHost) throw new Error('Native path selection requires NW.js');
  return new Promise<string>((resolve, reject) => {
    const input = document.createElement('input'); input.type = 'file';
    if (directory) input.setAttribute('nwdirectory', '');
    input.addEventListener('change', () => input.value ? resolve(input.value) : reject(new Error('No path selected')), { once: true });
    input.addEventListener('cancel', () => reject(new DOMException('Path selection cancelled', 'AbortError')), { once: true });
    input.click();
  });
}
