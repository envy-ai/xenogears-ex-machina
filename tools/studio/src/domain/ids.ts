// Copied from rpg_creator src/domain/ids.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
export function nextSequentialId(
  records: readonly { id: string }[],
  prefix: string
): string {
  let suffix = records.length + 1;
  let id = `${prefix}-${suffix}`;
  while (records.some((entry) => entry.id === id)) {
    suffix += 1;
    id = `${prefix}-${suffix}`;
  }
  return id;
}

export function uniqueSuffixedId(
  claimed: Iterable<string>,
  base: string,
  options?: { separator?: string; startSuffix?: number }
): string {
  const separator = options?.separator ?? '-';
  const ids = new Set(claimed);
  let candidate = base;
  let suffix = options?.startSuffix ?? 2;
  while (ids.has(candidate)) {
    candidate = `${base}${separator}${suffix}`;
    suffix += 1;
  }
  return candidate;
}

let fallbackIdCounter = 0;

export function createId(prefix: string): string {
  fallbackIdCounter += 1;
  const random =
    typeof globalThis.crypto?.randomUUID === 'function'
      ? globalThis.crypto.randomUUID()
      : `${Date.now().toString(36)}-${fallbackIdCounter.toString(36)}`;
  return `${prefix}-${random}`;
}
