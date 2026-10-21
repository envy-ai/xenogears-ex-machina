import { createHash } from 'node:crypto';
import { constants } from 'node:fs';
import { access, lstat, open, readdir, realpath } from 'node:fs/promises';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('../../../', import.meta.url));
const endpoint = '/__studio/discs';
const maxRangeBytes = 8 << 20;
const supportedExtensions = new Set(['.chd', '.bin', '.cue']);
const utf8 = new TextDecoder('utf-8', { fatal: true });

class DiscRequestError extends Error {
  constructor(status, kind, message, options) {
    super(message, options);
    this.status = status;
    this.kind = kind;
  }
}

function validLeaf(name) {
  return name.length > 0 && !name.startsWith('.') && !/[\x00-\x1f\x7f/\\:%]/.test(name)
    && supportedExtensions.has(extname(name).toLowerCase());
}

function fileId(name) { return Buffer.from(name, 'utf8').toString('base64url'); }

function decodeId(id) {
  if (!/^[A-Za-z0-9_-]+$/.test(id)) {
    throw new DiscRequestError(400, 'invalid_id', 'Expected one canonical base64url disc filename.');
  }
  let name;
  try {
    const bytes = Buffer.from(id, 'base64url');
    name = utf8.decode(bytes);
    if (bytes.toString('base64url') !== id || !validLeaf(name)) throw new Error('Invalid filename');
  } catch (cause) {
    throw new DiscRequestError(400, 'invalid_id', 'Disc id must identify a supported nonhidden single-leaf filename.', { cause });
  }
  return name;
}

function identity(stat) {
  const value = [stat.dev, stat.ino, stat.size, stat.mtimeNs, stat.ctimeNs].join(':');
  return `"${createHash('sha256').update(value).digest('hex')}"`;
}

function safeSize(stat) {
  if (stat.size < 0n || stat.size > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new DiscRequestError(500, 'invalid_file_size', 'Disc size cannot be represented as a safe byte offset.');
  }
  return Number(stat.size);
}

function sameFile(first, second) { return first.dev === second.dev && first.ino === second.ino; }

function checkOrigin(request) {
  const site = request.headers['sec-fetch-site'];
  if (site !== undefined && site !== 'same-origin' && site !== 'none') {
    throw new DiscRequestError(403, 'forbidden_origin', 'Disc access requires a same-origin request.');
  }
  const origin = request.headers.origin;
  if (origin === undefined) return;
  try {
    const protocol = request.socket.encrypted ? 'https:' : 'http:';
    const host = request.headers.host;
    if (typeof host !== 'string' || typeof origin !== 'string') throw new Error('Invalid origin');
    const expected = new URL(`${protocol}//${host}`);
    if (expected.username || expected.password || expected.pathname !== '/' || expected.search || expected.hash
      || origin !== expected.origin) throw new Error('Foreign origin');
  } catch (cause) {
    throw new DiscRequestError(403, 'forbidden_origin', 'Disc access requires a same-origin request.', { cause });
  }
}

function commonHeaders(response) {
  response.setHeader('Cache-Control', 'no-store');
  response.setHeader('X-Content-Type-Options', 'nosniff');
  response.setHeader('Cross-Origin-Resource-Policy', 'same-origin');
}

function json(response, status, value) {
  if (response.destroyed) return;
  const bytes = Buffer.from(JSON.stringify(value));
  response.statusCode = status;
  response.setHeader('Content-Type', 'application/json; charset=utf-8');
  response.setHeader('Content-Length', bytes.length);
  response.end(bytes);
}

function requestedRange(header, size) {
  if (header === undefined) throw new DiscRequestError(400, 'range_required', 'An explicit byte Range is required.');
  if (typeof header !== 'string' || !/^bytes=\d+-\d+$/.test(header)) {
    throw new DiscRequestError(400, 'invalid_range', 'Expected one closed Range: bytes=start-end.');
  }
  const [start, end] = header.slice(6).split('-').map(Number);
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start > end || end >= size) {
    throw new DiscRequestError(416, 'range_not_satisfiable', 'The requested byte range is outside the listed disc.');
  }
  const length = end - start + 1;
  if (length > maxRangeBytes) throw new DiscRequestError(416, 'range_too_large', 'Disc ranges may contain at most 8 MiB.');
  return { start, end, length };
}

