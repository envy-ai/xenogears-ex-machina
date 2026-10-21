import { posix } from 'node:path';
import type { FileHandle } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
import {
  openDiscSession,
  openNodeCueDiscReader,
  openNodeDiscReader,
  type DiscByteReader,
} from '../../src/backend/discs';
import type { Backend } from '../../src/backend/service';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function cueFs(
  cueText: string,
  open: ReturnType<typeof vi.fn>,
): Parameters<typeof openNodeCueDiscReader>[2] {
  return {
    stat: vi.fn(async () => ({ isFile: () => true, size: Buffer.byteLength(cueText) })),
    readFile: vi.fn(async () => cueText),
    open,
  } as unknown as Parameters<typeof openNodeCueDiscReader>[2];
}

function cueBackend(): Backend {
  return {
    command: vi.fn((request: string) => {
      const parsed = JSON.parse(request) as { op: string; args: { text?: string } };
      if (parsed.op !== 'disc.cue' || !parsed.args.text?.includes('INDEX 01 00:00:01')) {
        throw new Error('unexpected CUE command');
      }
      return JSON.stringify({ ok: { file: 'disc.bin', number: 1, start: 1, end: 3 } });
    }),
  };
}

function handle(size: number) {
  const stat = vi.fn(async () => ({ isFile: () => true, size }));
  const close = vi.fn(async () => undefined);
  const read = vi.fn(async (
    buffer: Uint8Array,
    offset: number,
    length: number,
    position: number,
  ) => {
    for (let index = 0; index < length; index += 1) {
      buffer[offset + index] = (position + index) & 0xff;
    }
    return { bytesRead: length, buffer };
  });
  return {
    value: { stat, close, read } as unknown as FileHandle,
    stat,
    close,
    read,
  };
}

describe('disc file acquisition', () => {
  it('closes a Node handle that arrives after the caller aborts during open', async () => {
    const pending = deferred<FileHandle>();
    const open = vi.fn(() => pending.promise);
    const controller = new AbortController();
    const reason = new Error('cancel open');
    const opening = openNodeDiscReader(
      '/disc/game.bin',
      { open } as unknown as Parameters<typeof openNodeDiscReader>[1],
      { signal: controller.signal, timeoutMs: 2_000 },
    );

    await vi.waitFor(() => expect(open).toHaveBeenCalledOnce());
    controller.abort(reason);
    await expect(opening).rejects.toBe(reason);

    const late = handle(4_704);
    pending.resolve(late.value);
    await vi.waitFor(() => expect(late.close).toHaveBeenCalledOnce());
    expect(late.stat).not.toHaveBeenCalled();
  });

  it('reports the original error when closing a late Node handle fails', async () => {
    const pending = deferred<FileHandle>();
    const open = vi.fn(() => pending.promise);
    const controller = new AbortController();
    const opening = openNodeDiscReader(
      '/disc/game.bin',
      { open } as unknown as Parameters<typeof openNodeDiscReader>[1],
      { signal: controller.signal, timeoutMs: 2_000 },
    );
    await vi.waitFor(() => expect(open).toHaveBeenCalledOnce());
    controller.abort(new Error('cancel open before failed cleanup'));
    await expect(opening).rejects.toThrow('cancel open before failed cleanup');

    const closeFailure = new Error('late handle close failed');
    const late = handle(4_704);
    late.close.mockRejectedValue(closeFailure);
    const callbacks: VoidFunction[] = [];
    const queueSpy = vi.spyOn(globalThis, 'queueMicrotask').mockImplementation(callback => {
      callbacks.push(callback);
    });
    try {
      pending.resolve(late.value);
      await vi.waitFor(() => expect(queueSpy).toHaveBeenCalledOnce());
      let thrown: unknown;
      try {
        callbacks[0]?.();
      } catch (error) {
        thrown = error;
      }
      expect(thrown).toBe(closeFailure);
      expect((thrown as Error).stack).toBe(closeFailure.stack);
    } finally {
      queueSpy.mockRestore();
    }
  });

  it('closes an acquired Node handle when stat is still pending at abort', async () => {
    const opened = handle(4 * 2_352);
    const pending = deferred<Awaited<ReturnType<typeof opened.stat>>>();
    opened.stat.mockImplementation(() => pending.promise);
    const controller = new AbortController();
    const reason = new Error('cancel stat');
    const opening = openNodeDiscReader(
      '/disc/game.bin',
      { open: vi.fn(async () => opened.value) } as unknown as Parameters<typeof openNodeDiscReader>[1],
      { signal: controller.signal, timeoutMs: 2_000 },
    );

    await vi.waitFor(() => expect(opened.stat).toHaveBeenCalledOnce());
    controller.abort(reason);
    await expect(opening).rejects.toBe(reason);
    expect(opened.close).toHaveBeenCalledOnce();
  });

  it('closes a late Node handle when CUE companion acquisition is aborted', async () => {
    const text = 'FILE "disc.bin" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:01\n';
    const pending = deferred<FileHandle>();
    const open = vi.fn(() => pending.promise);
    const controller = new AbortController();
    const reason = new Error('cancel CUE companion open');
    const opening = openNodeCueDiscReader(
      cueBackend(),
      '/disc/game.cue',
      cueFs(text, open),
      posix,
      { signal: controller.signal, timeoutMs: 2_000 },
    );

    await vi.waitFor(() => expect(open).toHaveBeenCalledOnce());
    expect(open).toHaveBeenCalledWith('/disc/disc.bin', 'r');
    controller.abort(reason);
    await expect(opening).rejects.toBe(reason);

    const late = handle(4_704);
    pending.resolve(late.value);
    await vi.waitFor(() => expect(late.close).toHaveBeenCalledOnce());
    expect(late.stat).not.toHaveBeenCalled();
  });

  it('maps CUE INDEX 01 sector bounds to bounded byte reads', async () => {
    const text = 'FILE "disc.bin" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:01\n';
    const opened = handle(4 * 2_352);
    const open = vi.fn(async () => opened.value);
    const reader = await openNodeCueDiscReader(
      cueBackend(),
      '/disc/game.cue',
      cueFs(text, open),
      posix,
    );

    expect(reader.name).toBe('/disc/game.cue');
    expect(reader.bytes).toBe(2 * 2_352);
    const bytes = await reader.read(0, 4);
    expect([...bytes]).toEqual([0x30, 0x31, 0x32, 0x33]);
    expect(opened.read).toHaveBeenCalledWith(expect.any(Uint8Array), 0, 4, 2_352);
    await reader.close();
    expect(opened.close).toHaveBeenCalledOnce();
  });
});

