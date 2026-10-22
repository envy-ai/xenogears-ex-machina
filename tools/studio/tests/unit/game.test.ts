import { describe, expect, it, vi } from 'vitest';
import { browserGameTransport, type BrowserGameApi } from '../../src/game/browser';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { nativeGameTransport } from '../../src/game/native';
import { launchNativeGame } from '../../src/game/native';
import { GameController } from '../../src/game/controller';
import type { GameTransport } from '../../src/game/protocol';
import { frameCount, gameJson, gameReply, parseGameJson, ExactInteger } from '../../src/game/protocol';
import { SchemaModel } from '../../src/game/schema';

function schemaModel() {
  return new SchemaModel({
    identity: { module: 'a'.repeat(64), disc: 'disc-1', packages: ['boot'] },
    schema: {
      module_sha256: 'a'.repeat(64),
      globals: { root: { address: 0x100, type: 0 }, wide: { address: 0x108, type: 4 }, float_global: { address: 0x110, type: 5 }, float_struct: { address: 0x114, type: 6 } },
      records: {},
      types: [
        { kind: 'struct', name: 'Root', members: [{ name: 'value', offset: 0, type: 1 }, { name: 'items', offset: 4, type: 2 }, { name: 'ptr', offset: 8, type: 3 }] },
        { kind: 'base', name: 'u8', size: 1, encoding: 'unsigned' },
        { kind: 'array', name: 'u8[130]', count: 130, of: 1 },
        { kind: 'pointer', name: 'Root*', to: 0 },
        { kind: 'base', name: 'u64', size: 8, encoding: 'unsigned' },
        { kind: 'base', name: 'f32', size: 4, encoding: 'float' },
        { kind: 'struct', name: 'FloatRecord', members: [{ name: 'value', offset: 0, type: 5 }] },
      ],
    },
  });
}

describe('runtime command values', () => {
  it('preserves unsafe integer source tokens and nested JSON while rejecting rounded writes', () => {
    const value = parseGameJson('{"n":18446744073709551615,"items":[-9007199254740994,null]}') as { n: ExactInteger; items: (ExactInteger | null)[] };
    expect(value.n.decimal).toBe('18446744073709551615');
    expect(value.items[0]).toBeInstanceOf(ExactInteger);
    expect(gameJson(value)).toBe('{"n":18446744073709551615,"items":[-9007199254740994,null]}');
    expect(() => gameJson(9007199254740992)).toThrow(/exact decimal integer/);
  });

  it('validates frame counts without coercing or clamping', () => {
    for (const n of [1, 2, 10_000]) expect(frameCount(n)).toBe(n);
    for (const n of [0, -1, 1.5, NaN, Infinity, 10_001, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => frameCount(n)).toThrow(/integer from 1 to 10000/);
    }
  });

  it('distinguishes protocol errors from ordinary successful values', () => {
    expect(gameReply('{"ok":null}')).toEqual({ ok: null });
    expect(() => gameReply('{"error":"invalid path"}')).toThrow(/invalid path/);
    // Game state is data, even when an inspected record happens to have a member named error.
    expect(gameReply('{"error":"ordinary game data","value":1}')).toEqual({ error: 'ordinary game data', value: 1 });
  });
});

