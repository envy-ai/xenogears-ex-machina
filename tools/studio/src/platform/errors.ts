/** Preserve the original failure and every cleanup diagnostic. */
export async function cleanupAfterFailure(error: unknown, cleanups: Array<() => Promise<unknown>>): Promise<never> {
  const failures: unknown[] = [];
  for (const cleanup of cleanups) {
    try { await cleanup(); } catch (failure) { failures.push(failure); }
  }
  if (failures.length) {
    const message = error instanceof Error ? error.message : String(error);
    throw new AggregateError([error, ...failures], `${message} (cleanup also failed)`, { cause: error });
  }
  throw error;
}
