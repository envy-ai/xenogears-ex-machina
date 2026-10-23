import type * as Fs from 'node:fs/promises';
import type * as Path from 'node:path';
import { readCommandResult, type Backend } from './service';
import { cleanupAfterFailure } from '../platform/errors';

const DEFAULT_TIMEOUT_MS = 120_000;
const MAX_FILL_BYTES = 8 << 20;
export const MAX_CUE_BYTES = 64 << 10;
const SECTOR_BYTES = 2352;

export interface DiscByteReader {
  readonly name: string;
  readonly bytes: number;
  /** Read exactly [start,end); no whole-image buffering. */
  read(start: number, end: number): Promise<Uint8Array>;
  close(): Promise<void>;
}

export interface DiscProgress {
  session: number;
  state: 'pending' | 'ready';
  phase?: string;
  completed?: number;
  total?: number;
}

export interface DiscOptions {
  /** Defaults to 120 seconds; includes queue wait, checked on taking ownership. */
  timeoutMs?: number;
  signal?: AbortSignal;
  onProgress?: (progress: DiscProgress) => void;
}

export interface DiscIdentity {
  disc: number | null;
  serial: string | null;
  boot_path: string;
  boot_lba: number;
  boot_sha256: string;
  executable_bytes: number;
  test_fixture: boolean;
}

export interface DiscStatus extends DiscProgress {
  name: string;
  bytes: number;
  phase: string;
  identity: DiscIdentity | null;
  missing: [number, number][];
  chunk_bytes: number;
  chunk_budget: number;
  hunk_budget: number;
  resident_bytes: number;
}

export interface DiscMap {
  map: number;
  file: number;
  slot: number;
  placeholder: boolean;
  actor_count: number | null;
  script_size: number | null;
  stream_size: number | null;
}

export interface EventComponent {
  bytes: number[];
  declared_size: number;
  bytecode_offset: number;
  component_end: number;
  actor_count: number;
}

export interface MapEvents {
  session: number;
  map: number;
  state: 'ready';
  placeholder: boolean;
  component: EventComponent | null;
}

export interface MapBundle { session:number; map:number; slot:number; state: 'ready'; placeholder:boolean; bytes:number[] }

export interface CueTrack {
  file: string;
  number: number;
  start: number;
  end: number | null;
}

interface Pending extends DiscProgress { missing: [number, number][] }
interface Scope extends DiscOptions { deadline: number }
type Source = { kind: 'pull'; reader: DiscByteReader } | { kind: 'native'; path: string };

function safeInteger(value: number, label: string): void {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`${label} must be a nonnegative safe integer`);
}

function range(start: number, end: number, bytes: number): void {
  safeInteger(start, 'Range start');
  safeInteger(end, 'Range end');
  if (start > end || end > bytes) throw new Error(`Disc range [${start},${end}) is outside ${bytes} bytes`);
}

export function readRange(start: number, end: number, bytes: number): void {
  range(start, end, bytes);
  if (end - start > MAX_FILL_BYTES) throw new Error(`Read at most ${MAX_FILL_BYTES} disc bytes at once`);
}

function rejectCue(name: string): void {
  if (/\.cue$/i.test(name)) {
    throw new Error('Use the CUE reader factory with its companion BIN, or open the CUE by path through a native backend. The ordinary range reader needs a BIN or CHD image.');
  }
}

/** Browser Files stay local; only the requested Blob slices enter memory. */
export function browserDiscReader(file: File): DiscByteReader {
  rejectCue(file.name);
  safeInteger(file.size, 'Disc file length');
  let closed = false;
  return {
    name: file.name,
    bytes: file.size,
    async read(start, end) {
      if (closed) throw new Error('Disc reader is closed');
      readRange(start, end, file.size);
      const bytes = new Uint8Array(await file.slice(start, end).arrayBuffer());
      if (bytes.length !== end - start) throw new Error(`Short disc read at ${start}: expected ${end - start}, got ${bytes.length}`);
      return bytes;
    },
    async close() { closed = true; },
  };
}

/** Inject Node fs from the established NW host; opening/stat obey the deadline
 * and signal too. A handle arriving after cancellation is closed on arrival.
 */