describe('schema-driven inspection', () => {
  it('checks module identity and validates type references before exposing a tree', () => {
    expect(() => new SchemaModel({ identity: { module: 'b'.repeat(64), disc: 'disc-1', packages: [] }, schema: { globals: {}, records: {}, types: [] } }, 'a'.repeat(64))).toThrow(/another game module/);
    expect(() => new SchemaModel({ identity: { module: 'a'.repeat(64), disc: 'disc-1', packages: [] }, schema: { module_sha256: 'a'.repeat(64), globals: { bad: { address: 0, type: 9 } }, records: {}, types: [] } })).toThrow(/Invalid schema global/);
    expect(() => new SchemaModel({ identity: { module: 'a'.repeat(64), disc: 'disc-1', packages: [] }, schema: { module_sha256: 'b'.repeat(64), globals: {}, records: {}, types: [] } })).toThrow(/game-module build binding/);
  });

  it('expands arrays in bounded pages and reports unsupported members explicitly', () => {
    const model = schemaModel();
    const root = model.roots('root')[0]!;
    const children = model.children(root);
    expect(children.map(item => item.path)).toEqual(['root.value', 'root.items', 'root.ptr']);
    const firstPage = model.children(children[1]!);
    expect(firstPage).toHaveLength(3);
    expect(firstPage.map(item => item.range)).toEqual([
      { start: 0, end: 64, element: 1 },
      { start: 64, end: 128, element: 1 },
      { start: 128, end: 130, element: 1 },
    ]);
    expect(model.children(firstPage[0]!)).toHaveLength(64);
    const pointerChildren = model.children(children[2]!);
    expect(pointerChildren.map(item => item.path)).toEqual(['root.ptr->value', 'root.ptr->items', 'root.ptr->ptr']);
  });

  it('preserves exact scalar text for schema writes and refuses non-scalars', () => {
    const model = schemaModel();
    const wide = model.roots('wide')[0]!;
    expect(model.value(wide, '18446744073709551615')).toEqual(new ExactInteger('18446744073709551615'));
    expect(() => model.value(wide, '18446744073709551616')).toThrow(/outside/);
    const u8 = model.children(model.roots('root')[0]!)[0]!;
    expect(() => model.value(u8, '-1')).toThrow(/outside/);
    expect(() => model.value(u8, '256')).toThrow(/outside/);
    const array = model.children(model.roots('root')[0]!)[1]!;
    expect(() => model.value(array, '1')).toThrow(/Scalar writes unsupported/);
    expect(() => model.value(wide, '1.5')).toThrow(/exact decimal integer/);
  });

  it('keeps floating-point paths visible while marking their raw scalar values unsupported', () => {
    const model = schemaModel();
    const floatRoot = model.roots('float_global')[0]!;
    const floatMember = model.children(model.roots('float_struct')[0]!).find(node => node.path === 'float_struct.value')!;

    for (const node of [floatRoot, floatMember]) {
      expect(node.path).toMatch(/float/);
      expect(node.type).toBe(5);
      expect(node.unsupported).toMatch(/f32.*inspection\/write unsupported by runtime/);
      expect(model.scalar(node)).toBe(false);
      expect(model.children(node)).toEqual([]);
    }
  });
});

function browserApi(): BrowserGameApi {
  let frames = 9_007_199_254_740_992n;
  return {
    version: 4,
    cancellableRun: true,
    status: () => ({ running: false, session: {}, page: { sessionGeneration: 1 } }),
    setRunning: vi.fn(),
    commandJson: vi.fn((text: string) => text === '{"cmd":"status"}' ? `{"frames":${frames}}` : text),
    run: vi.fn(async ({ count }: { cmd: 'frames'; count: number }) => {
      frames += BigInt(count);
      return { frames: Number(frames) };
    }),
  };
}

