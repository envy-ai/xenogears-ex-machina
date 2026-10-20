// Copied from rpg_creator src/editor/storedJson.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
export function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

export function readStoredJson<T>(
  key: string,
  parse: (value: unknown) => T,
  fallback: T,
  storage: Pick<Storage, 'getItem'>,
  options: { rawString?: boolean } = {}
): T {
  const raw = storage.getItem(key);
  if (raw === null) return fallback;
  if (options.rawString) return parse(raw);
  return parse(JSON.parse(raw) as unknown);
}
