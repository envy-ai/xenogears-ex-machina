import { readFileSync } from 'node:fs';
import * as fs from 'node:fs/promises';
import type { PathLike } from 'node:fs';
import { tmpdir } from 'node:os';
import * as path from 'node:path';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { initSync, Studio } from '../../src/backend/pkg/xem_studio.js';
import { openBrowserFolder, type DirectoryHandle } from '../../src/platform/browser-files';
import { openNodeFolder } from '../../src/platform/node-files';

let fixtureRoot: string;

beforeEach(async () => {
  fixtureRoot = await fs.mkdtemp(join(tmpdir(), 'xem-studio-files-'));
});

afterEach(async () => {
  await fs.rm(fixtureRoot, { recursive: true, force: true });
});

function echoThroughWasm(value: unknown) {
  initSync({
    module: readFileSync(
      new URL('../../src/backend/pkg/xem_studio_bg.wasm', import.meta.url),
    ),
  });
  const studio = new Studio();
  try {
    const response = JSON.parse(studio.command(JSON.stringify({ op: 'echo', args: value }))) as unknown;
    expect(response).toEqual({ ok: value });
    return (response as { ok: unknown }).ok;
  } finally {
    studio.free();
  }
}

async function openFixture() {
  return openNodeFolder(fixtureRoot, fs, await import('node:path'));
}

describe('wasm command success envelope', () => {
  it('keeps null, scalars and ok/error-shaped JSON as successful echo data', () => {
    const values: unknown[] = [
      null,
      false,
      0,
      '',
      { error: { kind: 'user-data', message: 'not a backend error' } },
      { ok: null, error: { kind: 'nested-user-data', message: 'also ordinary data' } },
    ];

    for (const value of values) expect(echoThroughWasm(value)).toEqual(value);
  });
});

class MemoryFileHandle {
  readonly kind = 'file' as const;
  abortCount = 0;

  constructor(
    readonly name: string,
    public bytes = new Uint8Array(),
    private readonly failWrite = false,
    private readonly failAbort = false,
  ) {}

  async getFile() {
    return new File([this.bytes], this.name);
  }

  async createWritable() {
    const original = this.bytes;
    let pending = new Uint8Array();
    return {
      write: async (data: Uint8Array) => {
        if (this.failWrite) throw new Error('synthetic write failure');
        pending = new Uint8Array(data);
      },
      close: async () => { this.bytes = pending; },
      abort: async () => {
        this.bytes = original;
        this.abortCount += 1;
        if (this.failAbort) throw new Error('synthetic abort cleanup failure');
      },
    };
  }
}

class MemoryDirectoryHandle {
  readonly kind = 'directory' as const;
  readonly children = new Map<string, MemoryDirectoryHandle | MemoryFileHandle>();

  constructor(readonly name: string) {}

  async *values() {
    yield* this.children.values();
  }

  async getDirectoryHandle(name: string) {
    const child = this.children.get(name);
    if (!(child instanceof MemoryDirectoryHandle)) throw new Error(`No directory: ${name}`);
    return child;
  }

  async getFileHandle(name: string, options = { create: false }) {
    const child = this.children.get(name);
    if (child instanceof MemoryFileHandle) return child;
    if (child || !options.create) throw new Error(`No file: ${name}`);
    const created = new MemoryFileHandle(name);
    this.children.set(name, created);
    return created;
  }
}

