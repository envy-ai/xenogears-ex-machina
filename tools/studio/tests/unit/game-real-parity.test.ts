import { test } from 'vitest';
import { runRealGameRealParity } from '../e2e/real-parity';

const enabled = process.env.XEM_STUDIO_REAL_PARITY === '1';

test.skipIf(!enabled)('same module-bound real disc replay matches in native and browser controllers', runRealGameRealParity, 600_000);
