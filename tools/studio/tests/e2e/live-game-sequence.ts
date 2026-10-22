import { gameJson, record, type GameValue } from '../../src/game/protocol';
import type { GameController } from '../../src/game/controller';
import type { SchemaNode, SchemaModel } from '../../src/game/schema';

export interface LiveParityProbe {
  identity: { module: string; disc: string; packages: string[] };
  statusSnapshots: string[];
  hashes: string[];
  fieldMap: string;
  validWriteReadback: string;
  invalidWriteError: string;
  changedHash: string;
  restoredHash: string;
  framesWhilePaused: [string, string];
}

function canonical(value: GameValue): string {
  const visit = (item: GameValue, topLevel = false): unknown => {
    if (item !== null && typeof item === 'object' && !Array.isArray(item) && 'decimal' in item && typeof item.decimal === 'string') {
      return { exactInteger: item.decimal };
    }
    if (Array.isArray(item)) return item.map(nested => visit(nested));
    if (item !== null && typeof item === 'object') {
      return Object.fromEntries(Object.entries(item)
        .filter(([key]) => !(topLevel && (key === 'host_running' || key === 'renderer')))
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, nested]) => [key, visit(nested)]));
    }
    return item;
  };
  return JSON.stringify(visit(value, true));
}

function findPath(schema: SchemaModel, path: string): SchemaNode {
  const pending = schema.roots(path.split('.')[0] ?? '');
  while (pending.length) {
    const node = pending.shift()!;
    if (node.path === path) return node;
    pending.push(...schema.children(node).filter(child => !child.range && !child.unsupported));
  }
  throw new Error(`Required schema scalar is absent: ${path}`);
}

function decimal(value: GameValue): string {
  if (typeof value === 'number' || typeof value === 'string') return String(value);
  if (value !== null && typeof value === 'object' && !Array.isArray(value) && 'decimal' in value && typeof value.decimal === 'string') return value.decimal;
  throw new Error('Expected a scalar game value');
}

/** A shared sequence run unchanged through both production GameController instances. */
export async function runLiveParitySequence(controller: GameController): Promise<LiveParityProbe> {
  const connected = controller.getSnapshot();
  if (!connected.schema || !connected.status || connected.running !== false) throw new Error('Controller did not acknowledge a bound, paused session');
  const statusSnapshots: string[] = [];
  const hashes: string[] = [];
  const capture = async () => {
    const status = await controller.refresh();
    statusSnapshots.push(canonical(status));
    const hash = await controller.command({ cmd: 'memory_hash' });
    hashes.push(decimal(hash));
    return record(status, 'Runtime status');
  };

  const identity = { ...connected.schema.identity };
  await controller.command({ cmd: 'rewind_configure', max_bytes: 0 });
  const initial = await capture();
  if (initial.frames !== 0) throw new Error(`The paused launch advanced before ownership: frame ${decimal(initial.frames as GameValue)}`);
  await controller.command({ cmd: 'boot_mode', mode: 0 });
  const title = record(await controller.step(303), '303-frame title replay');
  statusSnapshots.push(canonical(title));
  hashes.push(decimal(await controller.command({ cmd: 'memory_hash' })));
  if (decimal(title.frames as GameValue) !== '303') throw new Error('The first bounded replay did not stop at exactly frame 303');

  await controller.command({
    cmd: 'scenario', mode: 1,
    writes: [
      { path: 'game_data.map', value: 30 },
      { path: 'mode_kernel_menu_running', value: 0 },
      { path: 'mode_field_entered_once', value: 0 },
    ],
  });
  const field = record(await controller.step(400), '400-frame field replay');
  statusSnapshots.push(canonical(field));
  hashes.push(decimal(await controller.command({ cmd: 'memory_hash' })));
  if (decimal(field.frames as GameValue) !== '703') throw new Error('The field replay did not stop at exactly frame 703');
  if (field.mode !== 1) throw new Error(`The shared field replay did not enter field mode (mode ${String(field.mode)})`);

  const mapValue = await controller.inspect(findPath(connected.schema, 'mode_field_map_id'));
  const fieldMap = decimal(mapValue);
  if ((Number(fieldMap) & 0x0fff) !== 30) throw new Error(`The shared field replay entered map ${fieldMap}, not map 30`);
  const sameValue = await controller.write(findPath(connected.schema, 'mode_field_map_id'), fieldMap);
  const validWriteReadback = decimal(sameValue);

  await controller.snapshot('m4-parity');
  const beforeChange = decimal(await controller.command({ cmd: 'memory_hash' }));
  const mapNode = findPath(connected.schema, 'game_data.map');
  await controller.write(mapNode, '31');
  const changedHash = decimal(await controller.command({ cmd: 'memory_hash' }));
  if (changedHash === beforeChange) throw new Error('The valid schema write did not change the captured state');

  const invalidBefore = decimal(await controller.command({ cmd: 'memory_hash' }));
  let invalidWriteError = '';
  try {
    await controller.command({ cmd: 'write', path: '__m4_invalid_control_path__', value: 1 });
  } catch (error) {
    invalidWriteError = error instanceof Error ? error.message : String(error);
  }
  if (!invalidWriteError) throw new Error('The runtime accepted a write to a nonexistent path');
  const invalidAfter = decimal(await controller.command({ cmd: 'memory_hash' }));
  if (invalidBefore !== invalidAfter) throw new Error('The refused write changed game memory');

  await controller.restore('m4-parity');
  const restoredHash = decimal(await controller.command({ cmd: 'memory_hash' }));
  if (restoredHash !== beforeChange) throw new Error('Restoring the named session snapshot did not restore the memory hash');
  const restored = await capture();
  const frameBeforeDelay = decimal(restored.frames as GameValue);
  await new Promise(resolve => setTimeout(resolve, 300));
  const pausedStatus = record(await controller.refresh(), 'Paused status after delay');
  const frameAfterDelay = decimal(pausedStatus.frames as GameValue);
  if (controller.getSnapshot().running !== false || frameBeforeDelay !== frameAfterDelay) throw new Error('The acknowledged paused host advanced during the delay');

  return {
    identity,
    statusSnapshots,
    hashes,
    fieldMap,
    validWriteReadback,
    invalidWriteError,
    changedHash,
    restoredHash,
    framesWhilePaused: [frameBeforeDelay, frameAfterDelay],
  };
}

export async function runBrowserParityPage(): Promise<LiveParityProbe> {
  const { browserGameTransport } = await import('../../src/game/browser');
  const { GameController } = await import('../../src/game/controller');
  const controller = new GameController(() => {}, () => {});
  const transport = await browserGameTransport(window as unknown as import('../../src/game/browser').GameWindow, window.location.origin);
  await controller.connect(transport);
  try {
    return await runLiveParitySequence(controller);
  } finally {
    await controller.disconnect();
  }
}
