import type { GameController } from '../../src/game/controller';
import type { GameValue } from '../../src/game/protocol';
import type { SchemaNode } from '../../src/game/schema';
import { nativeHost } from '../../src/platform/host';

interface Config {
  executable: string;
  disc: string;
  settingsDirectory: string;
  cardsDirectory: string;
  output: string;
  waylandRuntimeDirectory: string;
  waylandDisplay: string;
  lavapipeIcd?: string;
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

async function waitFor(predicate: () => boolean, description: string): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error(`Timed out waiting for ${description}`);
    await new Promise(resolve => setTimeout(resolve, 25));
  }
}

function inputFor(labelText: string): HTMLInputElement {
  const row = [...document.querySelectorAll<HTMLElement>('.live-path')]
    .find(candidate => candidate.querySelector('label')?.textContent?.trim() === labelText);
  const input = row?.querySelector('input');
  assert(input, `Missing native launch input: ${labelText}`);
  return input;
}

function setInput(input: HTMLInputElement, value: string): void {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  assert(setter, 'HTML input value setter is unavailable');
  setter.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
}

function button(name: string): HTMLButtonElement {
  const match = [...document.querySelectorAll<HTMLButtonElement>('button')]
    .find(candidate => candidate.textContent?.trim() === name);
  assert(match && !match.disabled, `Missing or disabled button: ${name}`);
  return match;
}

function findNode(nodes: SchemaNode[], path: string, schema: NonNullable<ReturnType<GameController['getSnapshot']>['schema']>): SchemaNode | undefined {
  const pending = [...nodes];
  while (pending.length) {
    const node = pending.shift()!;
    if (node.path === path) return node;
    pending.push(...schema.children(node).filter(child => !child.range && !child.unsupported));
  }
  return undefined;
}

function scalarText(value: GameValue): string {
  if (typeof value === 'number' || typeof value === 'string') return String(value);
  if (value !== null && typeof value === 'object' && !Array.isArray(value) && 'decimal' in value && typeof value.decimal === 'string') return value.decimal;
  throw new Error('The selected schema value is not an exact scalar');
}

function safeStatus(value: GameValue | null): Record<string, unknown> {
  assert(value !== null && typeof value === 'object' && !Array.isArray(value), 'Native controller has no status object');
  const status = value as Record<string, GameValue>;
  return {
    frames: status.frames === undefined ? null : scalarText(status.frames),
    mode: status.mode ?? null,
    disc: status.disc ?? null,
    identity: status.identity ?? null,
  };
}