describe('binary disc session transport', () => {
  it('serializes bounded Uint8Array range fills before polling the backend', async () => {
    const events: string[] = [];
    let activeReads = 0;
    let maximumConcurrentReads = 0;
    const reader: DiscByteReader = {
      name: 'fixture.bin',
      bytes: 4,
      async read(start, end) {
        activeReads += 1;
        maximumConcurrentReads = Math.max(maximumConcurrentReads, activeReads);
        events.push(`read:${start}-${end}`);
        await Promise.resolve();
        activeReads -= 1;
        return Uint8Array.from({ length: end - start }, (_, index) => start + index);
      },
      async close() { events.push('reader.close'); },
    };
    const backend: Backend = {
      command: vi.fn((request: string) => {
        const parsed = JSON.parse(request) as { op: string };
        events.push(`command:${parsed.op}`);
        if (parsed.op === 'disc.open') {
          return JSON.stringify({ ok: {
            session: 7,
            name: 'fixture.bin',
            bytes: 4,
            state: 'pending',
            phase: 'identifying',
            identity: null,
            missing: [[0, 4]],
            chunk_bytes: 2,
            chunk_budget: 2,
            hunk_budget: 0,
            resident_bytes: 0,
          } });
        }
        if (parsed.op === 'disc.poll') {
          return JSON.stringify({ ok: {
            session: 7,
            name: 'fixture.bin',
            bytes: 4,
            state: 'ready',
            phase: 'ready',
            identity: null,
            missing: [],
            chunk_bytes: 2,
            chunk_budget: 2,
            hunk_budget: 0,
            resident_bytes: 4,
          } });
        }
        if (parsed.op === 'disc.close') return JSON.stringify({ ok: { session: 7, closed: true } });
        throw new Error(`unexpected operation ${parsed.op}`);
      }),
      disc_fill: vi.fn((session: number, offset: number, bytes: Uint8Array) => {
        events.push(`fill:${session}:${offset}:${[...bytes].join(',')}`);
        return JSON.stringify({ ok: { session, filled: bytes.length } });
      }),
    };

    const session = await openDiscSession(backend, reader);
    expect(session.status.state).toBe('ready');
    expect(maximumConcurrentReads).toBe(1);
    expect(events).toEqual([
      'command:disc.open',
      'read:0-2',
      'fill:7:0:0,1',
      'read:2-4',
      'fill:7:2:2,3',
      'command:disc.poll',
    ]);
    await session.close();
    expect(events.slice(-1)).toEqual(['reader.close']);
  });
});