export async function openNodeDiscReader(path: string, fs: Pick<typeof Fs, 'open'>, options: DiscOptions = {}): Promise<DiscByteReader> {
  rejectCue(path);
  const scope = operationScope({}, options);
  const handle = await acquire(() => fs.open(path, 'r'), file => file.close(), scope);
  try {
    const stat = await bounded(() => handle.stat(), scope);
    check(scope);
    if (!stat.isFile()) throw new Error(`Disc path is not a file: ${path}`);
    safeInteger(stat.size, 'Disc file length');
    let closed = false;
    return {
      name: path,
      bytes: stat.size,
      async read(start, end) {
        if (closed) throw new Error('Disc reader is closed');
        readRange(start, end, stat.size);
        const data = new Uint8Array(end - start);
        let read = 0;
        while (read < data.length) {
          const result = await handle.read(data, read, data.length - read, start + read);
          if (result.bytesRead === 0) throw new Error(`Short disc read at ${start + read}: expected ${end - start} bytes`);
          read += result.bytesRead;
        }
        return data;
      },
      async close() {
        if (!closed) {
          closed = true;
          await handle.close();
        }
      },
    };
  } catch (error) {
    return cleanupAfterFailure(error, [() => handle.close()]);
  }
}

export function cueTrack(backend: Backend, text: string): CueTrack {
  if (new TextEncoder().encode(text).length > MAX_CUE_BYTES) throw new Error('CUE sheet exceeds 64 KiB');
  const track = command<CueTrack>(backend, 'disc.cue', { text });
  if (typeof track.file !== 'string' || !track.file) throw new Error('Invalid CUE companion filename');
  safeInteger(track.number, 'CUE track number');
  safeInteger(track.start, 'CUE track start');
  if (track.end !== null) safeInteger(track.end, 'CUE track end');
  if (track.end !== null && track.end < track.start) throw new Error('CUE track ends before its start');
  return track;
}

/** Map logical track byte zero to INDEX 01, using Rust's parsed sector bounds.
 * Takes ownership of the underlying reader after successful construction.
 */
export function sliceDiscReader(reader: DiscByteReader, track: CueTrack, name = reader.name): DiscByteReader {
  safeInteger(reader.bytes, 'Disc file length');
  safeInteger(track.start, 'CUE track start');
  if (track.end !== null) safeInteger(track.end, 'CUE track end');
  const start = track.start * SECTOR_BYTES;
  const end = track.end === null ? reader.bytes : track.end * SECTOR_BYTES;
  range(start, end, reader.bytes);
  if ((end - start) % SECTOR_BYTES !== 0) throw new Error('CUE track is not whole 2352-byte sectors');
  let closed = false;
  return {
    name,
    bytes: end - start,
    async read(from, to) {
      if (closed) throw new Error('Disc reader is closed');
      readRange(from, to, end - start);
      return reader.read(start + from, start + to);
    },
    async close() {
      if (!closed) {
        closed = true;
        await reader.close();
      }
    },
  };
}

/** The caller selects the CUE and its referenced BIN; parsing stays in Rust. */
export async function browserCueDiscReader(backend: Backend, cue: File, companion: File, options: DiscOptions = {}): Promise<DiscByteReader> {
  const scope = operationScope({}, options);
  check(scope);
  if (cue.size > MAX_CUE_BYTES) throw new Error('CUE sheet exceeds 64 KiB');
  const track = cueTrack(backend, await bounded(() => cue.text(), scope));
  const filename = track.file.split(/[/\\]/).at(-1);
  if (companion.name !== filename) throw new Error(`CUE requires companion ${track.file}, received ${companion.name}`);
  const reader = browserDiscReader(companion);
  try {
    check(scope);
    return sliceDiscReader(reader, track, cue.name);
  } catch (error) { return cleanupAfterFailure(error, [() => reader.close()]); }
}

