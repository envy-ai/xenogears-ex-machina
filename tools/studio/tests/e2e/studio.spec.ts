import { expect, test, type Page } from '@playwright/test';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const artifactsDirectory = process.env.XEM_STUDIO_PLAYWRIGHT_ARTIFACTS
  ? resolve(process.env.XEM_STUDIO_PLAYWRIGHT_ARTIFACTS)
  : resolve(dirname(fileURLToPath(import.meta.url)), '../../../../tmp/studio');

async function expectResponseToFillBackend(page: Page) {
  const bounds = await page.locator('.studio-backend').evaluate((backend) => {
    const command = backend.querySelector('.studio-command')!.getBoundingClientRect();
    const response = backend.querySelector('.studio-response')!.getBoundingClientRect();
    return { commandWidth: command.width, responseWidth: response.width };
  });
  expect(bounds.responseWidth, 'response should span the backend content width')
    .toBeGreaterThanOrEqual(bounds.commandWidth - 24);
}

async function sendEcho(page: Page, value: unknown) {
  const input = page.getByLabel('Command (JSON)');
  await input.fill(JSON.stringify({ op: 'echo', args: value }));
  await page.getByRole('button', { name: 'Send command' }).click();
  const response = page.getByLabel('Backend response');
  await expect.poll(async () => {
    const text = await response.textContent();
    if (text === null || text === 'Send a command to see its reply.') return undefined;
    return JSON.parse(text);
  }).toEqual(value);
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    let content = 'fixture from the browser directory handle';
    const fileHandle = {
      name: 'fixture.txt',
      kind: 'file',
      async getFile() { return new File([content], 'fixture.txt'); },
      async createWritable() {
        let pending = content;
        return {
          async write(data: Uint8Array) { pending = new TextDecoder().decode(data); },
          async close() { content = pending; },
          async abort() { pending = content; },
        };
      },
    };
    const directoryHandle = {
      name: 'browser-fixture',
      kind: 'directory',
      async *values() { yield fileHandle; },
      async getDirectoryHandle() { throw new Error('No subdirectory in fixture'); },
      async getFileHandle(name: string) {
        if (name !== fileHandle.name) throw new Error(`No file: ${name}`);
        return fileHandle;
      },
    };
    const browserWindow = window as typeof window & {
      showDirectoryPicker: () => Promise<typeof directoryHandle>;
    };
    browserWindow.showDirectoryPicker = async () => directoryHandle;
  });
  const response = await page.goto('/');
  const pageUrl = new URL(page.url());
  expect(pageUrl.protocol).toBe('http:');
  expect(pageUrl.hostname).toBe('localhost');
  expect(response?.headers()['strict-transport-security']).toBeUndefined();
  await expect.poll(() => page.evaluate(() => window.isSecureContext)).toBe(true);
  console.log(`localhost HTTP checks: secureContext=true; Strict-Transport-Security=${response?.headers()['strict-transport-security'] ?? 'absent'}`);
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await sendEcho(page, { from: 'playwright', nested: [null, '🐉'] });
});

test('shows all workspace panels and the loaded wasm backend version', async ({ page }) => {
  await expect(page.locator('[data-panel-id="project"]')).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toBeVisible();
  await expect(page.locator('[data-panel-id="log"]')).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText(
    'Operations: version, echo, field.decode, field.encode, disc.open, disc.poll, disc.missing, disc.close, disc.maps, field.map_events, field.structure, names.validate, field.lanes, disc.cue',
  );
  await expectResponseToFillBackend(page);
  await page.screenshot({ path: resolve(artifactsDirectory, 'shell-1440x900.png') });
});

test('keeps all three panels readable at 1180x720', async ({ page }) => {
  await page.setViewportSize({ width: 1180, height: 720 });
  const panelBounds = await page.locator('[data-panel-id]').evaluateAll((panels) =>
    panels.map((panel) => {
      const bounds = panel.getBoundingClientRect();
      const titlebar = panel.querySelector('.workspace-panel-titlebar');
      return {
        id: panel.getAttribute('data-panel-id'),
        x: bounds.x,
        y: bounds.y,
        right: bounds.right,
        bottom: bounds.bottom,
        width: bounds.width,
        height: bounds.height,
        titleVisible: Boolean(titlebar && titlebar.getBoundingClientRect().width > 0),
      };
    }),
  );

  expect(panelBounds).toHaveLength(3);
  for (const panel of panelBounds) {
    expect(panel.x, `${panel.id} left edge`).toBeGreaterThanOrEqual(0);
    expect(panel.y, `${panel.id} top edge`).toBeGreaterThanOrEqual(0);
    expect(panel.right, `${panel.id} right edge`).toBeLessThanOrEqual(1181);
    expect(panel.bottom, `${panel.id} bottom edge`).toBeLessThanOrEqual(721);
    expect(panel.width, `${panel.id} panel width`).toBeGreaterThan(170);
    expect(panel.height, `${panel.id} panel height`).toBeGreaterThan(100);
    expect(panel.titleVisible, `${panel.id} titlebar`).toBe(true);
  }
  await expect(page.locator('[data-panel-id="backend"] .studio-command')).toBeVisible();
  await expect(page.locator('[data-panel-id="project"] .studio-project')).toBeVisible();
  await expect(page.locator('[data-panel-id="log"] .studio-log')).toBeVisible();
  await expectResponseToFillBackend(page);
  await page.screenshot({ path: resolve(artifactsDirectory, 'shell-1180x720.png') });
});

