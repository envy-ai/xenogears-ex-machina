import type * as NodeFs from 'node:fs/promises';
import type * as NodePath from 'node:path';
import type { FilesAdapter } from './files';
import { pathParts, sortEntries } from './files';
import { cleanupAfterFailure } from './errors';

type Fs = typeof NodeFs;
type Path = typeof NodePath;

/** All paths are relative to a user-selected folder; symlinks cannot escape it. */
export async function openNodeFolder(folder: string, fs: Fs, path: Path): Promise<FilesAdapter> {
  const root = await fs.realpath(folder);
  if (!(await fs.stat(root)).isDirectory()) throw new Error(`Not a folder: ${folder}`);
  const contained = (absolute: string) => {
    const relative = path.relative(root, absolute);
    if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
      throw new Error(`Path escapes the opened folder: ${absolute}`);
    }
    return absolute;
  };
  const existing = async (relative: string) => contained(await fs.realpath(path.join(root, ...pathParts(relative))));
  return {
    kind: 'node', name: path.basename(root),
    async list(relative = '') {
      const entries = await fs.readdir(await existing(relative), { withFileTypes: true });
      return sortEntries(await Promise.all(entries.map(async entry => ({
        name: entry.name,
        kind: (await fs.stat(await existing([relative, entry.name].filter(Boolean).join('/')))).isDirectory() ? 'directory' as const : 'file' as const,
      }))));
    },
    async read(relative) { return new Uint8Array(await fs.readFile(await existing(relative))); },
    async writeAtomic(relative, data) {
      const parts = pathParts(relative);
      const name = parts.pop();
      if (!name) throw new Error('A file path is required');
      const parent = await existing(parts.join('/'));
      const destination = path.join(parent, name);
      // Refuse symlink destinations: replacement must never follow an alias.
      try {
        if ((await fs.lstat(destination)).isSymbolicLink()) throw new Error(`Cannot replace a symlink: ${relative}`);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
      const partial = path.join(parent, `.${name}.${crypto.randomUUID()}.partial`);
      const file = await fs.open(partial, 'wx');
      try {
        await file.writeFile(data);
        await file.sync();
      } catch (error) {
        return cleanupAfterFailure(error, [() => file.close(), () => fs.unlink(partial)]);
      }
      try { await file.close(); }
      catch (error) { return cleanupAfterFailure(error, [() => fs.unlink(partial)]); }
      try { await fs.rename(partial, destination); }
      catch (error) { return cleanupAfterFailure(error, [() => fs.unlink(partial)]); }
    },
  };
}
