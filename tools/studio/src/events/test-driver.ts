import { nativeHost } from '../platform/host';
import type { CompiledScene, SceneDocument, SceneSnapshot } from '../backend/scenes';

type View = 'list' | 'graph' | 'lanes' | 'scene';
interface Snapshot {
  view: View;
  selection: { actor: number; event: number; pc: number | null };
  map: number | null;
  instructions: number;
  scene: SceneSnapshot | null;
  discs: { id: number; disc: number | null | undefined; mapCount: number; placeholders: number }[];
  renderTimes: { view: View; ms: number; actor: number; event: number; map: number; instructions: number }[];
}
interface TestHook {
  openNativePath(path: string): Promise<void>;
  selectMap(discId: number, map: number): Promise<void>;
  setView(view: View): Promise<void>;
  snapshot(): Snapshot;
  inspectScene(): { owner: number | null; document: SceneDocument | null; compiled: CompiledScene | null };
}
interface Config {
  paths: string[]; output: string; map?: number; disc?: number; measure?: boolean;
  scene?: { folder: string; seedProject: string; seedProjectSha256: string; actorId: string; placementNode: string; expectedSourceSha256: string; sourceHeaderX: number; initialX: number; editedX: number };
}

function safeHash(value: unknown): string {
  const crypto = (nativeHost as unknown as { require(id: 'node:crypto'): typeof import('node:crypto') }).require('node:crypto');
  return crypto.createHash('sha256').update(JSON.stringify(value) ?? '').digest('hex');
}

function safeBytesHash(value: Uint8Array): string {
  const crypto = (nativeHost as unknown as { require(id: 'node:crypto'): typeof import('node:crypto') }).require('node:crypto');
  return crypto.createHash('sha256').update(value).digest('hex');
}

function clickButton(label: string): void {
  const button = [...document.querySelectorAll<HTMLButtonElement>('button')]
    .find(candidate => candidate.textContent?.trim() === label || candidate.getAttribute('aria-label') === label);
  if (!button || button.disabled) throw new Error(`Expected enabled Scene NW button: ${label}`);
  button.click();
}

async function waitForEnabledButton(label: string): Promise<void> {
  await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  await waitForDocument(() => [...document.querySelectorAll<HTMLButtonElement>('button')]
    .some(candidate => (candidate.textContent?.trim() === label || candidate.getAttribute('aria-label') === label)
      && !candidate.disabled), `the enabled ${label} button`);
}

function setInput(label: string, value: string): void {
  const input = document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
  if (!input || input.disabled) throw new Error(`Expected enabled Scene NW field: ${label}`);
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  if (!setter) throw new Error('The NW input value setter is unavailable');
  setter.call(input, value);
  input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: value }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
}

function setLabeledInput(labelText: string, value: string): void {
  const label = [...document.querySelectorAll<HTMLLabelElement>('label')]
    .find(candidate => candidate.textContent?.trim().startsWith(labelText));
  const input = label?.querySelector<HTMLInputElement>('input');
  if (!input || input.disabled) throw new Error(`Expected enabled NW text input: ${labelText}`);
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  if (!setter) throw new Error('The NW input value setter is unavailable');
  setter.call(input, value);
  input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: value }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
}

function clickTab(label: string): void {
  const tab = [...document.querySelectorAll<HTMLButtonElement>('button[role="tab"]')]
    .find(candidate => candidate.textContent?.trim() === label);
  if (!tab || tab.disabled) throw new Error(`Expected enabled NW tab: ${label}`);
  tab.click();
}

async function waitForDocument(predicate: () => boolean, label: string): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (!predicate()) {
    const sceneError = document.querySelector<HTMLElement>('.scene-canvas__error [role="alert"]')?.textContent?.trim();
    if (sceneError) throw new Error(`The Scene viewport reported an error while waiting for ${label}: ${sceneError}`);
    if (Date.now() >= deadline) throw new Error(`Timed out waiting for ${label}`);
    await new Promise(resolve => setTimeout(resolve, 25));
  }
}

