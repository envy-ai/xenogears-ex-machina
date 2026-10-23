export interface FileEntry { name: string; kind: 'file' | 'directory' }
export interface FilesAdapter {
  readonly kind: 'node' | 'browser';
  readonly name: string;
  list(path?: string): Promise<FileEntry[]>;
  read(path: string): Promise<Uint8Array>;
  /** Existing contained path, available only in the native file adapter. */
  nativePath?(path: string): Promise<string>;
  writeAtomic(path: string, data: Uint8Array): Promise<void>;
}

export function pathParts(path: string): string[] {
  if (path === '') return [];
  const parts = path.split('/');
  if (parts.some(part => !part || part === '.' || part === '..' || part.includes('\\') || part.includes('\0') || part.includes(':'))) {
    throw new Error(`Expected a relative project path: ${path}`);
  }
  return parts;
}
export function sortEntries(entries: FileEntry[]): FileEntry[] {
  return entries.sort((a, b) => a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === 'directory' ? -1 : 1);
}
