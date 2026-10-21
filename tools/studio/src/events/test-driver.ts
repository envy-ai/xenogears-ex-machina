import { nativeHost } from '../platform/host';

type View = 'list' | 'graph' | 'lanes';
interface Snapshot {
  view: View;
  selection: { actor: number; event: number; pc: number | null };
  map: number | null;
  instructions: number;
  discs: { id: number; disc: number | null | undefined; mapCount: number; placeholders: number }[];
  renderTimes: { view: View; ms: number; actor: number; event: number; map: number; instructions: number }[];
}
interface TestHook {
  openNativePath(path: string): Promise<void>;
  selectMap(discId: number, map: number): Promise<void>;
  setView(view: View): Promise<void>;
  snapshot(): Snapshot;
}
interface Config { paths: string[]; output: string; map?: number; disc?: number; measure?: boolean }

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