/** Explicit directories are resolved from the repository, like the TLS path. */
export async function createDiscMiddleware(directory = process.env.XEM_STUDIO_DISC_DIR) {
  let root = null;
  let rootIdentity = null;
  if (directory?.trim()) {
    if (!Number.isInteger(constants.O_NOFOLLOW) || !Number.isInteger(constants.O_NONBLOCK)) {
      throw new Error('The read-only disc server requires O_NOFOLLOW and O_NONBLOCK support.');
    }
    root = resolve(repositoryRoot, directory);
    rootIdentity = await lstat(root, { bigint: true });
    if (!rootIdentity.isDirectory() || rootIdentity.isSymbolicLink() || await realpath(root) !== root) {
      throw new Error('XEM_STUDIO_DISC_DIR must name an existing directory without symlink components.');
    }
    await access(root, constants.R_OK | constants.X_OK);
  }

  async function assertRoot() {
    const current = await lstat(root, { bigint: true });
    if (!current.isDirectory() || current.isSymbolicLink() || !sameFile(current, rootIdentity)
      || await realpath(root) !== root) {
      throw new DiscRequestError(403, 'invalid_directory', 'The configured disc directory changed identity or contains a symlink.');
    }
  }

  async function withFile(name, operation) {
    await assertRoot();
    const path = join(root, name);
    if (dirname(path) !== root) throw new DiscRequestError(400, 'invalid_id', 'Disc filename is outside the configured root.');
    const before = await lstat(path, { bigint: true });
    if (!before.isFile() || before.isSymbolicLink() || await realpath(path) !== path) {
      throw new DiscRequestError(403, 'invalid_file', 'Disc files must be regular files without symlinks.');
    }
    let handle;
    let failure;
    try {
      handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
      const stat = await handle.stat({ bigint: true });
      const current = await lstat(path, { bigint: true });
      await assertRoot();
      if (!stat.isFile() || !current.isFile() || current.isSymbolicLink()
        || !sameFile(stat, before) || !sameFile(stat, current) || await realpath(path) !== path) {
        throw new DiscRequestError(403, 'invalid_file', 'Disc file identity changed while it was opened.');
      }
      const result = await operation(handle, stat);
      const after = await handle.stat({ bigint: true });
      const currentAfter = await lstat(path, { bigint: true });
      await assertRoot();
      if (identity(after) !== identity(stat) || !currentAfter.isFile() || currentAfter.isSymbolicLink()
        || !sameFile(after, currentAfter) || await realpath(path) !== path) {
        throw new DiscRequestError(412, 'disc_changed', 'Disc changed during the read; list the server discs again.');
      }
      return result;
    } catch (error) {
      failure = error;
      throw error;
    } finally {
      if (handle) {
        try { await handle.close(); }
        catch (error) {
          if (failure) {
            const message = failure instanceof Error ? failure.message : String(failure);
            const closing = error instanceof Error ? error.message : String(error);
            throw new AggregateError([failure, error], `${message} (closing the read-only disc handle also failed: ${closing})`, { cause: failure });
          }
          throw error;
        }
      }
    }
  }

  async function handleRequest(request, response, suffix) {
    checkOrigin(request);
    if (request.method !== 'GET') {
      response.setHeader('Allow', 'GET');
      throw new DiscRequestError(405, 'method_not_allowed', 'The disc server accepts GET requests only.');
    }
    if (!root) {
      if (suffix === '') return json(response, 200, { ok: { enabled: false, files: [] } });
      throw new DiscRequestError(503, 'not_configured', 'XEM_STUDIO_DISC_DIR is not configured.');
    }
    if (suffix === '') {
      await assertRoot();
      const entries = await readdir(root, { withFileTypes: true });
      const files = [];
      for (const entry of entries.sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0)) {
        if (!entry.isFile() || entry.isSymbolicLink() || !validLeaf(entry.name)) continue;
        files.push(await withFile(entry.name, async (_handle, stat) => ({
          id: fileId(entry.name), name: entry.name, bytes: safeSize(stat), etag: identity(stat),
        })));
      }
      await assertRoot();
      return json(response, 200, { ok: { enabled: true, files } });
    }
    const name = decodeId(suffix.slice(1));
    const ifMatch = request.headers['if-match'];
    if (ifMatch === undefined) throw new DiscRequestError(428, 'if_match_required', 'Send the exact ETag from the server disc listing in If-Match.');
    const result = await withFile(name, async (handle, stat) => {
      const etag = identity(stat);
      if (ifMatch !== etag) throw new DiscRequestError(412, 'disc_changed', 'Disc identity no longer matches the listing; list the server discs again.');
      const size = safeSize(stat);
      let range;
      try { range = requestedRange(request.headers.range, size); }
      catch (error) {
        if (error instanceof DiscRequestError && error.status === 416) response.setHeader('Content-Range', `bytes */${size}`);
        throw error;
      }
      const bytes = Buffer.allocUnsafe(range.length);
      let offset = 0;
      while (offset < bytes.length) {
        const { bytesRead } = await handle.read(bytes, offset, bytes.length - offset, range.start + offset);
        if (!bytesRead) throw new DiscRequestError(412, 'disc_changed', 'Disc was truncated during the range read; list the server discs again.');
        offset += bytesRead;
      }
      return { bytes, etag, size, range };
    });
    if (response.destroyed) return;
    response.statusCode = 206;
    response.setHeader('Content-Type', 'application/octet-stream');
    response.setHeader('Accept-Ranges', 'bytes');
    response.setHeader('Content-Range', `bytes ${result.range.start}-${result.range.end}/${result.size}`);
    response.setHeader('Content-Length', result.bytes.length);
    response.setHeader('ETag', result.etag);
    response.end(result.bytes);
  }

  return function discMiddleware(request, response, next) {
    const path = (request.url ?? '').split('?')[0];
    if (path !== endpoint && !path.startsWith(`${endpoint}/`)) return next();
    commonHeaders(response);
    void handleRequest(request, response, path.slice(endpoint.length)).catch(error => {
      const missing = error?.code === 'ENOENT';
      const status = error instanceof DiscRequestError ? error.status : missing ? 404 : 500;
      const kind = error instanceof DiscRequestError ? error.kind : missing ? 'disc_not_found' : 'io_error';
      if (status >= 500) console.error(error);
      json(response, status, { error: { kind, message: error instanceof Error ? error.message : String(error) } });
    });
  };
}

/** Install the same opt-in reader on Vite development and preview servers. */
export function studioDiscServer(directory = process.env.XEM_STUDIO_DISC_DIR) {
  return {
    name: 'studio-read-only-disc-server',
    async configureServer(server) { server.middlewares.use(await createDiscMiddleware(directory)); },
    async configurePreviewServer(server) { server.middlewares.use(await createDiscMiddleware(directory)); },
  };
}