describe('browser game transport', () => {
  it('waits for the same-origin runtime and preserves exact command JSON', async () => {
    const api = browserApi();
    const target = { closed: false, location: { origin: 'http://localhost:5176' }, xem: undefined as BrowserGameApi | undefined };
    const connecting = browserGameTransport(target, 'http://localhost:5176', 500);
    target.xem = api;
    const transport = await connecting;
    expect(transport.kind).toBe('browser');
    await expect(transport.request('{"cmd":"inspect","path":"wide"}')).resolves.toBe('{"cmd":"inspect","path":"wide"}');
    const wideWrite = '{"cmd":"write","path":"wide","value":18446744073709551615}';
    const wideReply = '{"value":18446744073709551615}';
    vi.mocked(api.commandJson).mockReturnValueOnce(wideReply);
    await expect(transport.request(wideWrite)).resolves.toBe(wideReply);
    expect(api.commandJson).toHaveBeenLastCalledWith(wideWrite);
    expect(gameReply(wideReply)).toEqual({ value: new ExactInteger('18446744073709551615') });
    await expect(transport.running(false)).resolves.toBe(false);
    const frameReply = await transport.frames(3);
    expect(gameReply(frameReply)).toEqual({ frames: new ExactInteger('9007199254740995') });
    await transport.close();
    await expect(transport.request('{}')).rejects.toThrow(/detached/);
  });

  it('aborts an in-flight exact frame step when the browser connection closes', async () => {
    const api = browserApi();
    let started!: () => void;
    let receivedSignal: AbortSignal | undefined;
    const begun = new Promise<void>(resolve => { started = resolve; });
    api.run = vi.fn(async (_command, options) => {
      receivedSignal = options?.signal;
      started();
      return new Promise((_resolve, reject) => {
        receivedSignal?.addEventListener('abort', () => reject(receivedSignal?.reason), { once: true });
      });
    });
    const transport = await browserGameTransport({ closed: false, location: { origin: 'http://localhost:5176' }, xem: api }, 'http://localhost:5176', 10);
    const stepping = transport.frames(1);
    await begun;
    expect(receivedSignal).toBeInstanceOf(AbortSignal);
    await transport.close();
    expect(receivedSignal?.aborted).toBe(true);
    await expect(stepping).rejects.toThrow(/detached/);
  });

  it('rejects wrong origins, incompatible APIs, closed windows, and blocked frame replies', async () => {
    let wrongOrigin: unknown;
    try { await browserGameTransport({ closed: false, location: { origin: 'https://other.test' }, xem: browserApi() }, 'http://localhost:5176', 10); }
    catch (error) { wrongOrigin = error; }
    expect(wrongOrigin).toMatchObject({ message: 'Cannot attach to browser game', cause: expect.objectContaining({ message: expect.stringMatching(/same origin/) }) });
    let incompatible: unknown;
    try { await browserGameTransport({ closed: false, location: { origin: 'http://localhost:5176' }, xem: { ...browserApi(), version: 3 } }, 'http://localhost:5176', 10); }
    catch (error) { incompatible = error; }
    expect(incompatible).toMatchObject({ message: 'Cannot attach to browser game', cause: expect.objectContaining({ message: expect.stringMatching(/required window.xem/) }) });
    const api = browserApi();
    api.run = vi.fn(async () => ({ blocked: true, missing: [[0, 32]] }));
    const transport = await browserGameTransport({ closed: false, location: { origin: 'http://localhost:5176' }, xem: api }, 'http://localhost:5176', 10);
    await expect(transport.frames(1)).rejects.toThrow(/disc-blocked/);
    const lost = { closed: false, location: { origin: 'http://localhost:5176' }, xem: api };
    const attached = await browserGameTransport(lost, 'http://localhost:5176', 10);
    lost.closed = true;
    await expect(attached.running()).rejects.toThrow(/closed/);
    await attached.close();
    await transport.close();
  });

  it('invalidates a link when the page boots a replacement session', async () => {
    const api = browserApi();
    let generation = 1;
    api.status = () => ({ running: false, session: {}, page: { sessionGeneration: generation } });
    const transport = await browserGameTransport({ closed: false, location: { origin: 'http://localhost:5176' }, xem: api }, 'http://localhost:5176', 10);
    generation += 1;
    await expect(transport.request('{"cmd":"status"}')).rejects.toThrow(/another session/);
    await transport.close();
  });
});

class FakeChild extends EventEmitter {
  stdin = new PassThrough();
  stdout = new PassThrough();
  stderr = new PassThrough();
  exitCode: number | null = null;
  signalCode: NodeJS.Signals | null = null;
  constructor(reply: (json: string) => void) {
    super();
    this.stdin.on('data', (chunk: Buffer) => {
      for (const line of chunk.toString().trimEnd().split('\n')) if (line) reply(line);
    });
  }
  kill(signal: NodeJS.Signals = 'SIGTERM') { this.emit('exit', null, signal); return true; }
}