test('echoes a JSON command into the response and command log', async ({ page }) => {
  const value = { greeting: 'こんにちは', values: [1, null, { ok: true }] };
  await sendEcho(page, value);

  const log = page.locator('[data-panel-id="log"]');
  await expect(log).toContainText(JSON.stringify({ op: 'echo', args: value }));
  await expect(log).toContainText(JSON.stringify({ ok: value }));
});

test('treats an error-shaped JSON value as successful echo data', async ({ page }) => {
  const value = {
    ok: false,
    error: { kind: 'user-data', message: 'ordinary echoed content' },
    nested: { ok: null, error: null },
  };
  await sendEcho(page, value);
  await expect(page.locator('.studio-errors')).toHaveCount(0);
});

test('unpacks valid null, scalar and empty-string echo results in the response panel', async ({ page }) => {
  for (const value of [null, false, 0, '']) await sendEcho(page, value);
  await expect(page.locator('.studio-errors')).toHaveCount(0);
});

test('opens the browser directory picker and lists its real adapter entries', async ({ page }) => {
  await page.getByRole('button', { name: 'Open folder…' }).click();
  const project = page.locator('[data-panel-id="project"]');
  await expect(project).toContainText('browser-fixture/');
  await expect(project).toContainText('fixture.txt');
  await expect(project).toContainText('1 entries · browser');
  await sendEcho(page, {
    adapter: 'browser',
    entries: [{ name: 'fixture.txt', kind: 'file' }],
  });
});

test('keeps a malformed-command error visible after a later successful echo', async ({ page }) => {
  await page.getByLabel('Command (JSON)').fill('{"op":"echo","args":]');
  await page.getByRole('button', { name: 'Send command' }).click();

  const errors = page.locator('.studio-errors');
  await expect(errors).toContainText('malformed_json');
  await expect(errors).toContainText('line');
  await sendEcho(page, { recovered: true });
  await expect(errors).toContainText('malformed_json');
});

test('moves and hides panels, then restores the layout after reload', async ({ page }) => {
  const backend = page.locator('[data-panel-id="backend"]');
  await backend.locator('.workspace-panel-titlebar button[aria-label="More actions"]').click();
  await page.getByRole('menu', { name: 'Backend panel' }).getByRole('menuitemcheckbox', { name: 'Dock left' }).click();
  await expect(backend.locator('.workspace-panel-titlebar')).toHaveAttribute('data-edge', 'left');

  const log = page.locator('[data-panel-id="log"]');
  await log.locator('.workspace-panel-titlebar button[aria-label="More actions"]').click();
  await page.getByRole('menu', { name: 'Log panel' }).getByRole('menuitem', { name: 'Hide panel' }).click();
  await expect(page.locator('[data-panel-id="log"]')).toHaveCount(0);

  await page.reload();
  await expect(page.locator('[data-panel-id="backend"] .workspace-panel-titlebar')).toHaveAttribute('data-edge', 'left');
  await expect(page.locator('[data-panel-id="log"]')).toHaveCount(0);
  await expect(page.locator('[data-panel-id="project"]')).toBeVisible();
});

test('opens menus and About by keyboard, navigates menu items and dismisses them with Escape', async ({ page }) => {
  const fileMenu = page.locator('[data-studio-menu="File"]');
  await page.keyboard.press('F10');
  await expect(fileMenu).toBeFocused();
  await page.keyboard.press('ArrowDown');
  const fileCommands = page.getByRole('menu', { name: 'File' });
  await expect(fileCommands).toBeVisible();
  await expect(fileCommands.getByRole('menuitem', { name: 'Open folder…' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(fileCommands).toHaveCount(0);
  await expect(fileMenu).toBeFocused();

  const helpMenu = page.locator('[data-studio-menu="Help"]');
  await helpMenu.focus();
  await page.keyboard.press('Enter');
  const helpCommands = page.getByRole('menu', { name: 'Help' });
  await expect(helpCommands).toBeVisible();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  const about = page.getByRole('dialog', { name: 'About XEM Studio' });
  await expect(about).toBeVisible();
  const closeAbout = about.getByRole('button', { name: 'Close' });
  await expect(closeAbout).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(closeAbout).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(about).toHaveCount(0);
  await expect(helpMenu).toBeFocused();
});
