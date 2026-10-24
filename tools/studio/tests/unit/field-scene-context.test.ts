import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const testDoubles = vi.hoisted(() => {
  class TestStudioCommandError extends Error {
    constructor(readonly kind: string, message: string) {
      super(`${kind}: ${message}`);
    }
  }

  const effects: (() => unknown)[] = [];
  const controller = {
    getSnapshot: vi.fn((): { connected: boolean; error: Error | null; schema?: { identity: { module: string } } | null } => ({ connected: false, error: null })),
    command: vi.fn(), subscribe: vi.fn(),
  };
  return {
    TestStudioCommandError,
    useCallback: (callback: unknown) => callback,
    useEffect: vi.fn((effect: () => unknown) => { effects.push(effect); }),
    useRef: (current: unknown) => ({ current }),
    useState: (initial: unknown) => [initial, vi.fn()],
    useContext: vi.fn(() => controller),
    useSyncExternalStore: vi.fn((): {
      connected: boolean; busy: number; running: boolean | null; error: Error | null; revision: number;
      schema: { identity: { module: string } } | null; status: unknown; snapshots: string[]; kind: string | null;
    } => ({
      connected: false, busy: 0, running: null, error: null, revision: 0,
      schema: null, status: null, snapshots: [], kind: null,
    })),
    createContext: vi.fn((value: unknown) => ({ value, Provider: vi.fn() })),
    effects,
    openOwner: vi.fn((..._args: unknown[]): unknown => undefined),
    openScene: vi.fn((..._args: unknown[]): unknown => undefined),
    decodeSceneAssets: vi.fn(),
    eventPackage: { registerSource: vi.fn((..._args: unknown[]) => undefined), register: vi.fn((..._args: unknown[]) => undefined) },
    navigation: { disc: 1, map: 23, graph: null, bytes: [], reveal: vi.fn() },
    controller,
  };
});

vi.mock('react', () => ({
  useCallback: testDoubles.useCallback,
  createContext: testDoubles.createContext,
  useEffect: testDoubles.useEffect,
  useContext: testDoubles.useContext,
  useRef: testDoubles.useRef,
  useState: testDoubles.useState,
  useSyncExternalStore: testDoubles.useSyncExternalStore,
}));

vi.mock('../../src/backend/field-authoring', () => ({
  FieldAuthoring: {
    open: testDoubles.openOwner,
    scene: testDoubles.openScene,
  },
  sameSceneSource: (left: unknown, right: unknown) => left === right,
}));

vi.mock('../../src/backend/scene-assets', () => ({
  decodeSceneAssets: testDoubles.decodeSceneAssets,
}));

vi.mock('../../src/backend/service', () => ({
  StudioCommandError: testDoubles.TestStudioCommandError,
  readCommandResult: vi.fn(),
}));

vi.mock('../../src/events/EventStudio', () => ({
  useEventNavigation: () => testDoubles.navigation,
  useEventPackagePlay: () => testDoubles.eventPackage,
}));
vi.mock('../../src/game/controller', () => ({ GameController: class {} }));
vi.mock('../../src/game/browser', () => ({ browserGameTransport: vi.fn() }));
vi.mock('../../src/game/native', () => ({ launchNativeGame: vi.fn() }));
vi.mock('../../src/game/protocol', () => ({ gameJson: vi.fn(), frameCount: vi.fn(), gameReply: vi.fn(), record: vi.fn() }));
vi.mock('../../src/game/pc', () => ({ readLivePc: vi.fn(), resolveLivePc: vi.fn(), formatLivePc: vi.fn() }));
vi.mock('../../src/game/schema', () => ({ SchemaModel: class {} }));
vi.mock('../../src/platform/host', () => ({ nativeHost: null, selectNativePath: vi.fn() }));
vi.mock('../../src/editor/NumberField', () => ({ NumberField: () => null }));
vi.mock('../../src/editor/RecordSelect', () => ({ RecordSelect: () => null }));
vi.mock('../../src/editor/EditorDialogShell', () => ({ EditorDialogShell: () => null }));
vi.mock('../../src/editor/DockerList', () => ({
  DockerList: () => null, DockerListBody: () => null, DockerListRow: () => null, DockerListEmpty: () => null,
}));

import { StudioCommandError } from '../../src/backend/service';
import { useFieldDraft } from '../../src/events/useFieldDraft';
import { LiveControls } from '../../src/game/LiveGame';
import type { SourceContext, SourceIdentity } from '../../src/backend/discs';

const source: SourceIdentity = {
  disc: 1,
  boot_sha256: 'boot-hash',
  map: 23,
  slot: 652,
  lba: 124528,
  indexed_size: 110052,
  source_sha256: 'source-hash',
};