describe('native game transport', () => {
  it('spawns only the requested executable with an argument array, paused control, and isolated settings', async () => {
    const child = new FakeChild(() => {});
    const spawn = vi.fn(() => child);
    const environment = { DISPLAY: ':88', WAYLAND_DISPLAY: 'wl-test', PATH: '/test/bin' };
    const transport = launchNativeGame({ executable: '/opt/xem/xem-desktop', disc: '/data/disc1.bin', settingsDirectory: '/tmp/studio-settings', cardsDirectory: '/tmp/studio-cards' }, spawn as never, environment);
    expect(spawn).toHaveBeenCalledWith('/opt/xem/xem-desktop', ['--disc', '/data/disc1.bin', '--cards', '/tmp/studio-cards', '--control', '--control-paused'], expect.objectContaining({ shell: false, env: expect.objectContaining({ ...environment, XEM_DATA_DIR: '/tmp/studio-settings' }) }));
    await transport.close();
    expect(() => launchNativeGame({ executable: 'xem-desktop', disc: '/data/disc1.bin', settingsDirectory: '/tmp/settings', cardsDirectory: '/tmp/cards' }, spawn as never, environment)).toThrow(/explicit absolute local path/);
  });

  it('frames partial stdout lines and enforces one outstanding request', async () => {
    let sendReply!: () => void;
    const child = new FakeChild(json => {
      expect(JSON.parse(json)).toEqual({ cmd: 'status' });
      sendReply = () => {
        child.stdout.write('{"host_');
        child.stdout.write('running":false}\n');
      };
    });
    const transport = nativeGameTransport(child as never);
    const first = transport.request('{"cmd":"status"}');
    await expect(transport.request('{"cmd":"status"}')).rejects.toThrow(/already pending/);
    sendReply();
    await expect(first).resolves.toBe('{"host_running":false}');
    await transport.close();
  });

  it('fails closed on malformed replies, EOF, and child exit', async () => {
    const malformedChild = new FakeChild(() => queueMicrotask(() => malformedChild.stdout.write('not-json\n')));
    const malformed = nativeGameTransport(malformedChild as never);
    await expect(malformed.request('{}')).rejects.toThrow(/Malformed native control JSON/);

    const eofChild = new FakeChild(() => queueMicrotask(() => eofChild.stdout.end()));
    const eof = nativeGameTransport(eofChild as never);
    await expect(eof.request('{}')).rejects.toThrow(/stdout ended/);

    const exitedChild = new FakeChild(() => queueMicrotask(() => exitedChild.emit('exit', 17, null)));
    const exited = nativeGameTransport(exitedChild as never);
    await expect(exited.request('{}')).rejects.toThrow(/exited \(code 17/);
  });
});

function controllerTransport(request: (json: string) => Promise<string> = async json => {
  const command = JSON.parse(json) as { cmd: string; count?: number };
  if (command.cmd === 'schema') return JSON.stringify({ identity: { module: 'a'.repeat(64), disc: 'disc-1', packages: [] }, schema: { module_sha256: 'a'.repeat(64), globals: {}, records: {}, types: [] } });
  if (command.cmd === 'status') return JSON.stringify({ identity: { module: 'a'.repeat(64), disc: 'disc-1', packages: [] }, host_running: false, frame: 0 });
  return JSON.stringify({ ok: true });
}): GameTransport {
  let running = true;
  return {
    kind: 'browser', request,
    frames: async count => JSON.stringify({ status: { frame: count } }),
    running: async value => { if (value !== undefined) running = value; return running; },
    onLoss() { return () => {}; },
    close: async () => {},
  };
}

describe('serialized live game controller', () => {
  it('stores rejected operations without duplicating UI reports and reports asynchronous link loss once', async () => {
    const operationFailure = new Error('invalid field write');
    const linkFailure = new Error('browser game window closed');
    const report = vi.fn();
    let lose: ((error: Error) => void) | undefined;
    const transport: GameTransport = {
      ...controllerTransport(async json => {
        if ((JSON.parse(json) as { cmd: string }).cmd === 'invalid') throw operationFailure;
        return controllerTransport().request(json);
      }),
      onLoss(listener) { lose = listener; return () => { lose = undefined; }; },
    };
    const controller = new GameController(report, () => {});
    await controller.connect(transport);

    await expect(controller.command({ cmd: 'invalid' })).rejects.toBe(operationFailure);
    expect(controller.getSnapshot().error).toBe(operationFailure);
    expect(report).not.toHaveBeenCalled();

    lose!(linkFailure);
    expect(controller.getSnapshot().error).toBe(linkFailure);
    expect(report).toHaveBeenCalledTimes(1);
    expect(report).toHaveBeenCalledWith(linkFailure);
  });

  it('connects only with a matching schema identity, pauses before exact steps, and scopes snapshots to the link', async () => {
    const transport = controllerTransport();
    const reported: unknown[] = [];
    const controller = new GameController(error => reported.push(error), () => {});
    await controller.connect(transport);
    expect(controller.getSnapshot().connected).toBe(true);
    expect(controller.getSnapshot().running).toBe(false);
    await expect(controller.step(2)).resolves.toEqual({ status: { frame: 2 } });
    expect(controller.getSnapshot().status).toEqual({ status: { frame: 2 } });
    await controller.snapshot('before test');
    expect(controller.getSnapshot().snapshots).toEqual(['before test']);
    await controller.restore('before test');
    await controller.disconnect();
    expect(controller.getSnapshot().snapshots).toEqual([]);
    await expect(controller.restore('before test')).rejects.toThrow(/snapshot belonging to this connection/);
    expect(reported).toEqual([]);
  });

  it('discards a delayed reply from a superseded connection without overwriting the new session', async () => {
    let finishSlow!: (reply: string) => void;
    const started = vi.fn();
    const first = controllerTransport(async json => {
      if ((JSON.parse(json) as { cmd: string }).cmd === 'slow') {
        started();
        return new Promise(resolve => { finishSlow = resolve; });
      }
      return controllerTransport().request(json);
    });
    const second = controllerTransport();
    const controller = new GameController(() => {}, () => {});
    await controller.connect(first);
    const delayed = controller.command({ cmd: 'slow' });
    await vi.waitFor(() => expect(started).toHaveBeenCalledOnce());
    await controller.connect(second);
    finishSlow('{"ok":"stale"}');
    await expect(delayed).rejects.toThrow(/connection changed/);
    expect(controller.getSnapshot().connected).toBe(true);
    expect(controller.getSnapshot().schema?.identity.module).toBe('a'.repeat(64));
  });

  it('serializes commands and closes an obsolete connection attempt without replacing the newest one', async () => {
    let active = 0;
    let maximum = 0;
    const serialized = controllerTransport(async json => {
      const command = JSON.parse(json) as { cmd: string };
      if (command.cmd === 'schema' || command.cmd === 'status') return controllerTransport().request(json);
      active += 1; maximum = Math.max(maximum, active);
      await new Promise(resolve => setTimeout(resolve, 5));
      active -= 1;
      return '{"ok":true}';
    });
    const controller = new GameController(() => {}, () => {});
    await controller.connect(serialized);
    await Promise.all([controller.command({ cmd: 'first' }), controller.command({ cmd: 'second' })]);
    expect(maximum).toBe(1);

    let finishClose!: () => void;
    const current = controllerTransport();
    current.close = () => new Promise(resolve => { finishClose = resolve; });
    await controller.connect(current);
    const obsolete = controllerTransport();
    const obsoleteConnection = controller.connect(obsolete);
    await vi.waitFor(() => expect(finishClose).toBeTypeOf('function'));
    const newest = controllerTransport();
    await controller.connect(newest);
    finishClose();
    await expect(obsoleteConnection).rejects.toThrow(/superseded/);
    expect(controller.getSnapshot().connected).toBe(true);
    expect(controller.getSnapshot().kind).toBe('browser');
  });
});