/** NW.js resolves the Rust-parsed FILE relative to the selected CUE path. */
export async function openNodeCueDiscReader(
  backend: Backend,
  cuePath: string,
  fs: Pick<typeof Fs, 'open' | 'readFile' | 'stat'>,
  path: Pick<typeof Path, 'dirname' | 'resolve'>,
  options: DiscOptions = {},
): Promise<DiscByteReader> {
  const scope = operationScope({}, options);
  const stat = await bounded(() => fs.stat(cuePath), scope);
  if (!stat.isFile()) throw new Error(`CUE path is not a file: ${cuePath}`);
  if (stat.size > MAX_CUE_BYTES) throw new Error('CUE sheet exceeds 64 KiB');
  const track = cueTrack(backend, await bounded(() => fs.readFile(cuePath, 'utf8'), scope));
  check(scope);
  const reader = await acquire(
    () => openNodeDiscReader(path.resolve(path.dirname(cuePath), track.file), fs, { ...options, timeoutMs: scope.deadline - Date.now() }),
    opened => opened.close(),
    scope,
  );
  try {
    check(scope);
    return sliceDiscReader(reader, track, cuePath);
  } catch (error) { return cleanupAfterFailure(error, [() => reader.close()]); }
}

function operationScope(defaults: DiscOptions, options: DiscOptions): Scope {
  const settings = { ...defaults, ...options };
  const timeout = settings.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  if (!Number.isFinite(timeout) || timeout <= 0 || timeout > 2_147_483_647) throw new Error('Disc operation timeout must be finite, positive, and at most 2147483647 ms');
  return { ...settings, deadline: Date.now() + timeout };
}

function check(scope: Scope): void {
  scope.signal?.throwIfAborted();
  if (Date.now() >= scope.deadline) throw new Error('Disc operation timed out');
}

/** Bound asynchronous reads too, including a File/Node read still in flight. */
async function bounded<T>(work: () => Promise<T>, scope: Scope): Promise<T> {
  check(scope);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let abort: (() => void) | undefined;
  try {
    const interrupted = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error('Disc operation timed out')), scope.deadline - Date.now());
      abort = () => reject(scope.signal?.reason ?? new DOMException('Disc operation aborted', 'AbortError'));
      scope.signal?.addEventListener('abort', abort, { once: true });
    });
    return await Promise.race([interrupted, Promise.resolve().then(() => {
      check(scope);
      return work();
    })]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    if (abort) scope.signal?.removeEventListener('abort', abort);
  }
}

/** Racing acquisition must retain ownership of a late success. The initiating
 * caller has already received its timeout/abort, so a late cleanup failure is
 * raised through the host error channel with its original message and stack.
 */
async function acquire<T>(work: () => Promise<T>, release: (value: T) => Promise<void>, scope: Scope): Promise<T> {
  let cancelled = false;
  let acquired: { value: T } | undefined;
  let releasing: Promise<void> | undefined;
  const releaseAcquired = (): Promise<void> => {
    releasing ??= Promise.resolve().then(() => release(acquired!.value));
    return releasing;
  };
  try {
    return await bounded(() => work().then(value => {
      acquired = { value };
      if (cancelled) {
        void releaseAcquired().catch(error => {
          // The browser's persistent error handler observes this; a Node host
          // fails loudly instead of losing an asynchronous cleanup failure.
          queueMicrotask(() => { throw error; });
        });
      }
      return value;
    }), scope);
  } catch (error) {
    cancelled = true;
    if (acquired) return cleanupAfterFailure(error, [releaseAcquired]);
    throw error;
  }
}

function command<T>(backend: Backend, op: string, args: object): T {
  return readCommandResult<T>(backend.command(JSON.stringify({ op, args })));
}

/** Operations on a session serialize both backend state and positioned reads. */
export class DiscSession {
  private tail: Promise<void> = Promise.resolve();
  private disposed = false;
  private handle: number | undefined;
  private chunkBytes = 0;
  private fillBytes = 0;
  private current: DiscStatus | undefined;

  private constructor(private readonly backend: Backend, private readonly source: Source, private readonly defaults: DiscOptions) {}

  get id(): number {
    if (this.handle === undefined) throw new Error('Disc session has not opened');
    return this.handle;
  }

  get status(): DiscStatus {
    if (!this.current) throw new Error('Disc session has no status');
    return this.current;
  }