const graph = { source_sha256: 'event-source', instructions: [], entries: [] } as never;
const originalDocument = { schema_version: 1, base: graph, actions: [], cursor: 0, allocation_starts: [], next_id: 1 };
const originalSnapshot = {
  draft: 12,
  revision: 0,
  modified: false,
  can_undo: false,
  can_redo: false,
  graph,
  bindings: {},
};

function mapTextures() {
  return {
    state: 'ready', map: 23, file: 0xB9 + 2 * 23, slot: 648, lba: 0, indexed_size: 0,
    source_sha256: '0'.repeat(64),
    images: { uploads: [], source_bytes_read: 0, image_count: 0, stop_reason: 'empty_file' },
  };
}

function owner(scene: unknown = null) {
  return {
    snapshot: originalSnapshot,
    scene,
    export: vi.fn(() => originalDocument),
    close: vi.fn(),
  };
}

function makeDraft() {
  const backend = { command: vi.fn() };
  const report = vi.fn();
  const log = vi.fn();
  const hook = useFieldDraft(backend, null, report, log);
  const original = owner();
  testDoubles.openOwner.mockReturnValue(original);
  hook.open(graph);
  return { backend, hook, original, report, log };
}

function registerLiveSourceHandler(report: (error: unknown) => void) {
  const firstEffect = testDoubles.effects.length;
  LiveControls({ report });
  const windowStub = { addEventListener: vi.fn(), removeEventListener: vi.fn() };
  vi.stubGlobal('window', windowStub);
  for (const effect of testDoubles.effects.slice(firstEffect)) effect();
  const handler = testDoubles.eventPackage.registerSource.mock.calls[0]?.[0] as
    ((value: SourceIdentity) => Promise<SourceContext>) | undefined;
  return handler;
}