/** Test-only NW driver; this uses the same launch form and controller as the application. */
export async function runLiveNwTest(controller: GameController): Promise<void> {
  if (!nativeHost) throw new Error('The live NW driver requires the desktop host');
  const fs = nativeHost.require('node:fs/promises');
  const path = nativeHost.require('node:path');
  const argument = nativeHost.App.argv.find(value => value.startsWith('--studio-live-test='));
  assert(argument, 'Missing --studio-live-test config path');

  let outputPath: string | undefined;
  let failure: unknown;
  let result: Record<string, unknown>;
  try {
    const configPath = argument.slice('--studio-live-test='.length);
    const config = JSON.parse(await fs.readFile(configPath, 'utf8')) as Config;
    for (const key of ['executable', 'disc', 'settingsDirectory', 'cardsDirectory', 'output'] as const) {
      assert(typeof config[key] === 'string' && config[key].length > 0, `The live NW config requires ${key}`);
    }
    outputPath = path.resolve(config.output);
    const processEnvironment = nativeHost.require('node:process').env;
    processEnvironment.XDG_RUNTIME_DIR = config.waylandRuntimeDirectory;
    processEnvironment.WAYLAND_DISPLAY = config.waylandDisplay;
    processEnvironment.SDL_VIDEODRIVER = 'wayland';
    processEnvironment.SDL_AUDIO_DRIVER = 'dummy';
    if (config.lavapipeIcd) processEnvironment.VK_ICD_FILENAMES = config.lavapipeIcd;

    setInput(inputFor('Runtime executable'), config.executable);
    setInput(inputFor('Game disc path'), config.disc);
    setInput(inputFor('Isolated settings directory'), config.settingsDirectory);
    setInput(inputFor('Isolated cards directory'), config.cardsDirectory);
    button('Launch & connect').click();

    await waitFor(() => {
      const state = controller.getSnapshot();
      if (state.error && !state.connected) throw state.error;
      return state.connected && state.busy === 0 && state.running === false && state.schema !== null && state.status !== null;
    }, 'the native game controller to finish schema loading and acknowledge pause');
    const connected = controller.getSnapshot();
    assert(connected.kind === 'native' && connected.running === false && connected.schema, 'The controller did not acknowledge a paused native session with a schema');
    assert(!connected.error, `The controller reported a connection error: ${connected.error?.message ?? ''}`);

    const beforeStep = controller.getSnapshot().status;
    assert(beforeStep, 'The connected native controller has no initial status');
    const beforeStatus = safeStatus(beforeStep);
    button('Step').click();
    await waitFor(() => controller.getSnapshot().busy === 0 && controller.getSnapshot().revision > connected.revision, 'one bounded UI step');
    const afterStep = safeStatus(controller.getSnapshot().status);
    assert(BigInt(String(afterStep.frames)) === BigInt(String(beforeStatus.frames)) + 1n, 'The native UI step did not advance exactly one frame');

    const inspector = document.querySelector<HTMLElement>('[aria-label="Schema inspector"]');
    assert(inspector, 'The schema inspector is not mounted');
    const filter = inspector.querySelector<HTMLInputElement>('input[type="search"]');
    assert(filter, 'The schema global filter is missing');
    setInput(filter, 'mode_field_map_id');
    const node = findNode(connected.schema.roots('mode_field_map_id'), 'mode_field_map_id', connected.schema);
    assert(node && connected.schema.scalar(node), 'The live map identifier is not a writable scalar in the loaded schema');
    await waitFor(() => Boolean(inspector.querySelector('[data-schema-node="mode_field_map_id"]')), 'the filtered schema row');
    const treeItem = inspector.querySelector<HTMLButtonElement>('[data-schema-node="mode_field_map_id"]');
    assert(treeItem, 'The live map identifier is missing from the schema tree');
    treeItem.click();
    await waitFor(() => {
      const text = inspector.querySelector<HTMLElement>('[aria-label="Inspected value"]')?.textContent?.trim();
      if (!text || text === 'Refresh to inspect the current state.') return false;
      try { JSON.parse(text); return true; } catch { return false; }
    }, 'the selected scalar inspection');
    const oldText = inspector.querySelector<HTMLElement>('[aria-label="Inspected value"]')!.textContent!.trim();
    const oldValue = JSON.parse(oldText) as GameValue;
    const oldDecimal = scalarText(oldValue);
    const nextDecimal = (BigInt(oldDecimal) + 1n).toString();

    const snapshotName = `m4-nw-${Date.now()}`;
    const snapshotInput = [...document.querySelectorAll<HTMLInputElement>('input')]
      .find(input => input.closest('label')?.textContent?.includes('Snapshot name'));
    assert(snapshotInput, 'The snapshot name input is missing');
    setInput(snapshotInput, snapshotName);
    button('Capture').click();
    await waitFor(() => controller.getSnapshot().snapshots.includes(snapshotName), 'the native runtime snapshot');

    const writeLabel = [...inspector.querySelectorAll<HTMLLabelElement>('label')]
      .find(label => label.textContent?.includes('New scalar value'));
    const writeInput = writeLabel?.querySelector('input');
    assert(writeInput, 'The scalar write input is missing');
    setInput(writeInput, nextDecimal);
    const writeButton = [...inspector.querySelectorAll<HTMLButtonElement>('button')]
      .find(candidate => candidate.textContent?.trim() === 'Write & read back');
    assert(writeButton && !writeButton.disabled, 'The scalar write action is unavailable');
    writeButton.click();
    await waitFor(() => inspector.querySelector('[role="status"]')?.textContent?.includes(`Read-back: ${nextDecimal}`) ?? false, 'the scalar write readback');
    const writtenText = inspector.querySelector<HTMLElement>('[aria-label="Inspected value"]')?.textContent?.trim();
    assert(writtenText && scalarText(JSON.parse(writtenText) as GameValue) === nextDecimal, 'The native scalar readback differs from the requested value');

    const snapshotLabel = [...document.querySelectorAll<HTMLLabelElement>('label')]
      .find(label => label.textContent?.trim().startsWith('Session snapshots'));
    const snapshots = snapshotLabel?.querySelector<HTMLSelectElement>('select');
    assert(snapshots, 'The session snapshot selector is missing');
    snapshots.value = snapshotName;
    snapshots.dispatchEvent(new Event('change', { bubbles: true }));
    button('Restore…').click();
    await waitFor(() => Boolean(document.querySelector('[role="dialog"][aria-label="Restore session snapshot"]')), 'the restore confirmation dialog to mount');
    const dialog = document.querySelector<HTMLElement>('[role="dialog"][aria-label="Restore session snapshot"]');
    assert(dialog, 'The restore confirmation dialog is missing');
    const restoreButton = [...dialog.querySelectorAll<HTMLButtonElement>('button')]
      .find(candidate => candidate.textContent?.trim() === 'Restore');
    assert(restoreButton, 'The restore confirmation action is missing');
    const beforeRestoreRevision = controller.getSnapshot().revision;
    restoreButton.click();
    await waitFor(() => controller.getSnapshot().revision > beforeRestoreRevision
      && controller.getSnapshot().busy === 0
      && !document.querySelector('[role="dialog"][aria-label="Restore session snapshot"]'), 'snapshot restoration');

    const refresh = [...inspector.querySelectorAll<HTMLButtonElement>('button')]
      .find(candidate => candidate.textContent?.trim() === 'Refresh value');
    assert(refresh, 'The selected scalar refresh action is missing');
    refresh.click();
    await waitFor(() => {
      const text = inspector.querySelector<HTMLElement>('[aria-label="Inspected value"]')?.textContent?.trim();
      if (!text || text === 'Refresh to inspect the current state.') return false;
      try { return scalarText(JSON.parse(text) as GameValue) === oldDecimal; } catch { return false; }
    }, 'the restored scalar value');

    result = {
      connected: true,
      kind: connected.kind,
      identity: connected.schema.identity,
      module: connected.schema.identity.module,
      before: beforeStatus,
      afterStep,
      oldValue: oldDecimal,
      writtenValue: nextDecimal,
      restoredValue: oldDecimal,
      snapshotCaptured: controller.getSnapshot().snapshots.includes(snapshotName),
      restored: true,
      errors: 0,
    };
  } catch (error) {
    failure = error;
    const detail = error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : { message: String(error) };
    result = { error: detail };
  } finally {
    try {
      await controller.disconnect();
    } catch (error) {
      failure ??= error;
      console.error('Failed to disconnect the NW smoke-owned native controller', error);
    }
    if (outputPath) {
      try {
        await fs.mkdir(path.dirname(outputPath), { recursive: true });
        await fs.writeFile(outputPath, `${JSON.stringify(result!)}\n`, { flag: 'wx' });
      } catch (error) {
        failure ??= error;
        console.error('Failed to write the NW live smoke result', error);
      }
    }
    if (failure) {
      console.error('NW live game smoke failed', failure);
      process.exitCode = 1;
    }
    nativeHost.App.quit();
  }
}