  static async open(backend: Backend, source: Source, options: DiscOptions = {}): Promise<DiscSession> {
    const session = new DiscSession(backend, source, options);
    try {
      const scope = operationScope({}, options);
      check(scope);
      if (source.kind === 'pull') {
        if (!backend.disc_fill) throw new Error('This backend has no binary disc_fill transport');
        safeInteger(source.reader.bytes, 'Disc file length');
      }
      const args = source.kind === 'pull'
        ? { kind: 'pull', name: source.reader.name, bytes: source.reader.bytes }
        : { kind: 'native', path: source.path };
      const status = command<DiscStatus>(backend, 'disc.open', args);
      safeInteger(status.session, 'Disc session');
      session.handle = status.session;
      session.chunkBytes = status.chunk_bytes;
      safeInteger(session.chunkBytes, 'Disc fetch chunk size');
      safeInteger(status.chunk_budget, 'Disc fetch budget');
      if (session.chunkBytes === 0 || status.chunk_budget < session.chunkBytes || session.chunkBytes > MAX_FILL_BYTES) throw new Error('Invalid disc fetch chunk size/budget');
      session.fillBytes = Math.floor(Math.min(MAX_FILL_BYTES, status.chunk_budget) / session.chunkBytes) * session.chunkBytes;
      session.current = await session.ready<DiscStatus>('disc.poll', scope, status);
      return session;
    } catch (error) {
      return session.fail(error);
    }
  }

  private enqueue<T>(work: (scope: Scope) => Promise<T>, options: DiscOptions): Promise<T> {
    let scope: Scope | undefined;
    let setupError: unknown;
    try { scope = operationScope(this.defaults, options); }
    catch (error) { setupError = error; }
    const result = this.tail.then(async () => {
      if (this.disposed) throw new Error('Disc session is closed');
      try {
        if (!scope) throw setupError;
        check(scope);
        return await work(scope);
      } catch (error) { return this.fail(error); }
    });
    this.tail = result.then(() => undefined, () => undefined);
    return result;
  }

  private async fill(ranges: [number, number][], scope: Scope): Promise<void> {
    if (!Array.isArray(ranges)) throw new Error('Invalid missing disc ranges');
    if (this.source.kind !== 'pull') {
      if (ranges.length) throw new Error('Native disc session unexpectedly requested byte ranges');
      return;
    }
    const reader = this.source.reader;
    for (const request of ranges) {
      if (!Array.isArray(request) || request.length !== 2) throw new Error('Invalid missing disc range');
      const [start, end] = request;
      range(start, end, reader.bytes);
      if (start === end || start % this.chunkBytes !== 0 || (end !== reader.bytes && end % this.chunkBytes !== 0)) throw new Error('Missing disc range is empty or not chunk aligned');
      for (let offset = start; offset < end;) {
        check(scope);
        const next = Math.min(end, offset + this.fillBytes);
        const data = await bounded(() => reader.read(offset, next), scope);
        check(scope);
        if (!(data instanceof Uint8Array) || data.length !== next - offset) throw new Error(`Short disc read at ${offset}: expected ${next - offset} bytes`);
        const filled = readCommandResult<{ session: number; filled: number }>(this.backend.disc_fill!(this.id, offset, data));
        if (filled.session !== this.id || filled.filled !== data.length) throw new Error('Invalid disc_fill acknowledgement');
        offset = next;
      }
    }
  }

  private async ready<T extends DiscProgress>(op: string, scope: Scope, initial?: T, args: object = { session: this.id }): Promise<T> {
    let reply = initial;
    let previousPending: string | undefined;
    for (;;) {
      check(scope);
      reply ??= command<T>(this.backend, op, args);
      if (reply.session !== this.id || (reply.state !== 'pending' && reply.state !== 'ready')) throw new Error(`Invalid ${op} session/state response`);
      scope.onProgress?.(reply);
      if (reply.state === 'ready') return reply;
      let missing = (reply as unknown as Pending).missing;
      if (!Array.isArray(missing)) throw new Error(`Invalid ${op} missing ranges`);
      if (missing.length === 0) {
        const drained = command<{ session: number; missing: [number, number][] }>(this.backend, 'disc.missing', { session: this.id });
        if (drained.session !== this.id) throw new Error('Invalid disc.missing session response');
        missing = drained.missing;
      }
      if (!Array.isArray(missing) || missing.length === 0) throw new Error(`${op} is pending without any missing ranges`);
      const pending = JSON.stringify([reply.phase, reply.completed, reply.total, missing]);
      if (pending === previousPending) throw new Error(`${op} made no progress after filling its missing ranges`);
      previousPending = pending;
      await this.fill(missing, scope);
      await bounded(() => new Promise<void>(resolve => setTimeout(resolve, 0)), scope);
      reply = undefined;
    }
  }