describe('Node files adapter', () => {
  it('lists folders before files and returns sorted entries', async () => {
    await fs.mkdir(join(fixtureRoot, 'Maps'));
    await fs.mkdir(join(fixtureRoot, 'Audio'));
    await fs.writeFile(join(fixtureRoot, 'zeta.txt'), 'z');
    await fs.writeFile(join(fixtureRoot, 'alpha.txt'), 'a');
    const adapter = await openFixture();

    const entries = await adapter.list();
    expect(entries).toEqual([
      { name: 'Audio', kind: 'directory' },
      { name: 'Maps', kind: 'directory' },
      { name: 'alpha.txt', kind: 'file' },
      { name: 'zeta.txt', kind: 'file' },
    ]);
    expect(echoThroughWasm(entries)).toEqual(entries);
  });

  it('reads a relative file as its exact UTF-8 bytes', async () => {
    const content = '雪だるま 🐉 — field script';
    await fs.mkdir(join(fixtureRoot, 'scripts'));
    await fs.writeFile(join(fixtureRoot, 'scripts', 'intro.txt'), content, 'utf8');
    const adapter = await openFixture();

    const bytes = await adapter.read('scripts/intro.txt');
    const text = new TextDecoder().decode(bytes);
    expect(text).toBe(content);
    expect(echoThroughWasm({ path: 'scripts/intro.txt', text })).toEqual({
      path: 'scripts/intro.txt',
      text: content,
    });
  });

  it('atomically replaces file contents without leaving a partial file', async () => {
    await fs.writeFile(join(fixtureRoot, 'result.json'), '{"old":true}');
    const adapter = await openFixture();
    const bytes = new TextEncoder().encode('{"new":"résultat"}');

    await adapter.writeAtomic('result.json', bytes);
    const text = new TextDecoder().decode(await adapter.read('result.json'));
    const remainingNames = await fs.readdir(fixtureRoot);
    expect(text).toBe('{"new":"résultat"}');
    expect(remainingNames).toEqual(['result.json']);
    expect(echoThroughWasm({ text, remainingNames })).toEqual({
      text: '{"new":"résultat"}',
      remainingNames: ['result.json'],
    });
  });

  it('preserves ENOENT for a missing file', async () => {
    const adapter = await openFixture();
    let missing: { code?: string; message: string } | undefined;
    try {
      await adapter.read('missing.txt');
    } catch (error) {
      missing = error as { code?: string; message: string };
    }

    expect(missing?.code).toBe('ENOENT');
    expect(missing?.message).toContain('missing.txt');
    expect(echoThroughWasm({ code: missing?.code, message: missing?.message })).toEqual({
      code: 'ENOENT',
      message: missing?.message,
    });
  });

  it('rejects traversal and symlinks that point outside the selected folder', async () => {
    const outsideRoot = await fs.mkdtemp(join(tmpdir(), 'xem-studio-outside-'));
    try {
      await fs.writeFile(join(outsideRoot, 'secret.txt'), 'outside');
      await fs.symlink(outsideRoot, join(fixtureRoot, 'outside-link'), 'dir');
      await fs.symlink(join(fixtureRoot, 'owned.txt'), join(fixtureRoot, 'file-link'));
      const adapter = await openFixture();
      const errors: string[] = [];

      await expect(adapter.read('../secret.txt')).rejects.toThrow('relative project path');
      await expect(adapter.read('outside-link/secret.txt')).rejects.toThrow('escapes the opened folder');
      await expect(adapter.writeAtomic('outside-link/new.txt', new Uint8Array([1])))
        .rejects.toThrow('escapes the opened folder');
      await expect(adapter.writeAtomic('file-link', new Uint8Array([1])))
        .rejects.toThrow('Cannot replace a symlink');

      errors.push(
        'traversal rejected',
        'outside symlink read rejected',
        'outside symlink write rejected',
        'symlink replacement rejected',
      );
      expect(echoThroughWasm({ errors, outsideSecretStillPresent: await fs.readFile(join(outsideRoot, 'secret.txt'), 'utf8') }))
        .toEqual({
          errors,
          outsideSecretStillPresent: 'outside',
        });
    } finally {
      await fs.rm(outsideRoot, { recursive: true, force: true });
    }
  });

  it('preserves a real rename failure when temporary-file cleanup also fails', async () => {
    await fs.mkdir(join(fixtureRoot, 'destination-directory'));
    const cleanupFailureFs = new Proxy(fs, {
      get(target, property, receiver) {
        if (property === 'unlink') {
          return async (filePath: PathLike) => {
            if (String(filePath).endsWith('.partial')) {
              throw new Error('synthetic unlink cleanup failure');
            }
            return target.unlink(filePath);
          };
        }
        const value = Reflect.get(target, property, receiver) as unknown;
        return typeof value === 'function' ? value.bind(target) : value;
      },
    }) as typeof fs;
    const adapter = await openNodeFolder(fixtureRoot, cleanupFailureFs, path);
    let failure: unknown;
    try {
      await adapter.writeAtomic('destination-directory', new Uint8Array([1, 2, 3]));
    } catch (error) {
      failure = error;
    }

    expect(failure).toBeInstanceOf(AggregateError);
    const aggregate = failure as AggregateError;
    const details = aggregate.errors.map((error) => ({
      name: error instanceof Error ? error.name : typeof error,
      message: error instanceof Error ? error.message : String(error),
    }));
    expect(details[0].message).toContain('rename');
    expect(details[1].message).toBe('synthetic unlink cleanup failure');
    const partials = (await fs.readdir(fixtureRoot)).filter((name) => name.endsWith('.partial'));
    expect(partials).toHaveLength(1);
    const diagnostic = { name: aggregate.name, message: aggregate.message, details };
    expect(echoThroughWasm(diagnostic)).toEqual(diagnostic);
  });
});

