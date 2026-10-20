import assert from 'node:assert/strict';
import { afterEach, describe, it } from 'vitest';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { studioTlsDirectory } from '../../scripts/tls-directory.js';

const originalTlsDirectory = process.env.XEM_STUDIO_TLS_DIR;
const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../../../..');

afterEach(() => {
  if (originalTlsDirectory === undefined) delete process.env.XEM_STUDIO_TLS_DIR;
  else process.env.XEM_STUDIO_TLS_DIR = originalTlsDirectory;
});

describe('studioTlsDirectory', () => {
  it('requires an explicit directory instead of silently choosing a machine-specific fallback', () => {
    delete process.env.XEM_STUDIO_TLS_DIR;
    assert.throws(() => studioTlsDirectory(), /XEM_STUDIO_TLS_DIR/);
  });

  it('rejects an empty or whitespace-only directory', () => {
    for (const value of ['', '  \t']) {
      process.env.XEM_STUDIO_TLS_DIR = value;
      assert.throws(() => studioTlsDirectory(), /XEM_STUDIO_TLS_DIR/);
    }
  });

  it('resolves an explicit relative directory from the repository root', () => {
    process.env.XEM_STUDIO_TLS_DIR = 'tmp/studio/test-certificates';
    assert.equal(studioTlsDirectory(), resolve(repositoryRoot, 'tmp/studio/test-certificates'));
  });

  it('preserves an explicit absolute directory after normalization', () => {
    const directory = resolve('/tmp/xem-studio-test-certificates');
    process.env.XEM_STUDIO_TLS_DIR = directory;
    assert.equal(studioTlsDirectory(), directory);
  });
});