describe('Scene source-context prerequisite UX', () => {
  afterEach(() => { vi.unstubAllGlobals(); });
  beforeEach(() => {
    vi.clearAllMocks();
    testDoubles.effects.length = 0;
    testDoubles.decodeSceneAssets.mockReturnValue({ schema_version: 1 });
    testDoubles.controller.getSnapshot.mockReturnValue({ connected: false, error: null });
    testDoubles.useContext.mockReturnValue(testDoubles.controller);
    testDoubles.useSyncExternalStore.mockReturnValue({
      connected: false, busy: 0, running: null, error: null, revision: 0,
      schema: null, status: null, snapshots: [], kind: null,
    });
  });

  it('keeps the real disconnected LiveControls guidance and connection cause through openScene', async () => {
    const { hook, original, report } = makeDraft();
    const backendRequirement = new StudioCommandError('source_context_required', 'the original map context is needed');
    const connectionFailure = new Error('the attached game connection is closed');
    testDoubles.controller.getSnapshot.mockReturnValue({ connected: false, error: connectionFailure });
    const sourceHandler = registerLiveSourceHandler(report);
    expect(sourceHandler).toBeTypeOf('function');
    testDoubles.openScene.mockImplementationOnce(() => { throw backendRequirement; });
    hook.registerSource(sourceHandler!);
    const disc = { mapBundle: vi.fn(async () => ({ placeholder: false, bytes: [1, 2, 3], source })), mapTextures: vi.fn(async () => mapTextures()) };

    let rejected: unknown;
    try { await hook.openScene(23, disc as never); } catch (error) { rejected = error; }
    expect(rejected).toBeInstanceOf(StudioCommandError);
    expect(rejected).toMatchObject({ kind: 'source_context_required' });
    expect((rejected as Error).message).toMatch(/one-time/i);
    expect((rejected as Error).message).toMatch(/Live game/i);
    expect((rejected as Error).message).toMatch(/\/runtime\//);
    expect((rejected as Error).message).toMatch(/Open game & attach/i);
    expect((rejected as Error).message).toMatch(/Choose disc image/i);
    expect((rejected as Error).message).toMatch(/Boot/i);
    expect((rejected as Error).message).toMatch(/Disc 1, Map 23/i);
    expect((rejected as Error).message).toMatch(/Save project/i);
    expect((rejected as Error).message).toMatch(/Load project/i);
    expect((rejected as Error).message).toMatch(/offline/i);
    expect((rejected as Error & { cause?: unknown }).cause).toBe(connectionFailure);
    expect(testDoubles.controller.command).not.toHaveBeenCalled();
    expect(testDoubles.openScene).toHaveBeenCalledTimes(1);
    expect(original.close).not.toHaveBeenCalled();
    expect(report).toHaveBeenCalledWith(rejected);
  });

  it('attaches the backend context requirement when disconnected guidance has no prior controller error', async () => {
    const { hook } = makeDraft();
    const backendRequirement = new StudioCommandError('source_context_required', 'complete measured map data is required');
    testDoubles.controller.getSnapshot.mockReturnValue({ connected: false, error: null });
    const sourceHandler = registerLiveSourceHandler(vi.fn());
    hook.registerSource(sourceHandler!);
    testDoubles.openScene.mockImplementationOnce(() => { throw backendRequirement; });
    const disc = { mapBundle: vi.fn(async () => ({ placeholder: false, bytes: [1], source })), mapTextures: vi.fn(async () => mapTextures()) };

    let rejected: unknown;
    try { await hook.openScene(23, disc as never); } catch (error) { rejected = error; }
    expect(rejected).toBeInstanceOf(StudioCommandError);
    expect((rejected as Error & { cause?: unknown }).cause).toBe(backendRequirement);
  });

  it('retries the same source map with the measured context returned by the registered handler', async () => {
    const { hook } = makeDraft();
    const backendRequirement = new StudioCommandError('source_context_required', 'complete measured map data is required');
    const context: SourceContext = {
      schema_version: 1,
      provenance: 'runtime_field_decompress',
      module_sha256: 'module-hash',
      identity: source,
      source_address: 0x80010000,
      allocation_size: 4096,
      load_vblank: '17',
      components: [],
    };
    const sceneOwner = owner({ snapshot: { revision: 0 } });
    sceneOwner.snapshot = { ...originalSnapshot, draft: -45 };
    testDoubles.openScene
      .mockImplementationOnce(() => { throw backendRequirement; })
      .mockReturnValueOnce(sceneOwner);
    testDoubles.controller.getSnapshot.mockReturnValue({ connected: true, error: null, schema: { identity: { module: 'module-hash' } } });
    testDoubles.controller.command.mockResolvedValue(context);
    testDoubles.useSyncExternalStore.mockReturnValue({
      connected: true, busy: 0, running: false, error: null, revision: 0,
      schema: { identity: { module: 'module-hash' } }, status: null, snapshots: [], kind: 'browser',
    });
    hook.registerSource(registerLiveSourceHandler(vi.fn())!);
    const disc = { mapBundle: vi.fn(async () => ({ placeholder: false, bytes: [1, 2, 3], source })), mapTextures: vi.fn(async () => mapTextures()) };

    await expect(hook.openScene(23, disc as never)).resolves.toMatchObject({ draft: -45, revision: 0 });
    expect(testDoubles.controller.command).toHaveBeenCalledOnce();
    expect(testDoubles.controller.command).toHaveBeenCalledWith({ cmd: 'field_source_context', map: 23, source_sha256: source.source_sha256 });
    expect(testDoubles.openScene).toHaveBeenCalledTimes(2);
    expect(testDoubles.openScene.mock.calls[1]?.[1]).toMatchObject({
      bundle: [1, 2, 3],
      source,
      context,
      module_sha256: context.module_sha256,
    });
  });

  it('opens Scene without a runtime-context call when the source is already sufficient', async () => {
    const { hook, original } = makeDraft();
    const sceneOwner = owner({ snapshot: { revision: 0 } });
    sceneOwner.snapshot = { ...originalSnapshot, draft: -44 };
    testDoubles.openScene.mockReturnValue(sceneOwner);
    const sourceContextHandler = vi.fn(async (): Promise<SourceContext> => {
      throw new Error('runtime context should not be requested');
    });
    hook.registerSource(sourceContextHandler);
    const disc = { mapBundle: vi.fn(async () => ({ placeholder: false, bytes: [1, 2, 3], source })), mapTextures: vi.fn(async () => mapTextures()) };

    await expect(hook.openScene(23, disc as never)).resolves.toMatchObject({ draft: -44, revision: 0 });
    expect(sourceContextHandler).not.toHaveBeenCalled();
    expect(testDoubles.openScene).toHaveBeenCalledTimes(1);
    expect(testDoubles.decodeSceneAssets).toHaveBeenCalledTimes(1);
    expect(original.close).toHaveBeenCalledOnce();
  });

  it('does not convert or retry an unrelated backend failure', async () => {
    const { hook, original, report } = makeDraft();
    const unrelated = new Error('malformed source bundle');
    testDoubles.openScene.mockImplementation(() => { throw unrelated; });
    const sourceContextHandler = vi.fn(async (): Promise<SourceContext> => {
      throw new Error('must not call runtime for unrelated error');
    });
    hook.registerSource(sourceContextHandler);
    const disc = { mapBundle: vi.fn(async () => ({ placeholder: false, bytes: [1, 2, 3], source })), mapTextures: vi.fn(async () => mapTextures()) };

    await expect(hook.openScene(23, disc as never)).rejects.toBe(unrelated);
    expect(sourceContextHandler).not.toHaveBeenCalled();
    expect(testDoubles.openScene).toHaveBeenCalledTimes(1);
    expect(original.close).not.toHaveBeenCalled();
    expect(report).toHaveBeenCalledWith(unrelated);
  });
});
