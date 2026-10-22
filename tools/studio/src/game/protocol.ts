/** Runtime control wire values. Unsafe integer tokens retain their exact spelling. */
export class ExactInteger {
  constructor(readonly decimal: string) {
    if (!/^-?(?:0|[1-9]\d*)$/.test(decimal)) throw new Error(`Invalid integer token: ${decimal}`);
  }
  toString() { return this.decimal; }
}
export type GameValue = null | boolean | number | string | ExactInteger | GameValue[] | { [name: string]: GameValue };

export function parseGameJson(text: string): GameValue {
  // Modern pinned Chromium/Node implement the JSON source-text access proposal.
  // Refuse a missing context rather than accepting rounded 64-bit values.
  return JSON.parse(text, (_key: string, value: unknown, context?: { source?: string }) => {
    if (typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)) {
      if (!context?.source || !/^-?(?:0|[1-9]\d*)$/.test(context.source)) {
        throw new Error('Runtime integer cannot be represented exactly by this JavaScript host');
      }
      return new ExactInteger(context.source);
    }
    return value;
  }) as GameValue;
}
export function gameJson(value: GameValue): string {
  if (value instanceof ExactInteger) return value.decimal;
  if (Array.isArray(value)) return `[${value.map(gameJson).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    return `{${Object.entries(value).map(([key, item]) => `${JSON.stringify(key)}:${gameJson(item)}`).join(',')}}`;
  }
  if (typeof value === 'number' && (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value)))) {
    throw new Error('Use an exact decimal integer token instead of an unsafe JavaScript number');
  }
  return JSON.stringify(value);
}
export function gameReply(text: string): GameValue {
  const value = parseGameJson(text);
  if (value !== null && typeof value === 'object' && !Array.isArray(value) && !(value instanceof ExactInteger) && Object.keys(value).length === 1 && typeof value.error === 'string') {
    throw new Error(typeof value.error === 'string' ? value.error : gameJson(value.error));
  }
  return value;
}
export function record(value: GameValue, description: string): { [name: string]: GameValue } {
  if (value === null || typeof value !== 'object' || Array.isArray(value) || value instanceof ExactInteger) throw new Error(`${description} is not an object`);
  return value;
}
export function frameCount(count: number): number {
  if (!Number.isSafeInteger(count) || count <= 0 || count > 10_000) throw new Error('Frame count must be an integer from 1 to 10000');
  return count;
}
export interface GameTransport {
  readonly kind: 'native' | 'browser';
  request(json: string): Promise<string>;
  frames(count: number): Promise<string>;
  running(value?: boolean): Promise<boolean>;
  onLoss(listener: (error: Error) => void): () => void;
  close(): Promise<void>;
}
