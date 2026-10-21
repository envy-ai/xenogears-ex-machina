import { createServer, type Server } from 'node:http';
import { mkdir, mkdtemp, rename, symlink, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { createDiscMiddleware } from '../../scripts/disc-server.js';

const repositoryRoot = fileURLToPath(new URL('../../../../', import.meta.url));
const evidenceRoot = resolve(repositoryRoot, 'tmp/studio/remote-discs');
const servers: Server[] = [];
const fixtureDirectories: string[] = [];

async function fixture(): Promise<string> {
  await mkdir(evidenceRoot, { recursive: true });
  const path = await mkdtemp(resolve(evidenceRoot, 'server-fixture-'));
  fixtureDirectories.push(path);
  return path;
}

async function listen(middleware: Awaited<ReturnType<typeof createDiscMiddleware>>): Promise<string> {
  const server = createServer((request, response) => middleware(request, response, () => {
    response.statusCode = 404;
    response.end();
  }));
  servers.push(server);
  await new Promise<void>((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      server.off('error', reject);
      resolveListen();
    });
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Test HTTP server did not bind a TCP port');
  return `http://127.0.0.1:${address.port}`;
}

async function json(response: Response): Promise<Record<string, unknown>> {
  return await response.json() as Record<string, unknown>;
}

afterEach(async () => {
  await Promise.all(servers.splice(0).map(server => new Promise<void>(resolveClose => server.close(() => resolveClose()))));
  // Fixture directories are intentionally retained under ignored tmp/studio for evidence.
  expect(fixtureDirectories.every(path => path.startsWith(evidenceRoot))).toBe(true);
});

describe('read-only disc HTTP middleware', () => {
  it('is disabled by default and does not answer non-GET or cross-origin requests', async () => {
    const base = await listen(await createDiscMiddleware());
    const catalogue = await fetch(`${base}/__studio/discs`);
    expect(catalogue.status).toBe(200);
    expect(await json(catalogue)).toEqual({ ok: { enabled: false, files: [] } });

    const post = await fetch(`${base}/__studio/discs`, { method: 'POST' });
    expect(post.status).toBe(405);
    expect(post.headers.get('allow')).toBe('GET');
    expect(await json(post)).toEqual({ error: { kind: 'method_not_allowed', message: expect.any(String) } });

    const foreign = await fetch(`${base}/__studio/discs`, { headers: { Origin: 'https://attacker.invalid' } });
    expect(foreign.status).toBe(403);
    expect(await json(foreign)).toMatchObject({ error: { kind: 'forbidden_origin' } });
    expect(foreign.headers.get('access-control-allow-origin')).toBeNull();
  });

  it('lists only supported flat regular files and serves exact conditional byte ranges', async () => {
    const directory = await fixture();
    const data = Buffer.from([0, 1, 2, 3, 254, 255]);
    await writeFile(resolve(directory, 'disc one.bin'), data);
    await writeFile(resolve(directory, 'extra.CHD'), Buffer.from([7]));
    await writeFile(resolve(directory, '.hidden.bin'), Buffer.from([8]));
    await writeFile(resolve(directory, 'notes.txt'), Buffer.from([9]));
    await mkdir(resolve(directory, 'subdir'));
    await writeFile(resolve(directory, 'subdir', 'nested.bin'), Buffer.from([10]));
    await symlink(resolve(directory, 'disc one.bin'), resolve(directory, 'linked.bin'));

    const base = await listen(await createDiscMiddleware(directory));
    const listingResponse = await fetch(`${base}/__studio/discs`);
    expect(listingResponse.status).toBe(200);
    const listing = await json(listingResponse) as { ok: { enabled: boolean; files: Array<{ id: string; name: string; bytes: number; etag: string }> } };
    expect(listing.ok.enabled).toBe(true);
    expect(listing.ok.files.map(file => [file.name, file.bytes])).toEqual([['disc one.bin', 6], ['extra.CHD', 1]]);
    const listed = listing.ok.files[0]!;
    const range = await fetch(`${base}/__studio/discs/${listed.id}`, {
      headers: { Range: 'bytes=2-4', 'If-Match': listed.etag },
    });
    expect(range.status).toBe(206);
    expect(range.headers.get('content-range')).toBe('bytes 2-4/6');
    expect(range.headers.get('content-length')).toBe('3');
    expect(range.headers.get('etag')).toBe(listed.etag);
    expect([...new Uint8Array(await range.arrayBuffer())]).toEqual([2, 3, 254]);
  });

  it('rejects malformed, encoded traversal, and unsupported IDs without serving files', async () => {
    const directory = await fixture();
    await writeFile(resolve(directory, 'disc.bin'), Buffer.from([1, 2, 3]));
    const base = await listen(await createDiscMiddleware(directory));
    const encodedTraversal = Buffer.from('../outside.bin').toString('base64url');
    for (const id of ['%', '%2e%2e%2foutside.bin', encodedTraversal, Buffer.from('note.txt').toString('base64url')]) {
      const result = await fetch(`${base}/__studio/discs/${id}`, { headers: { Range: 'bytes=0-0', 'If-Match': '"anything"' } });
      expect(result.status).toBe(400);
      expect(await json(result)).toMatchObject({ error: { kind: 'invalid_id' } });
    }
  });

  it('requires If-Match and a single in-bounds range no larger than 8 MiB', async () => {
    const directory = await fixture();
    await writeFile(resolve(directory, 'disc.bin'), Buffer.alloc((8 << 20) + 1, 0x2a));
    const base = await listen(await createDiscMiddleware(directory));
    const listing = await json(await fetch(`${base}/__studio/discs`)) as { ok: { files: Array<{ id: string; etag: string }> } };
    const file = listing.ok.files[0]!;
    const request = (range: string, etag = file.etag) => fetch(`${base}/__studio/discs/${file.id}`, { headers: { Range: range, 'If-Match': etag } });

    const missingEtag = await fetch(`${base}/__studio/discs/${file.id}`, { headers: { Range: 'bytes=0-0' } });
    expect(missingEtag.status).toBe(428);
    const malformed = await request('bytes=0-1,4-5');
    expect(malformed.status).toBe(400);
    const tooLarge = await request('bytes=0-8388608');
    expect(tooLarge.status).toBe(416);
    expect(tooLarge.headers.get('content-range')).toBe('bytes */8388609');
    const outside = await request('bytes=8388609-8388609');
    expect(outside.status).toBe(416);
    const wrongTag = await request('bytes=0-0', '"stale"');
    expect(wrongTag.status).toBe(412);
    expect(await json(wrongTag)).toMatchObject({ error: { kind: 'disc_changed' } });
  });

  it('returns 412 after a listed file changes and 404 after it disappears', async () => {
    const directory = await fixture();
    const path = resolve(directory, 'disc.bin');
    await writeFile(path, Buffer.from([1, 2, 3]));
    const base = await listen(await createDiscMiddleware(directory));
    const listing = await json(await fetch(`${base}/__studio/discs`)) as { ok: { files: Array<{ id: string; etag: string }> } };
    const file = listing.ok.files[0]!;

    await writeFile(path, Buffer.from([1, 2, 3, 4]));
    const changed = await fetch(`${base}/__studio/discs/${file.id}`, { headers: { Range: 'bytes=0-0', 'If-Match': file.etag } });
    expect(changed.status).toBe(412);

    const current = await json(await fetch(`${base}/__studio/discs`)) as { ok: { files: Array<{ id: string; etag: string }> } };
    const currentFile = current.ok.files[0]!;
    await rename(path, `${path}.retired`);
    const missing = await fetch(`${base}/__studio/discs/${currentFile.id}`, { headers: { Range: 'bytes=0-0', 'If-Match': currentFile.etag } });
    expect(missing.status).toBe(404);
    expect(await json(missing)).toMatchObject({ error: { kind: 'disc_not_found' } });
  });

  it('rejects a symlink configured as the root directory', async () => {
    const real = await fixture();
    const parent = await fixture();
    const alias = resolve(parent, 'alias');
    await symlink(real, alias, 'dir');
    await expect(createDiscMiddleware(alias)).rejects.toThrow('without symlink components');
  });
});
