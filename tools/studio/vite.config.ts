import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { studioTlsDirectory } from './scripts/tls-directory.js';

export default defineConfig(({ command }) => ({
  plugins: [react()],
  base: './',
  server: {
    host: '0.0.0.0', port: 5176, strictPort: true,
    // Keep the private key local. No HSTS header is sent.
    https: command === 'serve' ? {
      cert: readFileSync(resolve(studioTlsDirectory(), 'cert.pem')),
      key: readFileSync(resolve(studioTlsDirectory(), 'key.pem')),
    } : undefined,
  },
}));
