import { expect, test, type Page } from '@playwright/test';
import { syntheticEventComponent } from './events.fixture';
import type { StudioTestSnapshot } from './studio-test-driver';

test.skip(process.env.VITE_XEM_STUDIO_TEST !== '1', 'Synthetic event journeys require the explicit test-only Studio driver.');

async function loadSyntheticComponent(page: Page): Promise<void> {
  const fixture = syntheticEventComponent();
  await page.evaluate(async ({ bytes, declaredSize }) => {
    const driver = window.__xemStudioTest;
    if (!driver) throw new Error('The explicit Studio test driver is unavailable.');
    await driver.loadComponent(bytes, declaredSize, 0);
  }, fixture);
}

async function snapshot(page: Page): Promise<StudioTestSnapshot> {
  return page.evaluate(() => {
    const driver = window.__xemStudioTest;
    if (!driver) throw new Error('The explicit Studio test driver is unavailable.');
    return driver.snapshot();
  });
}

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await loadSyntheticComponent(page);
});

test('renders the three event views and preserves selection while folding and navigating', async ({ page }, testInfo) => {
  const list = page.getByRole('region', { name: 'Command list' });
  await expect(list).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Command list' })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Flow graph' })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Actor lanes' })).toBeVisible();
  await expect(page.getByRole('list', { name: 'Event commands' })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('events-command-list.png'), fullPage: false });

  // The invented component contains a nested if, a request link, a variable
  // write and an outside successor. Folding hides child rows without losing
  // the graph nodes or diagnostics.
  const innerBranch = list.locator('.event-command-row[data-pc="9"]');
  await expect(innerBranch).toBeVisible();
  await list.getByRole('button', { name: 'Collapse If / else' }).first().click();
  await expect(innerBranch).toHaveCount(0);
  await list.getByRole('button', { name: 'Expand all' }).click();
  await expect(innerBranch).toBeVisible();
  await list.locator('.event-diagnostics summary').click();
  await expect(list.locator('.event-diagnostic-content')).toContainText('Outside stream');
  await expect(list.locator('.event-diagnostic-content')).toContainText('@01f4');

  await list.locator('.event-command-row[data-pc="9"] .docker-item-select').click();
  await page.getByRole('tab', { name: 'Flow graph' }).click();
  const graph = page.getByRole('region', { name: 'Flow graph' });
  await expect(graph).toBeVisible();
  await expect(graph.locator('[data-graph-node][data-selected="true"]')).toHaveCount(1);
  expect((await snapshot(page)).selection.pc).toBe(9);
  await page.screenshot({ path: testInfo.outputPath('events-flow-graph.png'), fullPage: false });

  await graph.locator('[data-graph-node]').first().click();
  const graphSelection = (await snapshot(page)).selection.pc;
  expect(graphSelection).not.toBeNull();
  await page.getByRole('tab', { name: 'Command list' }).click();
  await expect(list.locator(`.event-command-row[data-pc="${graphSelection}"] .docker-item-select[data-selected="true"]`)).toBeVisible();

  await page.getByRole('tab', { name: 'Actor lanes' }).click();
  await expect(page.getByRole('region', { name: 'Actor lanes' })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('events-actor-lanes.png'), fullPage: false });
  await page.keyboard.press('Control+1');
  await expect(page.getByRole('tab', { name: 'Command list' })).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Control+2');
  await expect(page.getByRole('tab', { name: 'Flow graph' })).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Control+3');
  await expect(page.getByRole('tab', { name: 'Actor lanes' })).toHaveAttribute('aria-selected', 'true');

  await page.getByRole('treeitem', { name: /01 · Idle/ }).click();
  const links = page.getByRole('list', { name: 'Event links' });
  await expect(links).toContainText('request event started');
  await links.getByRole('button', { name: /request event started/ }).click();
  expect((await snapshot(page)).selection).toEqual({ actor: 0, event: 1, pc: 31 });
  await page.getByRole('treeitem', { name: /02 · Talk/ }).click();
  const accesses = page.locator('.event-lane-accesses');
  await expect(accesses.locator('summary')).toContainText('1 variable accesses');
  await accesses.locator('summary').click();
  await expect(accesses).toContainText('write · Variable 2');
});

