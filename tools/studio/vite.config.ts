import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { studioTlsDirectory } from './scripts/tls-directory.js';

export default defineConfig(({ command }) => {
  const fixturePackage = process.env.XEM_STUDIO_WASM_PACKAGE;
  if (fixturePackage && process.env.VITE_XEM_STUDIO_TEST !== '1') {
    throw new Error('An isolated WASM package requires the explicit studio test build');
  }
  const packageDirectory = fixturePackage ? resolve(fixturePackage) : resolve(import.meta.dirname, 'src/backend/pkg');
  return ({
  resolve: { alias: [
    { find: /^\.\/pkg\/xem_studio$/, replacement: resolve(packageDirectory, 'xem_studio.js') },
    { find: /^\.\/pkg\/xem_studio_bg\.wasm(\?.*)?$/, replacement: `${resolve(packageDirectory, 'xem_studio_bg.wasm')}$1` },
  ] },
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
});
});
