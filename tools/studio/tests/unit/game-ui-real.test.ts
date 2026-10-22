import { test } from 'vitest';
import { runRealGameUiReal } from '../e2e/ui-real';

const enabled = process.env.XEM_STUDIO_REAL_UI === '1';

test.skipIf(!enabled)('real browser Studio attach, map navigation, scalar write and snapshot restore', runRealGameUiReal, 600_000);
