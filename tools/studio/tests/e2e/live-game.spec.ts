import { expect, test, type Page } from '@playwright/test';

const syntheticGamePath = '/__synthetic_live_game';
const moduleId = 'a'.repeat(64);

// UI and adapter regression fixture only; this does not exercise a real game runtime.
async function setupSyntheticGame(page: Page) {
  await page.context().route(`**${syntheticGamePath}`, route => route.fulfill({
    status: 200,
    contentType: 'text/html',
    body: `<!doctype html><html><head><title>Synthetic game fixture</title></head><body>Synthetic game fixture
      <script>(() => {
        const id = ${JSON.stringify(moduleId)};
        const identity = { module: id, disc: 'synthetic-disc', packages: [] };
        const schema = {
          module_sha256: id,
          globals: { fixture_value: { address: 0, type: 0 }, fixture_record: { address: 4, type: 1 } },
          types: [
            { kind: 'base', name: 'u32', size: 4, encoding: 'unsigned' },
            { kind: 'struct', name: 'FixtureRecord', size: 4, members: [{ name: 'value', offset: 0, type: 0 }] },
          ],
          records: {},
        };
        const state = { running: false, frames: 0, value: 7, snapshots: {} };
        const runtimeStatus = () => ({ identity, mode: 1, disc: 1, frames: state.frames });
        const commandJson = json => {
          const command = JSON.parse(json);
          if (command.cmd === 'schema') return JSON.stringify({ identity, schema });
          if (command.cmd === 'status') return JSON.stringify(runtimeStatus());
          if (command.cmd === 'inspect' && ['fixture_value', 'fixture_record.value'].includes(command.path)) return JSON.stringify(state.value);
          if (command.cmd === 'write' && ['fixture_value', 'fixture_record.value'].includes(command.path)) {
            if (command.value === 13) return JSON.stringify({ error: 'Synthetic runtime refused scalar value 13' });
            state.value = command.value ?? state.value;
            return JSON.stringify({ ok: true });
          }
          if (command.cmd === 'snapshot' && command.name) {
            state.snapshots[command.name] = state.value;
            return JSON.stringify({ ok: true });
          }
          if (command.cmd === 'restore' && command.name && command.name in state.snapshots) {
            state.value = state.snapshots[command.name];
            return JSON.stringify({ ok: true });
          }
          return JSON.stringify({ error: 'Synthetic runtime does not support ' + (command.cmd ?? 'missing command') });
        };
        window.__syntheticLiveGame = state;
        window.xem = {
          version: 4,
          cancellableRun: true,
          status: () => ({ running: state.running, session: { frames: state.frames }, page: { sessionGeneration: 1 } }),
          setRunning: running => { state.running = running; },
          commandJson,
          run: async (command, options) => {
            if (options?.signal?.aborted) throw options.signal.reason;
            state.frames += command.count;
            return runtimeStatus();
          },
        };
      })();</script></body></html>`,
  }));
}

async function openStudio(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  const live = page.locator('[data-studio-panel="live"]');
  await expect(live).toBeVisible();
  return live;
}

async function attachSyntheticGame(page: Page, live = page.locator('[data-studio-panel="live"]')) {
  await live.getByLabel('Same-origin game page').fill(syntheticGamePath);
  const popupPromise = page.waitForEvent('popup');
  await live.getByRole('button', { name: 'Open game & attach' }).click();
  const popup = await popupPromise;
  await popup.waitForLoadState('domcontentloaded');
  await expect(popup).toHaveTitle('Synthetic game fixture');
  await expect.poll(() => popup.evaluate(() => typeof (window as Window & { xem?: unknown }).xem)).toBe('object');
  await expect(live.locator('> p[role="status"]')).toContainText('browser · Paused · acknowledged');
  return popup;
}

async function displayedFrames(live: ReturnType<Page['locator']>) {
  const text = await live.getByLabel('Live runtime status').textContent();
  const status = JSON.parse(text ?? 'null') as { frames?: number };
  return status.frames ?? -1;
}

async function storedErrors(errors: ReturnType<Page['locator']>) {
  return errors.locator('pre.editor-text--danger').allTextContents();
}

