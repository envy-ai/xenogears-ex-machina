import { frameCount, gameJson, gameReply, record, type GameTransport, type GameValue } from './protocol';
import { SchemaModel, type SchemaNode } from './schema';

export interface LinkState {
  connected: boolean; busy: number; running: boolean | null; error: Error | null;
  revision: number; schema: SchemaModel | null; status: GameValue | null;
  snapshots: string[]; kind: 'native' | 'browser' | null;
}
/** One queue owns all live mutations. Epoch checks reject old connection results. */
export class GameController {
  private transport: GameTransport | null = null;
  private epoch = 0;
  private connectionAttempt = 0;
  private queue: Promise<unknown> = Promise.resolve();
  private unsubscribe: (() => void) | null = null;
  private listeners = new Set<() => void>();
  private state: LinkState = { connected: false, busy: 0, running: null, error: null, revision: 0, schema: null, status: null, snapshots: [], kind: null };
  constructor(private readonly report: (error: unknown) => void, private readonly log: (kind: string, text: string) => void) {}
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private update(change: Partial<LinkState>) { this.state = { ...this.state, ...change }; for (const listener of this.listeners) listener(); }
  private check(epoch: number, transport: GameTransport) {
    if (epoch !== this.epoch || transport !== this.transport || !this.state.connected) throw new Error('The game connection changed before the operation completed');
  }
  async connect(transport: GameTransport): Promise<void> {
    const attempt = ++this.connectionAttempt;
    await this.disconnectCurrent();
    if (attempt !== this.connectionAttempt) {
      await transport.close();
      throw new Error('This game connection attempt was superseded');
    }
    const epoch = ++this.epoch;
    this.transport = transport;
    this.queue = Promise.resolve();
    this.update({ connected: true, error: null, running: null, kind: transport.kind });
    this.unsubscribe = transport.onLoss(error => {
      if (epoch !== this.epoch) return;
      ++this.epoch;
      this.update({ connected: false, running: null, busy: 0, schema: null, status: null, snapshots: [], error, revision: this.state.revision + 1 });
      this.report(error);
    });
    try {
      await this.enqueue(async t => {
        const running = await t.running(false);
        if (running) throw new Error('Runtime did not acknowledge paused command ownership');
        const schema = new SchemaModel(gameReply(await t.request('{"cmd":"schema"}')));
        const status = gameReply(await t.request('{"cmd":"status"}'));
        const identity = record(record(status, 'Runtime status').identity, 'Runtime identity');
        if (identity.module !== schema.identity.module || identity.disc !== schema.identity.disc || gameJson(identity.packages) !== gameJson(schema.identity.packages)) throw new Error('Runtime status and schema content identities differ');
        this.update({ running, schema, status, snapshots: [], revision: this.state.revision + 1 });
        this.log('Game', `Connected to ${t.kind}; paused; module ${schema.identity.module}`);
      }, false);
    } catch (error) {
      if (epoch === this.epoch && transport === this.transport) await this.disconnectCurrent();
      else await transport.close();
      throw error;
    }
  }
  async disconnect(): Promise<void> {
    ++this.connectionAttempt;
    await this.disconnectCurrent();
  }
  private async disconnectCurrent(): Promise<void> {
    ++this.epoch;
    this.unsubscribe?.(); this.unsubscribe = null;
    const transport = this.transport; this.transport = null;
    this.update({ connected: false, running: null, busy: 0, schema: null, status: null, snapshots: [], kind: null, revision: this.state.revision + 1 });
    if (transport) await transport.close();
  }
  private enqueue<T>(work: (transport: GameTransport) => Promise<T>, pause = true): Promise<T> {
    const transport = this.transport;
    const epoch = this.epoch;
    if (!transport || !this.state.connected) return Promise.reject(this.state.error ?? new Error('No game is connected'));
    this.update({ busy: this.state.busy + 1 });
    const next = this.queue.then(async () => {
      this.check(epoch, transport);
      if (pause) {
        const running = await transport.running(false); this.check(epoch, transport);
        if (running) throw new Error('Runtime did not acknowledge paused command ownership');
        this.update({ running });
      }
      const guarded = async <V,>(operation: Promise<V>): Promise<V> => {
        const value = await operation;
        this.check(epoch, transport);
        return value;
      };
      const owned: GameTransport = {
        ...transport,
        request: json => guarded(transport.request(json)),
        frames: count => guarded(transport.frames(count)),
        running: value => guarded(transport.running(value)),
      };
      const result = await work(owned);
      this.check(epoch, transport);
      return result;
    });
    this.queue = next.catch(() => {});
    return next.catch(error => {
      // The caller reports rejected operations; only unsolicited loss uses report.
      if (epoch === this.epoch) this.update({ error: error instanceof Error ? error : new Error(String(error)) });
      throw error;
    }).finally(() => { if (epoch === this.epoch) this.update({ busy: this.state.busy - 1 }); });
  }
  setRunning(running: boolean): Promise<void> {
    return this.enqueue(async transport => {
      const acknowledged = await transport.running(running);
      if (acknowledged !== running) throw new Error('Runtime running acknowledgement differs from requested state');
      this.update({ running: acknowledged, revision: this.state.revision + 1 });
      this.log('Game', acknowledged ? 'Running (acknowledged)' : 'Paused (acknowledged)');
    }, false);
  }
  command(command: { [key: string]: GameValue }): Promise<GameValue> {
    if (command.cmd === 'frames') {
      if (typeof command.count !== 'number') return Promise.reject(new Error('Frame count must be an integer from 1 to 10000'));
      return this.step(command.count);
    }
    return this.enqueue(async transport => {
      const reply = gameReply(await transport.request(gameJson(command)));
      if (reply !== null && typeof reply === 'object' && !Array.isArray(reply) && 'blocked' in reply && reply.blocked === true) throw new Error('Runtime command is incomplete while blocked on disc input; use bounded frame stepping');
      return reply;
    });
  }
  step(count: number): Promise<GameValue> {
    frameCount(count);
    return this.enqueue(async transport => {
      const result = gameReply(await transport.frames(count));
      const status = record(result, 'Frame reply');
      if (status.blocked === true) throw new Error('Runtime returned an incomplete disc-blocked frame command');
      this.update({ status, revision: this.state.revision + 1 });
      this.log('Game', `Stepped ${count} frames`);
      return result;
    });
  }
  refresh(): Promise<GameValue> {
    return this.enqueue(async transport => {
      const status = gameReply(await transport.request('{"cmd":"status"}'));
      this.update({ status, revision: this.state.revision + 1 }); return status;
    });
  }
  paused<T>(work: (request: (command: { [key: string]: GameValue }) => Promise<GameValue>) => Promise<T>): Promise<T> {
    return this.enqueue(transport => work(async command => gameReply(await transport.request(gameJson(command)))));
  }
  inspect(node: SchemaNode): Promise<GameValue> {
    if (node.range || node.unsupported) return Promise.reject(new Error(node.unsupported ?? 'Select a concrete schema element to inspect'));
    return this.command({ cmd: 'inspect', path: node.path, depth: 0 });
  }
  write(node: SchemaNode, text: string): Promise<GameValue> {
    const schema = this.state.schema;
    if (!schema) return Promise.reject(new Error('The attached session has no schema'));
    const value = schema.value(node, text);
    return this.enqueue(async transport => {
      gameReply(await transport.request(gameJson({ cmd: 'write', path: node.path, value })));
      const readback = gameReply(await transport.request(gameJson({ cmd: 'inspect', path: node.path, depth: 0 })));
      this.update({ revision: this.state.revision + 1 });
      this.log('Game', `Wrote and read back ${node.path}: ${gameJson(readback)}`);
      return readback;
    });
  }
  snapshot(name: string): Promise<void> {
    if (!name.trim()) return Promise.reject(new Error('Snapshot name is required'));
    return this.enqueue(async transport => {
      gameReply(await transport.request(gameJson({ cmd: 'snapshot', name })));
      this.update({ snapshots: [...new Set([...this.state.snapshots, name])] });
      this.log('Game', `Captured snapshot ${name}`);
    });
  }
  restore(name: string): Promise<void> {
    if (!this.state.snapshots.includes(name)) return Promise.reject(new Error('Select a snapshot belonging to this connection'));
    return this.enqueue(async transport => {
      gameReply(await transport.request(gameJson({ cmd: 'restore', name })));
      const status = gameReply(await transport.request('{"cmd":"status"}'));
      this.update({ status, revision: this.state.revision + 1 });
      this.log('Game', `Restored snapshot ${name}`);
    });
  }
}