describe('browser files adapter', () => {
  it('lists, reads and writes through directory handles', async () => {
    const root = new MemoryDirectoryHandle('browser-fixture');
    const scripts = new MemoryDirectoryHandle('scripts');
    const source = new MemoryFileHandle('intro.txt', new TextEncoder().encode('field script'));
    root.children.set('scripts', scripts);
    root.children.set('readme.txt', new MemoryFileHandle('readme.txt', new TextEncoder().encode('hello')));
    scripts.children.set('intro.txt', source);
    const adapter = openBrowserFolder(root as DirectoryHandle);

    const entries = await adapter.list();
    expect(entries).toEqual([
      { name: 'scripts', kind: 'directory' },
      { name: 'readme.txt', kind: 'file' },
    ]);
    const text = new TextDecoder().decode(await adapter.read('scripts/intro.txt'));
    await adapter.writeAtomic('scripts/intro.txt', new TextEncoder().encode('updated'));
    expect(source.bytes).toEqual(new TextEncoder().encode('updated'));
    expect(echoThroughWasm({ entries, text, written: new TextDecoder().decode(source.bytes) })).toEqual({
      entries,
      text: 'field script',
      written: 'updated',
    });
  });

  it('aborts a failed atomic write and preserves the original bytes', async () => {
    const root = new MemoryDirectoryHandle('browser-fixture');
    const target = new MemoryFileHandle('locked.txt', new TextEncoder().encode('original'), true);
    root.children.set(target.name, target);
    const adapter = openBrowserFolder(root as DirectoryHandle);

    await expect(adapter.writeAtomic('locked.txt', new TextEncoder().encode('partial')))
      .rejects.toThrow('synthetic write failure');
    const preserved = new TextDecoder().decode(await adapter.read('locked.txt'));
    expect(target.abortCount).toBe(1);
    expect(preserved).toBe('original');
    expect(echoThroughWasm({ abortCount: target.abortCount, preserved })).toEqual({
      abortCount: 1,
      preserved: 'original',
    });
  });

  it('preserves the browser write failure when abort cleanup also fails', async () => {
    const root = new MemoryDirectoryHandle('browser-fixture');
    const target = new MemoryFileHandle('locked.txt', new TextEncoder().encode('original'), true, true);
    root.children.set(target.name, target);
    const adapter = openBrowserFolder(root as DirectoryHandle);
    let failure: unknown;
    try {
      await adapter.writeAtomic('locked.txt', new TextEncoder().encode('partial'));
    } catch (error) {
      failure = error;
    }

    expect(failure).toBeInstanceOf(AggregateError);
    const aggregate = failure as AggregateError;
    const details = aggregate.errors.map((error) => ({
      name: error instanceof Error ? error.name : typeof error,
      message: error instanceof Error ? error.message : String(error),
    }));
    expect(details).toEqual([
      { name: 'Error', message: 'synthetic write failure' },
      { name: 'Error', message: 'synthetic abort cleanup failure' },
    ]);
    const preserved = new TextDecoder().decode(await adapter.read('locked.txt'));
    expect(target.abortCount).toBe(1);
    expect(preserved).toBe('original');
    const diagnostic = { name: aggregate.name, message: aggregate.message, details, preserved };
    expect(echoThroughWasm(diagnostic)).toEqual(diagnostic);
  });
});
