import { cueTrack, MAX_CUE_BYTES, readRange, sliceDiscReader, type DiscByteReader } from './discs';
import { readCommandResult, type Backend } from './service';
import { cleanupAfterFailure } from '../platform/errors';

const endpoint = '/__studio/discs';
const REQUEST_TIMEOUT_MS = 30_000;
export interface ServerDisc { id: string; name: string; bytes: number; etag: string }
export interface ServerDiscs { enabled: boolean; files: ServerDisc[] }
type Fetch = typeof globalThis.fetch;

function validateFile(file: ServerDisc): void {
  if (!file || typeof file.id !== 'string' || !/^[A-Za-z0-9_-]+$/.test(file.id)
    || typeof file.name !== 'string' || !file.name || file.name.startsWith('.') || /[/\\]/.test(file.name)
    || !/\.(bin|chd|cue)$/i.test(file.name) || !Number.isSafeInteger(file.bytes) || file.bytes < 0
    || typeof file.etag !== 'string' || !/^"[^"\r\n]+"$/.test(file.etag)) {
    throw new Error('Invalid server disc catalogue entry');
  }
}

async function jsonResult<T>(response: Response): Promise<T> {
  const result = readCommandResult<T>(await response.text());
  if (!response.ok) throw new Error(`Server disc request failed with HTTP ${response.status}`);
  return result;
}

/** The configured server exposes names/identities only; no whole images are fetched. */
export async function listServerDiscs(fetcher: Fetch = globalThis.fetch.bind(globalThis)): Promise<ServerDiscs> {
  const response = await fetcher(endpoint, { cache: 'no-store', credentials: 'same-origin', signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
  const catalogue = await jsonResult<ServerDiscs>(response);
  if (!catalogue || typeof catalogue.enabled !== 'boolean' || !Array.isArray(catalogue.files)) throw new Error('Invalid server disc catalogue');
  const names = new Set<string>(); const ids = new Set<string>();
  for (const file of catalogue.files) {
    validateFile(file);
    if (ids.has(file.id) || names.has(file.name)) throw new Error('Duplicate server disc catalogue entry');
    ids.add(file.id); names.add(file.name);
  }
  if (!catalogue.enabled && catalogue.files.length) throw new Error('Disabled server disc catalogue contains files');
  return catalogue;
}

/** Each response must match the listed file identity and exact requested range. */
export function serverDiscReader(file: ServerDisc, fetcher: Fetch = globalThis.fetch.bind(globalThis)): DiscByteReader {
  validateFile(file);
  const controller = new AbortController();
  let closed = false;
  return {
    name: file.name, bytes: file.bytes,
    async read(start, end) {
      if (closed) throw new Error('Server disc reader is closed');
      readRange(start, end, file.bytes);
      if (start === end) return new Uint8Array();
      const response = await fetcher(`${endpoint}/${encodeURIComponent(file.id)}`, {
        cache: 'no-store', credentials: 'same-origin', signal: AbortSignal.any([controller.signal, AbortSignal.timeout(REQUEST_TIMEOUT_MS)]),
        headers: { Range: `bytes=${start}-${end - 1}`, 'If-Match': file.etag },
      });
      if (response.status !== 206) {
        if (response.headers.get('content-type')?.includes('application/json')) await jsonResult(response);
        return cleanupAfterFailure(new Error(`Server disc range requires HTTP 206, received ${response.status}`), [async () => response.body?.cancel()]);
      }
      try {
        if (response.headers.get('content-range') !== `bytes ${start}-${end - 1}/${file.bytes}`
          || response.headers.get('etag') !== file.etag
          || response.headers.get('content-length') !== String(end - start)) throw new Error('Server disc response identity or range does not match the request');
        if (!response.body) throw new Error('Server disc range response has no body');
      } catch (error) { return cleanupAfterFailure(error, [async () => response.body?.cancel()]); }
      const stream = response.body!.getReader();
      const bytes = new Uint8Array(end - start);
      let offset = 0;
      try {
        for (;;) {
          const chunk = await stream.read();
          if (chunk.done) break;
          if (offset + chunk.value.length > bytes.length) throw new Error('Server disc range response exceeds the requested length');
          bytes.set(chunk.value, offset); offset += chunk.value.length;
        }
        if (offset !== bytes.length) throw new Error(`Short server disc read: expected ${bytes.length}, got ${offset}`);
        return bytes;
      } catch (error) { return cleanupAfterFailure(error, [() => stream.cancel()]); }
      finally { stream.releaseLock(); }
    },
    async close() { closed = true; controller.abort(); },
  };
}

/** CUE interpretation remains in Rust; companions must be in this same flat catalogue. */
export async function openServerDiscReader(backend: Backend, file: ServerDisc, catalogue: ServerDiscs, fetcher: Fetch = globalThis.fetch.bind(globalThis)): Promise<DiscByteReader> {
  if (!catalogue.enabled || !catalogue.files.some(item => item.id === file.id && item.etag === file.etag && item.name === file.name && item.bytes === file.bytes)) throw new Error('Server disc is not in the current catalogue');
  if (!/\.cue$/i.test(file.name)) return serverDiscReader(file, fetcher);
  if (file.bytes > MAX_CUE_BYTES) throw new Error('CUE sheet exceeds 64 KiB');
  const cue = serverDiscReader(file, fetcher);
  let track;
  try { track = cueTrack(backend, new TextDecoder('utf-8', { fatal: true }).decode(await cue.read(0, file.bytes))); }
  finally { await cue.close(); }
  if (/[/\\]/.test(track.file) || track.file.startsWith('.')) throw new Error('CUE companion must be a file in the configured server disc folder');
  const companion = catalogue.files.find(item => item.name === track.file && /\.bin$/i.test(item.name));
  if (!companion) throw new Error(`Server CUE companion is unavailable: ${track.file}`);
  const reader = serverDiscReader(companion, fetcher);
  try { return sliceDiscReader(reader, track, file.name); }
  catch (error) { return cleanupAfterFailure(error, [() => reader.close()]); }
}