async function chooseProjectFolder(path: string): Promise<void> {
  const originalClick = HTMLInputElement.prototype.click;
  let intercepted = false;
  HTMLInputElement.prototype.click = function click(this: HTMLInputElement): void {
    if (!intercepted && this.type === 'file' && this.hasAttribute('nwdirectory')) {
      intercepted = true;
      Object.defineProperty(this, 'value', { configurable: true, value: path });
      this.dispatchEvent(new Event('change', { bubbles: true }));
      return;
    }
    originalClick.call(this);
  };
  try {
    clickButton('Open folder…');
    const folderLabel = `${path.split(/[\\/]/).at(-1)}/`;
    await waitForDocument(() => [...document.querySelectorAll<HTMLElement>('.studio-project .studio-toolbar span')]
      .some(label => label.textContent?.trim() === folderLabel), 'the actual NodeFilesAdapter folder to open');
    if (!intercepted) throw new Error('The normal NW folder chooser was not intercepted');
  } finally {
    HTMLInputElement.prototype.click = originalClick;
  }
}

async function runSceneAcceptance(hook: TestHook, discPaths: string[], config: NonNullable<Config['scene']>, fs: typeof import('node:fs/promises'), path: typeof import('node:path')) {
  const root = path.resolve(path.dirname(config.folder));
  const folder = path.resolve(config.folder);
  const relative = path.relative(root, folder);
  if (relative.startsWith('..') || path.isAbsolute(relative) || relative === '') throw new Error('The NW project folder must be a fresh child of the smoke artifact root');
  const seedProjectBytes = await fs.readFile(config.seedProject);
  if (safeBytesHash(seedProjectBytes) !== config.seedProjectSha256) throw new Error('The checkpointed original Scene project does not match its recorded identity');
  await fs.mkdir(folder, { recursive: false });
  await fs.writeFile(path.join(folder, 'names.json'), `${JSON.stringify({ schema_version: 1, variables: {}, maps: {}, messages: {} })}\n`, { flag: 'wx' });
  await fs.writeFile(path.join(folder, 'field-map-23.json'), seedProjectBytes, { flag: 'wx' });

  for (const discPath of discPaths) {
    const expectedCount = hook.snapshot().discs.length + 1;
    setLabeledInput('Disc image path', discPath);
    await waitForEnabledButton('Open disc path');
    clickButton('Open disc path');
    await waitForSnapshot(hook, () => hook.snapshot().discs.length >= expectedCount, 'actual original-disc reader to open');
  }
  const discOne = [...document.querySelectorAll<HTMLButtonElement>('.event-disc-row button')]
    .find(button => button.textContent?.trim().startsWith('Disc 1 ·'));
  if (!discOne) throw new Error('The actual Disc 1 reader did not open');
  discOne.click();
  setLabeledInput('Filter maps', '23');
  await waitForDocument(() => Boolean(document.querySelector<HTMLButtonElement>('.event-map-row[data-map="23"]')), 'actual map 23 row');
  const mapRow = document.querySelector<HTMLButtonElement>('.event-map-row[data-map="23"]');
  if (!mapRow) throw new Error('Actual map 23 row is unavailable');
  mapRow.click();
  await waitForSnapshot(hook, () => hook.snapshot().map === 23, 'actual Disc 1 map 23 selection');
  const selectedDisc = [...document.querySelectorAll<HTMLButtonElement>('.event-disc-row button[aria-pressed="true"]')]
    .find(button => button.textContent?.trim().startsWith('Disc 1 ·'));
  if (!selectedDisc) throw new Error('Disc 1 is not the active source for map 23');
  await chooseProjectFolder(folder);
  const ownerBeforeSeedLoad = hook.inspectScene().owner;
  await waitForEnabledButton('Load project');
  clickButton('Load project');
  await waitForDocument(() => hook.inspectScene().owner !== ownerBeforeSeedLoad, 'normal Load project to adopt the seeded Scene document');
  clickTab('Scene');
  await waitForSnapshot(hook, () => hook.snapshot().view === 'scene' && hook.snapshot().scene !== null, 'the normal Scene tab to open the saved source');
  await waitForDocument(() => Boolean(document.querySelector('canvas[aria-label="3D field scene viewport"]')),
    'the actual WebGL Scene viewport');

  const initial = hook.snapshot().scene;
  if (!initial || initial.source_sha256 !== config.expectedSourceSha256) throw new Error('Scene did not decode the expected original Disc 1 map 23 source identity');
  const actor = initial.actors.find(item => item.id === config.actorId);
  if (!actor) throw new Error(`The qualified source actor ${config.actorId} is absent`);
  const descriptor = initial.descriptors.find(item => item.id === actor.id);
  if (!descriptor) throw new Error('The qualified Scene actor has no source descriptor');
  const originalHeaderX = descriptor.value.position[0];
  if (originalHeaderX !== config.sourceHeaderX) throw new Error('The checkpointed project descriptor source word changed unexpectedly');

  const treeRow = `[data-scene-id="${config.actorId}"]`;
  const filter = document.querySelector<HTMLInputElement>('[data-scene-filter]');
  if (filter) {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    setter?.call(filter, config.actorId);
    filter.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: config.actorId }));
  }
  await waitForDocument(() => Boolean(document.querySelector(treeRow)), `the qualified Scene actor row ${config.actorId}`);
  document.querySelector<HTMLButtonElement>(treeRow)?.click();
  await waitForDocument(() => [...document.querySelectorAll('label')].some(label => label.textContent?.includes('Authored placement source')
    && label.querySelector('select') !== null), 'the actor placement selector');
  const sourceLabel = [...document.querySelectorAll('label')].find(label => label.textContent?.includes('Authored placement source'));
  const selector = sourceLabel?.querySelector<HTMLSelectElement>('select') ?? null;
  if (!selector || ![...selector.options].some(option => option.value === config.placementNode)) throw new Error(`The qualified source placement ${config.placementNode} is unavailable`);
  const selectSetter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;
  selectSetter?.call(selector, config.placementNode);
  selector.dispatchEvent(new Event('change', { bubbles: true }));
  await waitForDocument(() => document.querySelector<HTMLInputElement>('input[aria-label="Scene position X"]')?.value === String(config.initialX), 'the bound authored X field');

  const beforeOwner = hook.inspectScene().owner;
  const beforeScene = hook.snapshot().scene;
  if (!beforeScene || beforeOwner === null) throw new Error('The Scene owner was unavailable before the Node project save');
  if (descriptor.value.position[0] !== originalHeaderX) throw new Error('The source descriptor changed while loading the saved Scene project');
  setInput('Scene position X', String(config.editedX));
  await waitForEnabledButton('Apply scene fields');
  clickButton('Apply scene fields');
  await waitForSnapshot(hook, () => hook.snapshot().scene?.revision === beforeScene.revision + 1, 'the UI-authored Scene change');
  const authored = hook.snapshot().scene!;
  const authoredDescriptor = authored.descriptors.find(item => item.id === config.actorId);
  const authoredPlacement = document.querySelector<HTMLInputElement>('input[aria-label="Scene position X"]');
  if (authoredPlacement?.value !== String(config.editedX)) throw new Error('The Scene placement field did not retain the requested source coordinate');
  if (authoredDescriptor?.value.position[0] !== originalHeaderX) throw new Error('Editing the script-owned placement changed the unrelated source descriptor');
  clickButton('Undo');
  await waitForDocument(() => document.querySelector<HTMLInputElement>('input[aria-label="Scene position X"]')?.value === String(config.initialX), 'NW Scene undo to the saved placement source');
  clickButton('Redo');
  await waitForDocument(() => document.querySelector<HTMLInputElement>('input[aria-label="Scene position X"]')?.value === String(config.editedX), 'NW Scene redo to the edited placement source');
  const authoredOwner = hook.inspectScene();
  const authoredDocumentHash = await safeHash(authoredOwner.document);
  const authoredBundleHash = authoredOwner.compiled?.bundle_sha256;
  if (!authoredBundleHash) throw new Error('Scene compilation did not produce a bundle identity');

  clickButton('Save project');
  const projectPath = path.join(folder, 'field-map-23.json');
  let projectBytes: Buffer | null = null;
  const saveDeadline = Date.now() + 120_000;
  while (projectBytes === null && Date.now() < saveDeadline) {
    const candidate = await fs.readFile(projectPath);
    try {
      const candidateProject = JSON.parse(candidate.toString('utf8')) as { document?: SceneDocument };
      if (candidateProject.document && safeHash(candidateProject.document) === authoredDocumentHash) projectBytes = candidate;
    } catch {
      // Wait for the actual NodeFilesAdapter write to finish before accepting its document.
    }
    if (projectBytes === null) await new Promise(resolve => setTimeout(resolve, 25));
  }
  if (!projectBytes) throw new Error('The NodeFilesAdapter did not write the accepted Scene document before the save deadline');
  const projectHash = safeBytesHash(projectBytes);
  const savedProject = JSON.parse(projectBytes.toString('utf8')) as { schema_version: number; map: number; disc: number; document: SceneDocument };
  if (savedProject.schema_version !== 2 || savedProject.map !== 23 || savedProject.disc !== 1) throw new Error('The saved Node project identity does not match Disc 1 map 23');
  if (safeHash(savedProject.document) !== authoredDocumentHash) throw new Error('The Node-written project document differs from the accepted Scene owner');

  const beforeReloadOwner = hook.inspectScene().owner;
  await waitForEnabledButton('Load project');
  clickButton('Load project');
  await waitForDocument(() => hook.inspectScene().owner !== beforeReloadOwner, 'normal Load project to re-adopt the saved Scene document from Node files');
  const loaded = hook.inspectScene();
  if (loaded.owner === beforeReloadOwner || safeHash(loaded.document) !== authoredDocumentHash
    || loaded.compiled?.bundle_sha256 !== authoredBundleHash) throw new Error('Node project reload did not restore the saved Scene owner and compiled source');
  const loadedScene = hook.snapshot().scene;
  const loadedPlacement = document.querySelector<HTMLInputElement>('input[aria-label="Scene position X"]');
  if (loadedPlacement?.value !== String(config.editedX)) {
    throw new Error('Node project reload did not restore the saved source coordinate');
  }
  const finalOwner = loaded.owner;
  const source = loadedScene?.source_sha256;
  const identity = loaded.document?.input.source;
  const canvas = document.querySelector<HTMLCanvasElement>('canvas[aria-label="3D field scene viewport"]');
  const gl = canvas?.getContext('webgl2') ?? canvas?.getContext('webgl');
  if (!canvas || !gl || gl.isContextLost()) throw new Error('The actual NW Scene canvas has no live WebGL context');
  await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  if (document.querySelector('.scene-canvas__error')) throw new Error('The actual NW Scene canvas reported a local rendering error');
  const rendererInfo = gl.getExtension('WEBGL_debug_renderer_info') as { UNMASKED_RENDERER_WEBGL: number } | null;
  const renderer = String(gl.getParameter(rendererInfo?.UNMASKED_RENDERER_WEBGL ?? gl.RENDERER));
  const webglVersion = String(gl.getParameter(gl.VERSION));
  if (!renderer || !webglVersion) throw new Error('The actual NW Scene canvas did not report its WebGL renderer and version');
  return {
    adapter: 'node', disc: 1, map: 23, source_sha256: source,
    source_identity: identity ? {
      disc: identity.disc, map: identity.map, slot: identity.slot, lba: identity.lba,
      indexed_size: identity.indexed_size, boot_sha256: identity.boot_sha256,
    } : null,
    module_sha256: loaded.document?.input.module_sha256 ?? null,
    actor_id: config.actorId, placement_node: config.placementNode,
    source_header_x: originalHeaderX, saved_x: config.editedX, owner_changed_on_reload: finalOwner !== authoredOwner.owner,
    seed_project_sha256: config.seedProjectSha256, saved_project_sha256: projectHash, saved_document_sha256: authoredDocumentHash,
    compiled_bundle_sha256: authoredBundleHash, saved_actions: savedProject.document.actions.length,
    canvas: { width: canvas.width, height: canvas.height, webgl: true, renderer, version: webglVersion },
    filesystem: (await fs.readdir(folder)).sort(),
    chooser: 'normal Open folder button; only NW nwdirectory chooser selection intercepted; actual NodeFilesAdapter read/write used',
  };
}