test('attaches with acknowledged pause and keeps F7 and Shift+F6 scoped to live controls', async ({ page }) => {
  await setupSyntheticGame(page);
  const live = await openStudio(page);
  await attachSyntheticGame(page, live);

  const step = live.getByRole('button', { name: 'Step' });
  await live.focus();
  await page.keyboard.press('F7');
  await expect.poll(() => displayedFrames(live)).toBe(1);
  await step.focus();
  await page.keyboard.press('F7');
  await expect.poll(() => displayedFrames(live)).toBe(2);

  const frameCount = live.getByLabel('Frames');
  await frameCount.focus();
  await page.keyboard.press('F7');
  await expect.poll(() => displayedFrames(live)).toBe(2);

  await live.focus();
  await page.keyboard.press('Shift+F6');
  await expect(live.locator('> p[role="status"]')).toContainText('Running · acknowledged');
  expect(await live.evaluate(element => element.contains(document.activeElement))).toBe(true);
  await page.keyboard.press('Shift+F6');
  await expect(live.locator('> p[role="status"]')).toContainText('Paused · acknowledged');
});

test('retains frame and runtime scalar errors through later successes and confirms snapshot restore', async ({ page }) => {
  await setupSyntheticGame(page);
  const live = await openStudio(page);
  await attachSyntheticGame(page, live);
  const errors = page.locator('.studio-errors');
  const count = live.getByLabel('Frames');
  const step = live.getByRole('button', { name: 'Step' });

  await count.fill('0');
  await step.click();
  await expect(errors).toContainText('Frame count must be an integer from 1 to 10000');
  const frameError = (await storedErrors(errors))[0]!;
  await count.fill('1');
  await step.click();
  await expect.poll(() => displayedFrames(live)).toBe(1);
  await expect.poll(() => storedErrors(errors)).toEqual([frameError]);

  await live.getByLabel('Snapshot name').fill('baseline');
  await live.getByRole('button', { name: 'Capture' }).click();
  const snapshots = live.getByLabel('Session snapshots');
  await expect(snapshots).toHaveValue('baseline');

  const inspector = page.locator('[data-studio-panel="inspector"]');
  // Expand while the inspector's details editor is empty, then select a child.
  await inspector.getByRole('button', { name: 'Expand fixture_record' }).click();
  const nestedValue = inspector.locator('[data-schema-node="fixture_record.value"]');
  await expect(nestedValue).toBeVisible();
  await nestedValue.click();
  await expect(inspector.getByLabel('Selected path')).toHaveValue('fixture_record.value');
  await expect(inspector.getByLabel('Inspected value')).toHaveText('7');
  const nestedInput = inspector.getByLabel('New scalar value');
  await nestedInput.fill('9');
  await nestedInput.press('Enter');
  await expect(inspector.getByRole('status')).toContainText('Read-back: 9');
  await expect(inspector.getByLabel('Inspected value')).toHaveText('9');

  const write = inspector.getByRole('button', { name: 'Write & read back' });
  await nestedInput.fill('8');
  await write.click();
  await expect(inspector.getByRole('status')).toContainText('Read-back: 8');

  await nestedInput.fill('13');
  await write.click();
  await expect(errors).toContainText('Synthetic runtime refused scalar value 13');
  const errorsAfterRefusal = await storedErrors(errors);
  expect(errorsAfterRefusal.some(text => text.includes('Synthetic runtime refused scalar value 13'))).toBe(true);
  await nestedInput.fill('8');
  await write.click();
  await expect(inspector.getByRole('status')).toContainText('Read-back: 8');
  await expect.poll(() => storedErrors(errors)).toEqual(errorsAfterRefusal);

  await snapshots.selectOption('baseline');
  const restore = live.getByRole('button', { name: 'Restore…' });
  await restore.click();
  const dialog = page.getByRole('dialog', { name: 'Restore session snapshot' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused();
  const beforeDialogStep = await displayedFrames(live);
  await page.keyboard.press('F7');
  await expect.poll(() => displayedFrames(live)).toBe(beforeDialogStep);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(restore).toBeFocused();

  await restore.click();
  const confirmed = page.getByRole('dialog', { name: 'Restore session snapshot' });
  await confirmed.getByRole('button', { name: 'Restore', exact: true }).click();
  await expect(confirmed).toHaveCount(0);
  await inspector.getByRole('button', { name: 'Refresh value' }).click();
  await expect(inspector.getByLabel('Inspected value')).toHaveText('7');
  await expect.poll(() => storedErrors(errors)).toEqual(errorsAfterRefusal);
});

test('keeps the schema tree selectable after opening details at supported widths', async ({ page }, testInfo) => {
  await setupSyntheticGame(page);
  const live = await openStudio(page);
  await attachSyntheticGame(page, live);
  const inspector = page.locator('[data-studio-panel="inspector"]');
  await inspector.getByRole('button', { name: 'Expand fixture_record' }).click();
  await inspector.locator('[data-schema-node="fixture_record.value"]').click();
  await expect(inspector.getByLabel('Selected path')).toHaveValue('fixture_record.value');

  const root = inspector.locator('[data-schema-node="fixture_value"]');
  const child = inspector.locator('[data-schema-node="fixture_record.value"]');
  const treeSlot = inspector.locator('.docker-list-layout.schema-list');
  const treeViewport = inspector.locator('.docker-item-list');
  const expectReadableRow = async (node: typeof root) => {
    const textBoxes = await node.locator('span').evaluateAll(spans => spans.map(span => {
      const rect = span.getBoundingClientRect();
      return { top: rect.top, bottom: rect.bottom };
    }));
    const rowBounds = await node.locator('xpath=..').boundingBox();
    expect(textBoxes).toHaveLength(2);
    expect(textBoxes[1]!.top).toBeGreaterThanOrEqual(textBoxes[0]!.bottom);
    expect(textBoxes[0]!.top).toBeGreaterThanOrEqual(rowBounds?.y ?? Infinity);
    expect(textBoxes[1]!.bottom).toBeLessThanOrEqual((rowBounds?.y ?? 0) + (rowBounds?.height ?? 0));
  };
  for (const width of [1440, 1180]) {
    await page.setViewportSize({ width, height: width === 1440 ? 900 : 720 });
    await root.scrollIntoViewIfNeeded();
    await expect(root).toBeVisible();
    await expectReadableRow(root);
    const slotBounds = await treeSlot.boundingBox();
    const viewportBounds = await treeViewport.boundingBox();
    const rowBounds = await root.boundingBox();
    expect(slotBounds?.height).toBeGreaterThanOrEqual(180);
    expect(viewportBounds?.height).toBeGreaterThan(32);
    expect(rowBounds?.y).toBeGreaterThanOrEqual(viewportBounds?.y ?? Infinity);
    expect((rowBounds?.y ?? -Infinity) + (rowBounds?.height ?? 0)).toBeLessThanOrEqual((viewportBounds?.y ?? 0) + (viewportBounds?.height ?? 0));
    expect((rowBounds?.y ?? 0) + (rowBounds?.height ?? 0)).toBeLessThanOrEqual(width === 1440 ? 900 : 720);
    await root.click();
    await expect(inspector.getByLabel('Selected path')).toHaveValue('fixture_value');
    await expect(inspector.getByLabel('Inspected value')).toHaveText('7');
    await child.scrollIntoViewIfNeeded();
    await expectReadableRow(child);
    await child.click();
    await expect(inspector.getByLabel('Selected path')).toHaveValue('fixture_record.value');
    await root.focus();
    await page.keyboard.press('Enter');
    await expect(inspector.getByLabel('Selected path')).toHaveValue('fixture_value');
    if (width === 1440) await page.screenshot({ path: testInfo.outputPath('schema-tree-1440x900.png') });
  }
  await page.screenshot({ path: testInfo.outputPath('schema-tree-1180x720.png') });
});

test('window loss disconnects with its original error and a later attach leaves that error visible', async ({ page }) => {
  await setupSyntheticGame(page);
  const live = await openStudio(page);
  const popup = await attachSyntheticGame(page, live);
  await popup.close();

  await expect(live.locator('> p[role="status"]')).toHaveText('Disconnected', { timeout: 5_000 });
  const errors = page.locator('.studio-errors');
  await expect(errors).toContainText('The attached game window was closed');
  const lostError = (await storedErrors(errors))[0]!;

  await attachSyntheticGame(page, live);
  await expect.poll(() => storedErrors(errors)).toEqual([lostError]);
});
