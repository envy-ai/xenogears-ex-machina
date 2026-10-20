import type { Backend } from './backend/service';
import { nativeHost } from './platform/host';
import { openNodeFolder } from './platform/node-files';

/** Explicit, fixture-only NW.js acceptance entry. No production project is opened. */
export async function runNativeSmoke(backend: Backend): Promise<boolean> {
  if (!nativeHost) return false;
  const host = nativeHost;
  const argument = (name: string) => host.App.argv.find(arg => arg.startsWith(`${name}=`))?.slice(name.length + 1);
  const fixture = argument('--studio-smoke');
  const output = argument('--studio-smoke-output');
  if (!fixture && !output) return false;
  if (!fixture || !output) throw new Error('NW smoke requires both --studio-smoke and --studio-smoke-output');
  const fs = nativeHost.require('node:fs/promises');
  try {
    const adapter = await openNodeFolder(fixture, fs, nativeHost.require('node:path'));
    const entries = await adapter.list();
    const version = JSON.parse(backend.command('{"op":"version"}'));
    const echo = JSON.parse(backend.command(JSON.stringify({ op: 'echo', args: entries })));
    await fs.writeFile(output, JSON.stringify({ version, entries, echo, adapter: adapter.kind, windowControls: document.querySelector('[role="group"][aria-label="Window controls"]') !== null }));
  } catch (error) {
    await fs.writeFile(output, JSON.stringify({ error: error instanceof Error ? { message: error.message, stack: error.stack } : String(error) }));
  } finally { nativeHost.App.quit(); }
  return true;
}
