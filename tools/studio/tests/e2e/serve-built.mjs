import { preview } from 'vite';

const server = await preview({
  configFile: false,
  preview: { host: '127.0.0.1', port: 5176, strictPort: true },
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => server.httpServer.close());
}
