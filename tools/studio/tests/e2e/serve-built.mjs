import { preview } from 'vite';
import { studioDiscServer } from '../../scripts/disc-server.js';
import { studioGameServer } from '../../scripts/game-server.js';

const server = await preview({
  configFile: false,
  plugins: [studioDiscServer(process.env.XEM_STUDIO_DISC_DIR), studioGameServer(process.env.XEM_STUDIO_GAME_DIR)],
  build: { outDir: process.env.XEM_STUDIO_TEST_DIST ?? 'dist' },
  preview: { host: '127.0.0.1', port: 5176, strictPort: true },
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => server.httpServer.close());
}
