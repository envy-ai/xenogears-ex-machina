import { describe, expect, it, vi } from 'vitest';
import { openServerDiscReader, serverDiscReader, listServerDiscs, type ServerDisc } from '../../src/backend/remote-discs';
import type { Backend } from '../../src/backend/service';

const image: ServerDisc = { id: 'ZGlzYy5iaW4', name: 'disc.bin', bytes: 4, etag: '"sha256-abc"' };
const success = (ok: unknown) => JSON.stringify({ ok });

function response(
  bytes: Uint8Array,
  start: number,
  total: number,
  etag = image.etag,
  expectedLength = bytes.length,
  rangeEnd = start + expectedLength - 1,
): Response {
  return new Response(bytes.slice().buffer as ArrayBuffer, { status: 206, headers: {
    'Content-Range': `bytes ${start}-${rangeEnd}/${total}`,
    'Content-Length': String(expectedLength), ETag: etag,
  } });
}

describe('read-only server disc transport', () => {
  it('validates and retains the enabled catalogue identity', async () => {
    const fetcher = vi.fn(async () => new Response(success({ enabled: true, files: [image] }), { status: 200 }));
    await expect(listServerDiscs(fetcher)).resolves.toEqual({ enabled: true, files: [image] });
    expect(fetcher).toHaveBeenCalledWith('/__studio/discs', expect.objectContaining({ cache: 'no-store', credentials: 'same-origin' }));
  });

  it('accepts an explicitly disabled empty catalogue', async () => {
    const fetcher = vi.fn(async () => new Response(success({ enabled: false, files: [] }), { status: 200 }));
    await expect(listServerDiscs(fetcher)).resolves.toEqual({ enabled: false, files: [] });
  });

  it.each([
    { enabled: false, files: [image] },
    { enabled: true, files: [{ ...image, name: '../disc.bin' }] },
    { enabled: true, files: [{ ...image, id: 'bad/id' }] },
    { enabled: true, files: [{ ...image, etag: 'unquoted' }] },
    { enabled: true, files: [image, image] },
  ])('rejects malformed or ambiguous catalogue data', async catalogue => {
    const fetcher = vi.fn(async () => new Response(success(catalogue), { status: 200 }));
    await expect(listServerDiscs(fetcher)).rejects.toThrow();
  });

  it('requests only the exact bounded range and checks response identity', async () => {
    const fetcher = vi.fn(async () => response(Uint8Array.of(8, 9), 1, 4));
    const reader = serverDiscReader(image, fetcher);
    await expect(reader.read(1, 3)).resolves.toEqual(Uint8Array.of(8, 9));
    expect(fetcher).toHaveBeenCalledWith('/__studio/discs/ZGlzYy5iaW4', expect.objectContaining({
      cache: 'no-store', credentials: 'same-origin',
      headers: { Range: 'bytes=1-2', 'If-Match': image.etag },
    }));
    await reader.close();
    await expect(reader.read(0, 1)).rejects.toThrow('closed');
  });

  it.each([
    ['status', () => new Response(null, { status: 412 })],
    ['range', () => response(Uint8Array.of(8, 9), 0, 4)],
    ['etag', () => response(Uint8Array.of(8, 9), 1, 4, '"sha256-changed"')],
    ['short body', () => response(Uint8Array.of(8), 1, 4, image.etag, 2, 2)],
    ['oversized body', () => response(Uint8Array.of(8, 9, 10), 1, 4, image.etag, 2, 2)],
  ])('rejects a mismatched %s response', async (_label, makeResponse) => {
    const fetcher = vi.fn(async () => makeResponse());
    await expect(serverDiscReader(image, fetcher).read(1, 3)).rejects.toThrow();
  });

  it('surfaces a structured server error for stale file identity', async () => {
    const fetcher = vi.fn(async () => new Response(JSON.stringify({ error: { kind: 'changed', message: 'file changed since catalogue read' } }), {
      status: 412, headers: { 'Content-Type': 'application/json' },
    }));
    await expect(serverDiscReader(image, fetcher).read(0, 1)).rejects.toThrow('file changed since catalogue read');
  });

  it('parses CUE through the backend and reads only the catalogued companion slice', async () => {
    const cueText = 'FILE "disc.bin" BINARY\n';
    const cueBytes = new TextEncoder().encode(cueText);
    const cue: ServerDisc = { id: 'ZGlzYy5jdWU', name: 'disc.cue', bytes: cueBytes.length, etag: '"cue-v1"' };
    const companion: ServerDisc = { ...image, bytes: 4 * 2_352 };
    const catalogue = { enabled: true, files: [cue, companion] };
    const commands: string[] = [];
    const backend: Backend = { command: request => {
      commands.push(request);
      return success({ file: 'disc.bin', number: 1, start: 1, end: 3 });
    } };
    const fetcher = vi.fn(async (input: RequestInfo | URL) => String(input).endsWith(cue.id)
      ? response(cueBytes, 0, cue.bytes, cue.etag)
      : response(Uint8Array.of(5, 6, 7, 8), 2_352, companion.bytes));
    const reader = await openServerDiscReader(backend, cue, catalogue, fetcher);

    expect(JSON.parse(commands[0] ?? '{}')).toEqual({ op: 'disc.cue', args: { text: 'FILE "disc.bin" BINARY\n' } });
    expect(reader.name).toBe(cue.name);
    expect(reader.bytes).toBe(2 * 2_352);
    await expect(reader.read(0, 4)).resolves.toEqual(Uint8Array.of(5, 6, 7, 8));
    expect(fetcher).toHaveBeenLastCalledWith(`/__studio/discs/${companion.id}`, expect.objectContaining({
      headers: { Range: 'bytes=2352-2355', 'If-Match': companion.etag },
    }));
    await reader.close();
  });

  it('requires a current enabled catalogue and a safe CUE companion', async () => {
    const cue: ServerDisc = { id: 'Y3Vl', name: 'disc.cue', bytes: 5, etag: '"cue"' };
    const backend: Backend = { command: () => success({ file: '../escape.bin', number: 1, start: 0, end: null }) };
    await expect(openServerDiscReader(backend, cue, { enabled: false, files: [] }, vi.fn()))
      .rejects.toThrow('not in the current catalogue');
    const fetcher = vi.fn(async () => response(Uint8Array.of(1, 2, 3, 4, 5), 0, cue.bytes, cue.etag));
    await expect(openServerDiscReader(backend, cue, { enabled: true, files: [cue] }, fetcher))
      .rejects.toThrow('CUE companion must be a file');
  });

  it('aborts the CUE catalogue read when its bounded request signal expires', async () => {
    const cue: ServerDisc = { id: 'Y3Vl', name: 'disc.cue', bytes: 5, etag: '"cue"' };
    const catalogue = { enabled: true, files: [cue] };
    const backend: Backend = { command: () => success({ file: 'disc.bin', number: 1, start: 0, end: null }) };
    const timeout = new AbortController();
    const timeoutSpy = vi.spyOn(AbortSignal, 'timeout').mockReturnValue(timeout.signal);
    let capturedSignal: AbortSignal | undefined;
    const fetcher = vi.fn((_input: RequestInfo | URL, init?: RequestInit) => new Promise<Response>((_resolve, reject) => {
      capturedSignal = init?.signal as AbortSignal | undefined;
      capturedSignal?.addEventListener('abort', () => reject(capturedSignal?.reason), { once: true });
    }));
    try {
      const opening = openServerDiscReader(backend, cue, catalogue, fetcher);
      await vi.waitFor(() => expect(capturedSignal).toBeDefined());
      expect(capturedSignal?.aborted).toBe(false);
      timeout.abort(new DOMException('request deadline elapsed', 'TimeoutError'));
      await expect(opening).rejects.toThrow('request deadline elapsed');
      expect(capturedSignal?.aborted).toBe(true);
    } finally {
      timeoutSpy.mockRestore();
    }
  });
});
