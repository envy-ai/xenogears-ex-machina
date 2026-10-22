import { expect, test, type Page } from '@playwright/test';
import { syntheticEventComponent } from './events.fixture';

test.skip(process.env.VITE_XEM_STUDIO_TEST !== '1', 'Graph QoL journeys require the explicit test-only Studio driver.');

async function loadSyntheticGraph(page: Page, fixture = syntheticEventComponent()): Promise<void> {
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await page.evaluate(async ({ bytes, declaredSize }) => {
    const driver = window.__xemStudioTest;
    if (!driver) throw new Error('The explicit Studio test driver is unavailable.');
    await driver.loadComponent(bytes, declaredSize, 0);
  }, fixture);
  await page.getByRole('tab', { name: 'Flow graph' }).click();
  await expect(page.getByRole('group', { name: 'Event flow graph canvas' })).toBeVisible();
  await expect(page.locator('.graph-canvas__wire')).not.toHaveCount(0);
}

async function scaleAtPoint(page: Page, x: number, y: number): Promise<{ x: number; y: number }> {
  return page.getByRole('group', { name: 'Event flow graph canvas' }).evaluate((viewport, point) => {
    const world = viewport.querySelector<HTMLElement>('.graph-canvas__world');
    if (!world) throw new Error('Graph world element is missing.');
    const transform = getComputedStyle(world).transform;
    const match = /^matrix\(([^,]+)/.exec(transform);
    const scale = match ? Number(match[1]) : 1;
    const bounds = world.getBoundingClientRect();
    return { x: (point.x - bounds.left) / scale, y: (point.y - bounds.top) / scale };
  }, { x, y });
}

test('reverses wheel zoom around the pointer and preserves graph controls and selection', async ({ page }) => {
  await loadSyntheticGraph(page);
  const canvas = page.getByRole('group', { name: 'Event flow graph canvas' });
  await page.getByRole('button', { name: 'Fit graph' }).click();

  const bounds = await canvas.boundingBox();
  expect(bounds).not.toBeNull();
  const pointer = { x: bounds!.x + bounds!.width * 0.67, y: bounds!.y + bounds!.height * 0.38 };
  await page.mouse.move(pointer.x, pointer.y);
  const zoomLabel = page.locator('.graph-canvas__toolbar .editor-text--code');
  const initialZoom = Number((await zoomLabel.textContent())?.replace('%', ''));
  const beforeIn = await scaleAtPoint(page, pointer.x, pointer.y);
  await page.mouse.wheel(0, -120);
  await expect.poll(async () => Number((await zoomLabel.textContent())?.replace('%', '')))
    .toBeGreaterThan(initialZoom);
  const afterIn = await scaleAtPoint(page, pointer.x, pointer.y);
  expect(Math.abs(afterIn.x - beforeIn.x)).toBeLessThan(2);
  expect(Math.abs(afterIn.y - beforeIn.y)).toBeLessThan(2);

  const zoomedIn = Number((await zoomLabel.textContent())?.replace('%', ''));
  const beforeOut = await scaleAtPoint(page, pointer.x, pointer.y);
  await page.mouse.wheel(0, 120);
  await expect.poll(async () => Number((await zoomLabel.textContent())?.replace('%', '')))
    .toBeLessThan(zoomedIn);
  const afterOut = await scaleAtPoint(page, pointer.x, pointer.y);
  expect(Math.abs(afterOut.x - beforeOut.x)).toBeLessThan(2);
  expect(Math.abs(afterOut.y - beforeOut.y)).toBeLessThan(2);

  const beforeButtonIn = Number((await zoomLabel.textContent())?.replace('%', ''));
  await page.getByRole('button', { name: 'Zoom in' }).click();
  const afterButtonIn = Number((await zoomLabel.textContent())?.replace('%', ''));
  expect(afterButtonIn).toBeGreaterThan(beforeButtonIn);
  const sourceNode = page.locator('[data-graph-node]').filter({ has: page.locator('.graph-canvas__port--branch') }).first();
  await sourceNode.click();
  await expect(sourceNode).toHaveAttribute('aria-pressed', 'true');
  await page.screenshot({ path: test.info().outputPath('graph-qol-wheel.png') });
});

test('routes graph edges from bottom or right outputs into top-center inputs', async ({ page }) => {
  await loadSyntheticGraph(page);
  await page.getByRole('button', { name: 'Fit graph' }).click();

  const geometry = await page.getByRole('group', { name: 'Event flow graph canvas' }).evaluate(viewport => {
    const world = viewport.querySelector<HTMLElement>('.graph-canvas__world');
    if (!world) throw new Error('Graph world element is missing.');
    const worldBounds = world.getBoundingClientRect();
    const scaleMatch = /^matrix\(([^,]+)/.exec(getComputedStyle(world).transform);
    const scale = scaleMatch ? Number(scaleMatch[1]) : 1;
    const markerCenter = (marker: Element) => {
      const bounds = marker.getBoundingClientRect();
      return {
        x: (bounds.left + bounds.width / 2 - worldBounds.left) / scale,
        y: (bounds.top + bounds.height / 2 - worldBounds.top) / scale,
      };
    };
    const nodes = [...viewport.querySelectorAll<HTMLElement>('[data-graph-node]')].map(node => {
      const bounds = node.getBoundingClientRect();
      return {
        id: node.dataset.graphNode ?? '',
        left: (bounds.left - worldBounds.left) / scale,
        top: (bounds.top - worldBounds.top) / scale,
        width: bounds.width / scale,
        height: bounds.height / scale,
        input: markerCenter(node.querySelector('.graph-canvas__port--input')!),
        output: markerCenter(node.querySelector('.graph-canvas__port--output')!),
        branch: node.querySelector('.graph-canvas__port--branch')
          ? markerCenter(node.querySelector('.graph-canvas__port--branch')!) : null,
      };
    });
    const edges = [...viewport.querySelectorAll<SVGPathElement>('.graph-canvas__wire')].map(path => {
      const values = (path.getAttribute('d') ?? '').match(/-?[\d.]+/g)?.map(Number) ?? [];
      if (values.length < 8 || !values.every(Number.isFinite)) throw new Error('A rendered wire has invalid cubic geometry.');
      return {
        kind: path.dataset.kind ?? '',
        start: { x: values[0]!, y: values[1]! },
        end: { x: values.at(-2)!, y: values.at(-1)! },
      };
    });
    return { nodes, edges };
  });

  expect(geometry.edges.some(edge => edge.kind === 'branch')).toBe(true);
  const close = (a: { x: number; y: number } | undefined, b: { x: number; y: number } | null) =>
    Boolean(a && b && Math.abs(a.x - b.x) < 1.5 && Math.abs(a.y - b.y) < 1.5);
  const branchSources = new Set<string>();
  for (const edge of geometry.edges) {
    const source = geometry.nodes.find(node => close(edge.start, edge.kind === 'branch' ? node.branch : node.output));
    expect(source, `${edge.kind} edge starts at its expected output port`).toBeDefined();
    if (edge.kind === 'branch') branchSources.add(source!.id);
    expect(geometry.nodes.some(node => close(edge.end, node.input)), `${edge.kind} edge ends at a top input port`).toBe(true);
  }
  expect(branchSources.size).toBeGreaterThan(0);
  expect(geometry.nodes.filter(node => node.branch).map(node => node.id).sort()).toEqual([...branchSources].sort());
  await page.screenshot({ path: test.info().outputPath('graph-qol-ports.png') });
});

test('routes a backward jump around node frames while keeping its ports connected', async ({ page }) => {
  const fixture = syntheticEventComponent();
  const bytecodeOffset = 132 + 2 * 64;
  // The invented jump at PC 18 normally targets PC 25. Aim it back at the
  // component entry to exercise the canvas's backward-edge detour geometry.
  fixture.bytes[bytecodeOffset + 19] = 1;
  fixture.bytes[bytecodeOffset + 20] = 0;
  await loadSyntheticGraph(page, fixture);
  await page.getByRole('button', { name: 'Fit graph' }).click();

  const geometry = await page.getByRole('group', { name: 'Event flow graph canvas' }).evaluate(viewport => {
    const world = viewport.querySelector<HTMLElement>('.graph-canvas__world');
    if (!world) throw new Error('Graph world element is missing.');
    const worldBounds = world.getBoundingClientRect();
    const transformMatch = /^matrix\(([^,]+)/.exec(getComputedStyle(world).transform);
    const scale = transformMatch ? Number(transformMatch[1]) : 1;
    const markerCenter = (node: HTMLElement, selector: string) => {
      const marker = node.querySelector(selector);
      if (!marker) throw new Error(`Expected graph marker ${selector} is missing.`);
      const bounds = marker.getBoundingClientRect();
      return { x: (bounds.left + bounds.width / 2 - worldBounds.left) / scale,
        y: (bounds.top + bounds.height / 2 - worldBounds.top) / scale };
    };
    const node = (id: string) => {
      const element = viewport.querySelector<HTMLElement>(`[data-graph-node="${id}"]`);
      if (!element) throw new Error(`Expected graph node ${id} is missing.`);
      const bounds = element.getBoundingClientRect();
      return { left: (bounds.left - worldBounds.left) / scale,
        right: (bounds.right - worldBounds.left) / scale,
        input: markerCenter(element, '.graph-canvas__port--input'),
        output: markerCenter(element, '.graph-canvas__port--output') };
    };
    const source = node('18');
    const target = node('1');
    const paths = [...viewport.querySelectorAll<SVGPathElement>('.graph-canvas__wire')];
    const path = paths.find(candidate => {
      const values = candidate.getAttribute('d')?.match(/-?[\d.]+/g)?.map(Number) ?? [];
      return values.length >= 8
        && Math.abs(values[0]! - source.output.x) < 1.5 && Math.abs(values[1]! - source.output.y) < 1.5
        && Math.abs(values.at(-2)! - target.input.x) < 1.5 && Math.abs(values.at(-1)! - target.input.y) < 1.5;
    });
    if (!path) throw new Error('The PC 18 to PC 1 backward edge was not rendered between its ports.');
    const values = path.getAttribute('d')?.match(/-?[\d.]+/g)?.map(Number) ?? [];
    return { values, sideClearance: Math.max(source.right, target.right) };
  });
  expect(geometry.values.length).toBeGreaterThanOrEqual(14);
  const controlXs = geometry.values.slice(2, -2).filter((_, index) => index % 2 === 0);
  expect(Math.max(...controlXs)).toBeGreaterThan(geometry.sideClearance);
  await page.screenshot({ path: test.info().outputPath('graph-qol-back-edge.png') });
});