async function waitForSnapshot(hook: TestHook, predicate: () => boolean, label: string): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error(`Timed out waiting for ${label}.`);
    await new Promise(resolve => setTimeout(resolve, 25));
  }
}

function mapRows() {
  return [...document.querySelectorAll<HTMLButtonElement>('.event-map-row')].flatMap(button => {
    const map = Number(button.dataset.map);
    if (!Number.isSafeInteger(map)) return [];
    return [{ map, placeholder: button.textContent?.includes('Placeholder') ?? false }];
  });
}

/** NW-only, opt-in test driver. It writes summaries and never reads disc bytes. */
export async function runEventsTest(hook: TestHook): Promise<void> {
  if (!nativeHost) throw new Error('The event smoke driver requires the NW.js host.');
  const fs = nativeHost.require('node:fs/promises');
  const path = nativeHost.require('node:path');
  const argument = nativeHost.App.argv.find(value => value.startsWith('--studio-events-test='));
  if (!argument) throw new Error('Missing --studio-events-test config path.');
  const configPath = argument.slice('--studio-events-test='.length);
  let outputPath: string | undefined;
  try {
    const config = JSON.parse(await fs.readFile(configPath, 'utf8')) as Config;
    if (!Array.isArray(config.paths) || config.paths.length === 0 || config.paths.some(value => typeof value !== 'string' || value.length === 0)) {
      throw new Error('The event smoke config requires one or more disc paths.');
    }
    if (typeof config.output !== 'string' || config.output.length === 0) throw new Error('The event smoke config requires an output path.');
    outputPath = path.resolve(config.output);
    if (config.scene) {
      if (config.disc !== 1 || config.map !== 23) throw new Error('The Scene NW check requires the actual Disc 1 map 23 selection');
      const scene = await runSceneAcceptance(hook, config.paths, config.scene, fs, path);
      await fs.mkdir(path.dirname(outputPath), { recursive: true });
      await fs.writeFile(outputPath, `${JSON.stringify({ kind: 'scene', ...scene })}\n`, { flag: 'wx' });
      console.log(`NW Scene smoke passed: ${JSON.stringify(scene)}`);
      return;
    }
    for (const discPath of config.paths) {
      const expectedCount = hook.snapshot().discs.length + 1;
      await hook.openNativePath(discPath);
      await waitForSnapshot(hook, () => hook.snapshot().discs.length >= expectedCount, 'disc session state to commit');
    }
    const latest = hook.snapshot();
    const active = config.disc === undefined
      ? config.measure ? latest.discs.find(disc => disc.disc === 1) ?? latest.discs.at(-1) : latest.discs.at(-1)
      : latest.discs.find(disc => disc.disc === config.disc);
    if (!active) throw new Error('No disc session opened.');
    if (config.map !== undefined) {
      await hook.selectMap(active.id, config.map);
      await waitForSnapshot(hook, () => hook.snapshot().map === config.map, `map ${config.map} selection to commit`);
    }
    const selected = hook.snapshot();
    if (selected.map !== config.map && config.map !== undefined) throw new Error(`Requested map ${config.map} did not become active.`);
    if (config.measure && selected.instructions !== 2358) throw new Error(`Disc 1 map 2 should decode to 2358 instructions, received ${selected.instructions}.`);

    if (config.measure) {
      const assertRendered = (view: View) => {
        const panelName = view === 'list' ? 'Command list' : view === 'graph' ? 'Flow graph' : 'Actor lanes';
        const panel = document.querySelector<HTMLElement>(`[role="tabpanel"][aria-label="${panelName}"]`);
        if (!panel || panel.getBoundingClientRect().width === 0 || panel.getBoundingClientRect().height === 0) {
          throw new Error(`${panelName} did not become visible for first-render timing.`);
        }
        const contentCount = view === 'list' ? panel.querySelectorAll('.event-command-row').length
          : view === 'graph' ? panel.querySelectorAll('[data-graph-node]').length
            : panel.querySelectorAll('.event-lane-row').length;
        if (contentCount === 0) throw new Error(`${panelName} rendered no real content.`);
      };
      // The list is rendered while selecting the map, so retabbing to it would
      // measure a cached view. Its first-render record comes from map loading.
      if (hook.snapshot().view !== 'list') throw new Error('The command list was not the initial map view.');
      assertRendered('list');
      for (const view of ['graph', 'lanes'] as const) {
        await hook.setView(view);
        assertRendered(view);
      }
    }
    const snapshot = hook.snapshot();
    const renderTimes = config.measure ? ['list', 'graph', 'lanes'].map(view => {
      const record = snapshot.renderTimes.filter(item => item.map === config.map && item.view === view).at(-1);
      if (!record) throw new Error(`Missing first-render timing for ${view} on map ${config.map}.`);
      return record;
    }) : [];
    const result = {
      discs: snapshot.discs,
      map: snapshot.map,
      instructions: snapshot.instructions,
      mapRows: mapRows(),
      renderTimes,
      visibleViews: ['Command list', 'Flow graph', 'Actor lanes'].filter(name => {
        const tab = [...document.querySelectorAll('[role="tab"]')].find(item => item.textContent?.trim() === name);
        return Boolean(tab);
      }),
    };
    await fs.mkdir(path.dirname(outputPath), { recursive: true });
    await fs.writeFile(outputPath, `${JSON.stringify(result)}\n`, { flag: 'wx' });
  } catch (error) {
    if (outputPath) {
      const cause = error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : { message: String(error) };
      await fs.mkdir(path.dirname(outputPath), { recursive: true });
      try {
        await fs.writeFile(outputPath, `${JSON.stringify({ error: cause })}\n`, { flag: 'wx' });
      } catch (writeError) {
        console.error('Could not preserve the Studio events smoke failure artifact', writeError);
      }
    }
    console.error('Studio events smoke failed', error);
    process.exitCode = 1;
  } finally {
    nativeHost.App.quit();
  }
}