test('follows a synthetic branch with Enter and returns through Alt-arrow history', async ({ page }) => {
  const before = await snapshot(page);
  expect(before.selection.pc).toBe(1);
  await page.getByRole('list', { name: 'Event commands' }).focus();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await snapshot(page)).selection.pc).toBe(29);
  await page.keyboard.press('Alt+ArrowLeft');
  await expect.poll(async () => (await snapshot(page)).selection.pc).toBe(1);
  await page.keyboard.press('Alt+ArrowRight');
  await expect.poll(async () => (await snapshot(page)).selection.pc).toBe(29);
});

test('renames a variable through the names sidecar and reloads it from the folder adapter', async ({ page }) => {
  await page.addInitScript(() => {
    const key = 'xem-studio-event-names';
    const initial = JSON.stringify({ schema_version: 1, variables: {}, maps: {}, messages: {} });
    if (localStorage.getItem(key) === null) localStorage.setItem(key, initial);
    const names = () => JSON.parse(localStorage.getItem(key) ?? initial) as unknown;
    const fileHandle = {
      name: 'names.json',
      kind: 'file',
      async getFile() { return new File([JSON.stringify(names())], 'names.json'); },
      async createWritable() {
        let pending = '';
        return {
          async write(data: Uint8Array) { pending = new TextDecoder().decode(data); },
          async close() { localStorage.setItem(key, pending); },
          async abort() {},
        };
      },
    };
    const directory = {
      name: 'event-project',
      kind: 'directory',
      async *values() { yield fileHandle; },
      async getDirectoryHandle() { throw new Error('No subdirectory in synthetic project'); },
      async getFileHandle(name: string) {
        if (name !== 'names.json') throw new Error(`No synthetic file: ${name}`);
        return fileHandle;
      },
    };
    (window as typeof window & { showDirectoryPicker: () => Promise<typeof directory> }).showDirectoryPicker = async () => directory;
  });
  await page.reload();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await loadSyntheticComponent(page);
  await page.getByRole('button', { name: 'Open folder…' }).click();
  await expect(page.locator('[data-panel-id="project"]')).toContainText('names.json');
  await page.keyboard.press('Control+f');
  const find = page.getByLabel('Find within map');
  await expect(find).toBeFocused();
  await find.fill('set variable one');
  await find.press('Enter');
  await expect.poll(async () => (await snapshot(page)).selection).toEqual({ actor: 0, event: 2, pc: 35 });
  await expect(page.getByRole('list', { name: 'Event commands' })).toContainText('Variable 2');
  await page.keyboard.press('F2');
  const dialog = page.getByRole('dialog', { name: 'Names' });
  await expect(dialog).toBeVisible();
  const closeNames = dialog.getByRole('button', { name: 'Close names' });
  await closeNames.focus();
  await page.keyboard.press('Tab');
  await expect(dialog.getByLabel('Name scope')).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(closeNames).toBeFocused();
  await dialog.getByLabel('Name scope').selectOption('variable');
  await dialog.getByRole('textbox', { name: 'Variable slot' }).fill('2');
  await dialog.getByRole('textbox', { name: 'Name', exact: true }).fill('Hero HP');
  await dialog.getByRole('button', { name: 'Save name' }).click();
  await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem('xem-studio-event-names') ?? '{}').variables?.['2']))
    .toBe('Hero HP');
  await dialog.getByRole('button', { name: 'Close names' }).click();
  await expect(find).toBeFocused();
  await expect(page.getByRole('list', { name: 'Event commands' })).toContainText('Hero HP');

  await page.reload();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await loadSyntheticComponent(page);
  await page.getByRole('button', { name: 'Open folder…' }).click();
  await expect(page.locator('[data-panel-id="project"]')).toContainText('names.json');
  await page.getByRole('treeitem', { name: /02 · Talk/ }).click();
  await expect(page.getByRole('list', { name: 'Event commands' })).toContainText('Hero HP');
});
