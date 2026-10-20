import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

export default defineConfig(({ command }) => ({
  plugins: [react()],
  base: './',
  server: {
    host: '0.0.0.0', port: 5176, strictPort: true,
    // Keep the private key local. No HSTS header is sent.
    https: command === 'serve' ? {
      cert: readFileSync(resolve(process.env.XEM_STUDIO_TLS_DIR ?? '/home/bart/xenogears_toolkit/.local/serve', 'cert.pem')),
      key: readFileSync(resolve(process.env.XEM_STUDIO_TLS_DIR ?? '/home/bart/xenogears_toolkit/.local/serve', 'key.pem')),
    } : undefined,
  },
}));
