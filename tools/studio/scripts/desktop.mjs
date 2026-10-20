import { spawn } from 'node:child_process';
import { X509Certificate } from 'node:crypto';
import { findpath } from 'nw';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { studioTlsDirectory } from './tls-directory.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dev = process.argv.includes('--dev');
if (!dev && !existsSync(resolve(root, 'dist/index.html'))) throw new Error('Run npm run build before npm run desktop, or use npm run desktop -- --dev.');
const packageManifest = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8'));
let launch = root;
if (dev) {
  launch = resolve(root, '../../tmp/studio/nw-launch');
  mkdirSync(launch, { recursive: true });
  const tlsDirectory = studioTlsDirectory();
  // Serialize only the public certificate into the temporary dev manifest.
  const certificate = new X509Certificate(readFileSync(resolve(tlsDirectory, 'cert.pem')));
  const manifest = {
    name: packageManifest.name,
    version: packageManifest.version,
    main: 'https://localhost:5176/',
    'node-remote': 'https://localhost:5176/*',
    additional_trust_anchors: [certificate.toString()],
    window: packageManifest.window,
  };
  writeFileSync(resolve(launch, 'package.json'), JSON.stringify(manifest, null, 2));
}
const binary = await findpath('nwjs', { version: '0.113.0', flavor: 'normal' });
if (!existsSync(binary)) throw new Error('NW.js binary is missing. Reinstall the pinned nw package.');
const args = [launch, ...process.argv.slice(2).filter(arg => arg !== '--dev')];
const child = spawn(binary, args, { stdio: 'inherit' });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
child.on('error', error => { throw error; });
child.on('exit', (code, signal) => process.exit(code ?? (signal === 'SIGINT' ? 130 : signal === 'SIGTERM' ? 143 : 1)));
