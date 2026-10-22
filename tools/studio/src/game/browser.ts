import { ExactInteger, frameCount, gameReply, record, type GameTransport } from './protocol';

export interface BrowserGameApi {
  version: number;
  cancellableRun: true;
  status(): { running: boolean; session: unknown; page: { sessionGeneration: number } };
  setRunning(running: boolean): void;
  commandJson(json: string): string;
  run(command: { cmd: 'frames'; count: number }, options?: { signal?: AbortSignal }): Promise<unknown>;
}
export interface GameWindow {
  readonly closed: boolean;
  readonly location: { readonly origin: string };
  readonly xem?: BrowserGameApi;
}

/** Direct same-origin window access; no message bridge, service or CORS bypass. */
export async function browserGameTransport(target: GameWindow, origin: string, timeout = 30_000): Promise<GameTransport> {
  const cancellation = new AbortController();
  let terminal: Error | null = null;
  let closed = false;
  const listeners = new Set<(error: Error) => void>();
  const fail = (error: Error) => {
    if (terminal) return;
    terminal = error;
    cancellation.abort(error);
    clearInterval(monitor);
    for (const listener of listeners) listener(error);
  };
  const access = (): BrowserGameApi | undefined => {
    if (closed || terminal) throw terminal ?? new Error('Browser game link is closed');
    if (target.closed) throw new Error('The attached game window was closed');
    if (target.location.origin !== origin) throw new Error('The game window must have the same origin as Studio');
    return target.xem;
  };
  let api: BrowserGameApi | undefined;
  const deadline = performance.now() + timeout;
  try {
    while (!(api = access())) {
      if (performance.now() >= deadline) throw new Error('The game window.xem API did not become ready before its deadline');
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    if (api.version !== 4 || api.cancellableRun !== true || typeof api.commandJson !== 'function' || typeof api.run !== 'function' || typeof api.status !== 'function' || typeof api.setRunning !== 'function') {
      throw new Error('The game window lacks the required window.xem runtime control API');
    }
  } catch (error) { throw new Error('Cannot attach to browser game', { cause: error }); }
  const connected = api;
  const generation = connected.status().page?.sessionGeneration;
  if (!Number.isSafeInteger(generation) || generation < 1) throw new Error('Browser game has no acknowledged booted session generation');
  const checked = () => {
    const current = access();
    if (current !== connected) throw new Error('The game window reloaded or replaced its runtime API; reconnect');
    const status = connected.status();
    if (status.page?.sessionGeneration !== generation) throw new Error('The browser game booted another session; reconnect');
    if (!status.session) throw new Error('The browser game session ended; reconnect');
    return connected;
  };
  const monitor = setInterval(() => { try { checked(); } catch (error) { fail(error instanceof Error ? error : new Error(String(error))); } }, 250);
  const invoke = async <T>(work: (api: BrowserGameApi) => T | Promise<T>): Promise<T> => {
    try {
      let rejectLoss: (error: Error) => void = () => {};
      const lost = new Promise<never>((_resolve, reject) => { rejectLoss = reject; });
      listeners.add(rejectLoss);
      const timer = setTimeout(() => fail(new Error('Browser game operation timed out after 120 seconds')), 120_000);
      try {
        const result = await Promise.race([Promise.resolve().then(() => work(checked())), lost]);
        checked();
        return result;
      } finally { clearTimeout(timer); listeners.delete(rejectLoss); }
    } catch (error) {
      // Runtime errors remain operation errors; inaccessible/replaced windows end the link.
      try { checked(); } catch (loss) { fail(loss instanceof Error ? loss : new Error(String(loss))); }
      throw error;
    }
  };
  return {
    kind: 'browser',
    request: json => invoke(api => api.commandJson(json)),
    frames: count => invoke(async api => {
      frameCount(count);
      if (api.status().running) throw new Error('Pause automatic browser pacing before exact frame work');
      const frame = (text: string) => {
        const value = record(gameReply(text), 'Runtime frame status').frames;
        if (value instanceof ExactInteger) return BigInt(value.decimal);
        if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) throw new Error('Runtime did not report an exact frame counter');
        return BigInt(value);
      };
      const before = frame(api.commandJson('{"cmd":"status"}'));
      const reply = JSON.stringify(await api.run({ cmd: 'frames', count }, { signal: cancellation.signal }));
      gameReply(reply);
      if ((JSON.parse(reply) as { blocked?: boolean }).blocked) throw new Error('Browser runtime returned an incomplete disc-blocked frame command');
      // run() completes disc fetches but its object reply has ordinary JS numbers.
      // Read the final paused status again over the exact JSON boundary.
      const status = api.commandJson('{"cmd":"status"}');
      if (api.status().running || frame(status) !== before + BigInt(count)) throw new Error('Automatic browser pacing or another frame command interleaved with exact stepping');
      return status;
    }),
    running: value => invoke(api => {
      if (value !== undefined) api.setRunning(value);
      const status = api.status();
      if (typeof status.running !== 'boolean') throw new Error('Browser runtime did not acknowledge its running state');
      if (!status.session) throw new Error('The browser game has no booted session');
      return status.running;
    }),
    onLoss(listener) { listeners.add(listener); if (terminal) listener(terminal); return () => { listeners.delete(listener); }; },
    async close() { closed = true; clearInterval(monitor); fail(new Error('Browser game link detached')); },
  };
}
