import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdir, writeFile, symlink } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { createGameMiddleware } from '../../scripts/game-server.js';

const enabled = process.env.XEM_STUDIO_GAME_SERVER_TEST === '1';
const artifactRoot = resolve(process.env.XEM_STUDIO_M4_ARTIFACT_ROOT ?? '');
const repoRoot = resolve(import.meta.dirname, '../../../..');

describe.skipIf(!enabled)('same-origin runtime static mount', () => {
  it('serves only fixed regular build files and refuses missing, unsupported, symlinked and non-GET requests', async () => {
    expect(artifactRoot).toContain(join(repoRoot, 'tmp/studio/milestone-4'));
    const root = join(artifactRoot, `static-root-${process.pid}-${Date.now()}`);
    const outside = join(artifactRoot, `static-outside-${process.pid}-${Date.now()}.js`);
    await mkdir(root, { recursive: false });
    await writeFile(join(root, 'index.html'), '<title>runtime fixture</title>', { flag: 'wx' });
    await writeFile(outside, 'outside runtime fixture', { flag: 'wx' });
    await symlink(outside, join(root, 'main.js'));

    const unconfigured = await createGameMiddleware('');
    const configured = await createGameMiddleware(root);
    let active = configured;
    const server = createServer((request: IncomingMessage, response: ServerResponse) => {
      void active(request, response, () => { response.writeHead(404).end(); });
    });
    await new Promise<void>(resolveListen => server.listen(0, '127.0.0.1', resolveListen));
    const address = server.address();
    if (!address || typeof address === 'string') throw new Error('Static-mount fixture server did not bind a TCP port');
    const origin = `http://127.0.0.1:${address.port}`;
    try {
      active = unconfigured;
      const unavailable = await fetch(`${origin}/runtime/index.html`);
      expect(unavailable.status).toBe(503);
      expect(await unavailable.text()).toContain('set XEM_STUDIO_GAME_DIR');

      active = configured;
      const page = await fetch(`${origin}/runtime/index.html`);
      expect(page.status).toBe(200);
      expect(await page.text()).toBe('<title>runtime fixture</title>');
      expect(page.headers.get('x-content-type-options')).toBe('nosniff');
      expect(page.headers.get('cross-origin-resource-policy')).toBe('same-origin');
      expect(page.headers.get('cache-control')).toBe('no-store');

      const unsupported = await fetch(`${origin}/runtime/../../outside.js`);
      expect([400, 404]).toContain(unsupported.status);
      const missing = await fetch(`${origin}/runtime/game.js`);
      expect(missing.status).toBe(404);
      const symlinked = await fetch(`${origin}/runtime/main.js`);
      expect(symlinked.status).toBe(500);
      expect(await symlinked.text()).toContain('without symlinks');
      const method = await fetch(`${origin}/runtime/index.html`, { method: 'POST' });
      expect(method.status).toBe(405);
      expect(method.headers.get('allow')).toBe('GET, HEAD');
    } finally {
      await new Promise<void>((resolveClose, reject) => server.close(error => error ? reject(error) : resolveClose()));
    }
  });
});