  poll(options: DiscOptions = {}): Promise<DiscStatus> {
    return this.enqueue(async scope => {
      this.current = await this.ready<DiscStatus>('disc.poll', scope);
      return this.current;
    }, options);
  }

  maps(options: DiscOptions = {}): Promise<DiscMap[]> {
    return this.enqueue(async scope => {
      const result = await this.ready<DiscProgress & { maps: DiscMap[] }>('disc.maps', scope);
      if (!Array.isArray(result.maps)) throw new Error('Invalid disc.maps result');
      return result.maps;
    }, options);
  }

  mapEvents(map: number, options: DiscOptions = {}): Promise<MapEvents> {
    return this.enqueue(async scope => {
      safeInteger(map, 'Map id');
      const result = await this.ready<MapEvents>('field.map_events', scope, undefined, { session: this.id, map });
      if (result.map !== map || typeof result.placeholder !== 'boolean' || (result.placeholder ? result.component !== null : !result.component)) throw new Error('Invalid field.map_events result');
      return result;
    }, options);
  }

  mapBundle(map: number, options: DiscOptions = {}): Promise<MapBundle> {
    return this.enqueue(async scope => {
      safeInteger(map, 'Map id');
      const result = await this.ready<MapBundle>('field.map_bundle', scope, undefined, { session: this.id, map });
      if (result.map !== map || !Number.isSafeInteger(result.slot) || !Array.isArray(result.bytes)) throw new Error('Invalid field.map_bundle result');
      return result;
    }, options);
  }

  /** Drains and fills ranges queued by other reads on this session. */
  missing(options: DiscOptions = {}): Promise<void> {
    return this.enqueue(async scope => {
      const result = command<{ session: number; missing: [number, number][] }>(this.backend, 'disc.missing', { session: this.id });
      if (result.session !== this.id) throw new Error('Invalid disc.missing session response');
      await this.fill(result.missing, scope);
    }, options);
  }

  private async dispose(): Promise<unknown[]> {
    if (this.disposed) return [];
    this.disposed = true;
    const errors: unknown[] = [];
    if (this.handle !== undefined) {
      try {
        const result = command<{ session: number; closed: boolean }>(this.backend, 'disc.close', { session: this.id });
        if (result.session !== this.id || result.closed !== true) throw new Error('Invalid disc.close acknowledgement');
      }
      catch (error) { errors.push(error); }
    }
    if (this.source.kind === 'pull') {
      try { await this.source.reader.close(); }
      catch (error) { errors.push(error); }
    }
    return errors;
  }

  private async fail(error: unknown): Promise<never> {
    const cleanup = await this.dispose();
    if (cleanup.length) throw new AggregateError([error, ...cleanup], error instanceof Error ? error.message : String(error), { cause: error });
    throw error;
  }

  /** Idempotent; closes even if the operation signal has already aborted. */
  close(): Promise<void> {
    const result = this.tail.then(async () => {
      const errors = await this.dispose();
      if (errors.length) throw new AggregateError(errors, 'Closing the disc session failed');
    });
    this.tail = result.then(() => undefined, () => undefined);
    return result;
  }
}

/** Takes ownership of the reader, including closing it if opening fails. */
export function openDiscSession(backend: Backend, reader: DiscByteReader, options: DiscOptions = {}): Promise<DiscSession> {
  return DiscSession.open(backend, { kind: 'pull', reader }, options);
}

/** Native Rust callers can open BIN/CUE/CHD paths without range transport. */
export function openNativeDiscSession(backend: Backend, path: string, options: DiscOptions = {}): Promise<DiscSession> {
  return DiscSession.open(backend, { kind: 'native', path }, options);
}
