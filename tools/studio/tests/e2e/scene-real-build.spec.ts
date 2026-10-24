import { test } from '@playwright/test';
import { runRealSceneBuildPlay } from './scene-real-build';

const enabled = process.env.XEM_STUDIO_M7_REAL_SCENE_BUILD === '1';
test.skip(!enabled, 'set XEM_STUDIO_M7_REAL_SCENE_BUILD=1 for the explicit local original-disc Scene Build/Play acceptance');

test('gated M7 original-disc Scene edit saves, builds, plays and reads back the same package', async ({ page }) => {
  test.setTimeout(900_000);
  await runRealSceneBuildPlay(page);
});
