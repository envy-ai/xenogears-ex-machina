import type { FilesAdapter } from './files';
import { pathParts, sortEntries } from './files';
import { cleanupAfterFailure } from './errors';

// Chromium's File System Access API (not the similarly named sandbox filesystem).
export interface DirectoryHandle {
  name: string;
  values(): AsyncIterable<DirectoryHandle | FileHandle>;
  kind: 'directory';
  getDirectoryHandle(name: string): Promise<DirectoryHandle>;
  getFileHandle(name: string, options?: { create: boolean }): Promise<FileHandle>;
}
interface FileHandle {
  name: string; kind: 'file';
  getFile(): Promise<File>;
  createWritable(): Promise<{ write(data: Uint8Array): Promise<void>; close(): Promise<void>; abort(): Promise<void> }>;
}
export function openBrowserFolder(root: DirectoryHandle): FilesAdapter {
  const directory = async (parts: string[]) => {
    let current = root;
    for (const part of parts) current = await current.getDirectoryHandle(part);
    return current;
  };
  const file = async (relative: string, create: boolean) => {
    const parts = pathParts(relative);
    const name = parts.pop();
    if (!name) throw new Error('A file path is required');
    return (await directory(parts)).getFileHandle(name, { create });
  };
  return {
    kind: 'browser', name: root.name,
    async list(relative = '') {
      const entries = [];
      for await (const entry of (await directory(pathParts(relative))).values()) entries.push({ name: entry.name, kind: entry.kind });
      return sortEntries(entries);
    },
    async read(relative) { return new Uint8Array(await (await (await file(relative, false)).getFile()).arrayBuffer()); },
    async writeAtomic(relative, data) {
      const writer = await (await file(relative, true)).createWritable();
      try { await writer.write(data); await writer.close(); }
      catch (error) { return cleanupAfterFailure(error, [() => writer.abort()]); }
    },
  };
}
