// Explicit same-origin static runtime build mount; no live control endpoint.
import { constants } from 'node:fs';
import { lstat, open, realpath, stat } from 'node:fs/promises';
import { extname, isAbsolute, relative, resolve, sep } from 'node:path';

const files = new Set(['index.html', 'style.css', 'main.js', 'game.js', 'disc.js', 'storage.js', 'audio.js', 'audio-worklet.js', 'cards.js', 'xem.js', 'game/game.wasm', 'game/schema.json', 'game/stubs.txt', 'pkg/xem_web.js', 'pkg/xem_web_bg.wasm']);
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.json': 'application/json', '.txt': 'text/plain; charset=utf-8', '.wasm': 'application/wasm' };
export async function createGameMiddleware(directory = process.env.XEM_STUDIO_GAME_DIR) {
  const selected = directory?.trim();
  const root = selected ? await realpath(resolve(import.meta.dirname, '../../..', selected)) : null;
  const rootIdentity = root ? await stat(root, { bigint: true }) : null;
  if (rootIdentity && !rootIdentity.isDirectory()) throw new Error('XEM_STUDIO_GAME_DIR must name a built runtime directory');
  const same = (a, b) => a.dev === b.dev && a.ino === b.ino;
  const checkRoot = async () => {
    if (await realpath(root) !== root || !same(await stat(root, { bigint: true }), rootIdentity)) throw new Error('Runtime build directory changed; restart the server');
  };
  return async (request, response, next) => {
    let url;
    try { url = new URL(request.url ?? '/', 'http://studio.invalid'); }
    catch (error) { response.writeHead(400).end(`Invalid runtime request URL: ${error.message}`); return; }
    if (!url.pathname.startsWith('/runtime/')) { next(); return; }
    response.setHeader('Cache-Control', 'no-store');
    response.setHeader('X-Content-Type-Options', 'nosniff');
    response.setHeader('Cross-Origin-Resource-Policy', 'same-origin');
    if (!root) { response.writeHead(503).end('No runtime build configured: set XEM_STUDIO_GAME_DIR.'); return; }
    if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405, { Allow: 'GET, HEAD' }).end(); return; }
    let handle;
    try {
      const name = url.pathname.slice('/runtime/'.length) || 'index.html';
      if (!files.has(name)) { response.writeHead(404).end('Not a runtime build file.'); return; }
      await checkRoot();
      const path = resolve(root, name);
      const actual = await realpath(path);
      const suffix = relative(root, actual);
      if (isAbsolute(suffix) || suffix === '..' || suffix.startsWith(`..${sep}`) || actual !== path) throw new Error('Runtime build file must be a contained regular file without symlinks');
      const before = await lstat(path, { bigint: true });
      if (!before.isFile() || before.isSymbolicLink()) throw new Error('Runtime build path is not a regular file');
      handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
      const opened = await handle.stat({ bigint: true });
      await checkRoot();
      if (await realpath(path) !== actual || !same(before, opened) || !same(await lstat(path, { bigint: true }), opened) || !opened.isFile()) throw new Error('Runtime build file changed during open');
      const info = await handle.stat();
      response.writeHead(200, { 'Content-Type': types[extname(path)], 'Content-Length': info.size });
      if (request.method === 'HEAD') { await handle.close(); handle = null; response.end(); return; }
      const stream = handle.createReadStream(); handle = null;
      stream.on('error', error => response.destroy(error));
      response.on('close', () => stream.destroy());
      stream.pipe(response);
    } catch (error) {
      if (handle) await handle.close();
      if (response.headersSent) response.destroy(error);
      else response.writeHead(error.code === 'ENOENT' ? 404 : 500).end(`Runtime build: ${error.message}`);
    }
  };
}
export function studioGameServer(directory = process.env.XEM_STUDIO_GAME_DIR) {
  return {
    name: 'xem-studio-runtime-build',
    async configureServer(server) { server.middlewares.use(await createGameMiddleware(directory)); },
    async configurePreviewServer(server) { server.middlewares.use(await createGameMiddleware(directory)); },
  };
}
