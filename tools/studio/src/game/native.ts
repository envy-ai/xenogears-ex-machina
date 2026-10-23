import type { ChildProcessWithoutNullStreams, spawn as Spawn } from 'node:child_process';
import { frameCount, gameJson, gameReply, record, type GameTransport } from './protocol';

export interface NativeLaunch {
  executable: string;
  disc: string;
  settingsDirectory: string;
  cardsDirectory: string;
  packages?: string;
}
export function launchNativeGame(options: NativeLaunch, spawn: typeof Spawn, environment: NodeJS.ProcessEnv): GameTransport {
  for (const [name, path] of Object.entries(options)) {
    const separator = String.fromCharCode(92);
    const absolute = path.startsWith('/') || (/^[A-Za-z]:/.test(path) && ['/', separator].includes(path[2])) || path.startsWith(separator.repeat(2));
    if (!absolute || path.includes(String.fromCharCode(0))) throw new Error(`${name} must be an explicit absolute local path`);
  }
  const packageArguments = options.packages ? ['--packages', options.packages] : [];
  const child = spawn(options.executable, ['--disc', options.disc, '--cards', options.cardsDirectory, '--control', '--control-paused', ...packageArguments], {
    shell: false, stdio: ['pipe', 'pipe', 'pipe'], env: { ...environment, XEM_DATA_DIR: options.settingsDirectory },
  });
  return nativeGameTransport(child);
}

/** Complete-line framing over only the child created for this connection. */
export function nativeGameTransport(child: ChildProcessWithoutNullStreams): GameTransport {
  let buffer = '';
  let terminal: Error | null = null;
  let closed = false;
  let pending: { resolve(text: string): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout> } | null = null;
  const listeners = new Set<(error: Error) => void>();
  let stderr = '';
  const fail = (error: Error) => {
    if (terminal) return;
    terminal = error;
    if (pending) { clearTimeout(pending.timer); pending.reject(error); pending = null; }
    for (const listener of listeners) listener(error);
  };
  child.stdout.setEncoding('utf8');
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', (text: string) => { stderr = (stderr + text).slice(-32_768); });
  child.stdout.on('data', (text: string) => {
    if (terminal) return;
    buffer += text;
    if (buffer.length > 8 * 1024 * 1024) { fail(new Error('Native control reply exceeds 8 MiB')); return; }
    for (;;) {
      const end = buffer.indexOf('\n');
      if (end < 0) break;
      const line = buffer.slice(0, end); buffer = buffer.slice(end + 1);
      if (!pending) { fail(new Error(`Native control sent an unsolicited reply${stderr ? `\nNative stderr:\n${stderr}` : ''}`)); return; }
      try { JSON.parse(line); }
      catch (cause) { fail(new Error('Malformed native control JSON reply', { cause })); return; }
      const completion = pending; pending = null; clearTimeout(completion.timer); completion.resolve(line);
    }
  });
  child.stdout.on('end', () => fail(new Error(`Native control stdout ended${buffer ? ' with an incomplete reply' : ''}${stderr ? `\nNative stderr:\n${stderr}` : ''}`)));
  child.on('error', fail);
  child.stdin.on('error', fail);
  child.stdout.on('error', fail);
  child.stderr.on('error', fail);
  child.on('exit', (code, signal) => fail(new Error(`Native game exited (code ${code}, signal ${signal})${stderr ? `\nNative stderr:\n${stderr}` : ''}`)));
  const request = (json: string): Promise<string> => new Promise((resolve, reject) => {
    if (closed || terminal) { reject(terminal ?? new Error('Native game link is closed')); return; }
    if (pending) { reject(new Error('A native control request is already pending')); return; }
    if (json.includes('\n') || json.includes('\r')) { reject(new Error('A native request must be one JSON line')); return; }
    try { JSON.parse(json); } catch (error) { reject(error); return; }
    const timer = setTimeout(() => fail(new Error('Native control request timed out after 120 seconds')), 120_000);
    pending = { resolve, reject, timer };
    child.stdin.write(`${json}\n`, error => { if (error) fail(error); });
  });
  return {
    kind: 'native', request,
    frames: count => request(gameJson({ cmd: 'frames', count: frameCount(count) })),
    async running(value) {
      const reply = record(gameReply(await request(gameJson({ cmd: 'host_running', ...(value === undefined ? {} : { running: value }) }))), 'Native running acknowledgement');
      if (typeof reply.running !== 'boolean') throw new Error('Native runtime did not acknowledge its running state');
      return reply.running;
    },
    onLoss(listener) { listeners.add(listener); if (terminal) listener(terminal); return () => { listeners.delete(listener); }; },
    async close() {
      if (closed) return;
      closed = true;
      fail(new Error('Native game link closed'));
      // This transport holds the exact child it spawned, never an arbitrary PID.
      child.stdin.end();
      if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
    },
  };
}
