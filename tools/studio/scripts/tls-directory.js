import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = fileURLToPath(new URL('../../../', import.meta.url));

/** Select the same explicitly configured TLS directory for Vite and NW.js. */
export function studioTlsDirectory() {
  const directory = process.env.XEM_STUDIO_TLS_DIR;
  if (!directory?.trim()) {
    throw new Error('Set XEM_STUDIO_TLS_DIR to a directory containing cert.pem and key.pem for HTTPS development.');
  }
  return resolve(repositoryRoot, directory);
}
