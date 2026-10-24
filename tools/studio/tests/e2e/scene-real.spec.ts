import { test } from '@playwright/test';
import { runRealSceneStageA } from './scene-real';

const enabled = process.env.XEM_STUDIO_M7_REAL_SCENE_STAGE_A === '1';
test.skip(!enabled, 'set XEM_STUDIO_M7_REAL_SCENE_STAGE_A=1 for the explicit local original-disc discovery');

test('gated M7 original-disc Scene opens and observes a qualified runtime actor without authoring changes', async ({ page }) => {
  test.setTimeout(600_000);
  await runRealSceneStageA(page);
});
