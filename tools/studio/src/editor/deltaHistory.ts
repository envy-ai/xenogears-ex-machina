// Copied from rpg_creator src/editor/deltaHistory.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
// MIT grant extended by the author on 2026-10-21 for this repository.
import { createId } from '../domain/ids';

const UNSAFE_PATH_KEYS = new Set(['__proto__', 'prototype', 'constructor']);

const OPERATION_OVERHEAD_BYTES = 32;
const PATH_SEGMENT_OVERHEAD_BYTES = 8;
const DEFAULT_MAX_OPERATIONS = 4096;

export type DeltaPath = ReadonlyArray<string | number>;

export type DeltaOperation =
  | {
      kind: 'set';
      path: DeltaPath;
      value: unknown;
    }
  | {
      kind: 'remove';
      path: DeltaPath;
    }
  | {
      kind: 'splice';
      path: DeltaPath;
      index: number;
      deleteCount: number;
      items: readonly unknown[];
    }
  | {
      kind: 'move';
      path: DeltaPath;
      from: number;
      to: number;
    }
  | {
      kind: 'reorder';
      path: DeltaPath;
      keys: readonly string[];
    };

export interface ReversibleDelta {
  forward: readonly DeltaOperation[];
  inverse: readonly DeltaOperation[];
  estimatedBytes: number;
  usedSubtreeFallback: boolean;
}

export interface DeltaHistoryEntry<Metadata = unknown>
  extends ReversibleDelta {
  id: string;
  label: string;
  beforeRevision: string;
  afterRevision: string;
  metadata: Metadata;
}

export interface DeltaHistoryLimits {
  maxEntries: number;
  maxBytes: number;
}

export interface PrunedDeltaHistory<Metadata> {
  entries: readonly DeltaHistoryEntry<Metadata>[];
  retainedBytes: number;
  oversizedNewestEntry: boolean;
}

interface MutableDelta {
  forward: DeltaOperation[];
  inverse: DeltaOperation[];
  usedSubtreeFallback: boolean;
}

interface DiffOptions {
  maxOperations: number;
}

function hasOwn(value: object, key: PropertyKey): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isUnsafePathKey(key: string | number): boolean {
  return typeof key === 'string' && UNSAFE_PATH_KEYS.has(key);
}

function assertSafePath(path: DeltaPath): void {
  const unsafe = path.find(isUnsafePathKey);
  if (unsafe !== undefined) {
    throw new Error(`Unsafe delta path segment: ${String(unsafe)}`);
  }
}

function valueSize(value: unknown, seen = new WeakSet<object>()): number {
  if (value === null) return 4;
  if (value === undefined) return 1;
  if (typeof value === 'string') return 8 + value.length * 2;
  if (typeof value === 'number') return 8;
  if (typeof value === 'boolean') return 4;
  if (typeof value === 'bigint' || typeof value === 'symbol' || typeof value === 'function') {
    throw new Error(`Unsupported delta value type: ${typeof value}`);
  }
  if (seen.has(value)) {
    throw new Error('Delta values must be acyclic.');
  }
  seen.add(value);
  if (Array.isArray(value)) {
    let bytes = 16;
    value.forEach((entry) => {
      bytes += valueSize(entry, seen);
    });
    seen.delete(value);
    return bytes;
  }
  let bytes = 16;
  Object.entries(value).forEach(([key, entry]) => {
    if (UNSAFE_PATH_KEYS.has(key)) {
      throw new Error(`Unsafe delta value key: ${key}`);
    }
    bytes += key.length * 2 + valueSize(entry, seen);
  });
  seen.delete(value);
  return bytes;
}

export function estimateDeltaValueBytes(value: unknown): number {
  return valueSize(value);
}

function pathSize(path: DeltaPath): number {
  return path.reduce<number>(
    (bytes, segment) =>
      bytes +
      PATH_SEGMENT_OVERHEAD_BYTES +
      (typeof segment === 'string' ? segment.length * 2 : 8),
    0
  );
}

export function estimateDeltaOperationBytes(operation: DeltaOperation): number {
  const base = OPERATION_OVERHEAD_BYTES + pathSize(operation.path);
  switch (operation.kind) {
    case 'set':
      return base + valueSize(operation.value);
    case 'remove':
      return base;
    case 'splice':
      return (
        base +
        16 +
        operation.items.reduce<number>(
          (bytes, item) => bytes + valueSize(item),
          0
        )
      );
    case 'move':
      return base + 16;
    case 'reorder':
      return (
        base +
        16 +
        operation.keys.reduce(
          (bytes, key) => bytes + PATH_SEGMENT_OVERHEAD_BYTES + key.length * 2,
          0
        )
      );
  }
}

export function estimateDeltaBytes(
  forward: readonly DeltaOperation[],
  inverse: readonly DeltaOperation[]
): number {
  return [...forward, ...inverse].reduce(
    (bytes, operation) => bytes + estimateDeltaOperationBytes(operation),
    0
  );
}

function appendDelta(target: MutableDelta, child: MutableDelta): void {
  target.forward.push(...child.forward);
  target.inverse.unshift(...child.inverse);
  target.usedSubtreeFallback ||= child.usedSubtreeFallback;
}

function replacementDelta(
  path: DeltaPath,
  before: unknown,
  after: unknown,
  fallback = false
): MutableDelta {
  return {
    forward: [{ kind: 'set', path, value: after }],
    inverse: [{ kind: 'set', path, value: before }],
    usedSubtreeFallback: fallback
  };
}

/**
 * Object key order is observable — switch/variable records and similar
 * id-keyed maps render in insertion order. Per-key edits cannot restore a
 * removed key to its original position, so every order difference needs an
 * explicit reorder after its value operations in both directions.
 */
function objectKeyOrderChanged(
  before: Record<string, unknown>,
  after: Record<string, unknown>
): boolean {
  const beforeKeys = Object.keys(before);
  const afterKeys = Object.keys(after);
  if (beforeKeys.length !== afterKeys.length) return true;
  for (let index = 0; index < beforeKeys.length; index += 1) {
    if (beforeKeys[index] !== afterKeys[index]) return true;
  }
  return false;
}

function identityKey(value: unknown): string | null {
  if (
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean'
  ) {
    return `${typeof value}:${String(value)}`;
  }
  if (
    isObject(value) &&
    typeof value.id === 'string' &&
    value.id.length > 0
  ) {
    return `id:${value.id}`;
  }
  return null;
}

function uniqueIdentityKeys(values: readonly unknown[]): string[] | null {
  const keys: string[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    const key = identityKey(value);
    if (key === null || seen.has(key)) return null;
    seen.add(key);
    keys.push(key);
  }
  return keys;
}

function canUseIdentityDiff(
  before: readonly unknown[],
  after: readonly unknown[]
): boolean {
  if (before.length === 0 && after.length === 0) return false;
  const beforeKeys = uniqueIdentityKeys(before);
  const afterKeys = uniqueIdentityKeys(after);
  if (!beforeKeys || !afterKeys) return false;
  if (before.length === 0 || after.length === 0) {
    return true;
  }
  const beforeKind = beforeKeys[0].split(':', 1)[0];
  return afterKeys.every((key) => key.startsWith(`${beforeKind}:`));
}

function diffIdentityArray(
  before: readonly unknown[],
  after: readonly unknown[],
  path: DeltaPath,
  options: DiffOptions
): MutableDelta {
  const result: MutableDelta = {
    forward: [],
    inverse: [],
    usedSubtreeFallback: false
  };
  const working = [...before];
  const targetKeys = uniqueIdentityKeys(after) ?? [];
  const targetKeySet = new Set(targetKeys);

  for (let index = working.length - 1; index >= 0; index -= 1) {
    const key = identityKey(working[index]);
    if (key !== null && targetKeySet.has(key)) continue;
    const removed = working[index];
    result.forward.push({
      kind: 'splice',
      path,
      index,
      deleteCount: 1,
      items: []
    });
    result.inverse.unshift({
      kind: 'splice',
      path,
      index,
      deleteCount: 0,
      items: [removed]
    });
    working.splice(index, 1);
  }

  for (let index = 0; index < after.length; index += 1) {
    const target = after[index];
    const targetKey = targetKeys[index];
    const currentKey = identityKey(working[index]);
    if (currentKey === targetKey) continue;
    const existingIndex = working.findIndex(
      (entry, candidateIndex) =>
        candidateIndex > index && identityKey(entry) === targetKey
    );
    if (existingIndex >= 0) {
      result.forward.push({
        kind: 'move',
        path,
        from: existingIndex,
        to: index
      });
      result.inverse.unshift({
        kind: 'move',
        path,
        from: index,
        to: existingIndex
      });
      const [moved] = working.splice(existingIndex, 1);
      working.splice(index, 0, moved);
      continue;
    }
    result.forward.push({
      kind: 'splice',
      path,
      index,
      deleteCount: 0,
      items: [target]
    });
    result.inverse.unshift({
      kind: 'splice',
      path,
      index,
      deleteCount: 1,
      items: []
    });
    working.splice(index, 0, target);
  }

  while (working.length > after.length) {
    const index = working.length - 1;
    const removed = working[index];
    result.forward.push({
      kind: 'splice',
      path,
      index,
      deleteCount: 1,
      items: []
    });
    result.inverse.unshift({
      kind: 'splice',
      path,
      index,
      deleteCount: 0,
      items: [removed]
    });
    working.pop();
  }

  for (let index = 0; index < after.length; index += 1) {
    if (working[index] === after[index]) continue;
    if (
      identityKey(working[index]) === targetKeys[index] &&
      isObject(working[index]) &&
      isObject(after[index])
    ) {
      appendDelta(
        result,
        diffAt(
          working[index],
          after[index],
          [...path, index],
          options
        )
      );
    }
  }
  return result;
}

function diffPlainArray(
  before: readonly unknown[],
  after: readonly unknown[],
  path: DeltaPath,
  options: DiffOptions
): MutableDelta {
  if (before.length === after.length) {
    const result: MutableDelta = {
      forward: [],
      inverse: [],
      usedSubtreeFallback: false
    };
    for (let index = 0; index < before.length; index += 1) {
      appendDelta(
        result,
        diffAt(before[index], after[index], [...path, index], options)
      );
    }
    return result;
  }

  let prefix = 0;
  while (
    prefix < before.length &&
    prefix < after.length &&
    before[prefix] === after[prefix]
  ) {
    prefix += 1;
  }
  let suffix = 0;
  while (
    suffix < before.length - prefix &&
    suffix < after.length - prefix &&
    before[before.length - 1 - suffix] ===
      after[after.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  const removed = before.slice(prefix, before.length - suffix);
  const inserted = after.slice(prefix, after.length - suffix);
  return {
    forward: [
      {
        kind: 'splice',
        path,
        index: prefix,
        deleteCount: removed.length,
        items: inserted
      }
    ],
    inverse: [
      {
        kind: 'splice',
        path,
        index: prefix,
        deleteCount: inserted.length,
        items: removed
      }
    ],
    usedSubtreeFallback: false
  };
}

function maybeCollapse(
  path: DeltaPath,
  before: unknown,
  after: unknown,
  delta: MutableDelta,
  options: DiffOptions
): MutableDelta {
  if (delta.forward.length === 0) return delta;
  const replacement = replacementDelta(path, before, after, true);
  const deltaBytes = estimateDeltaBytes(delta.forward, delta.inverse);
  const replacementBytes = estimateDeltaBytes(
    replacement.forward,
    replacement.inverse
  );
  if (
    delta.forward.length > options.maxOperations ||
    (delta.forward.length >= 32 && deltaBytes > replacementBytes)
  ) {
    return replacement;
  }
  return delta;
}

function diffAt(
  before: unknown,
  after: unknown,
  path: DeltaPath,
  options: DiffOptions
): MutableDelta {
  if (before === after) {
    return {
      forward: [],
      inverse: [],
      usedSubtreeFallback: false
    };
  }

  if (Array.isArray(before) && Array.isArray(after)) {
    const delta = canUseIdentityDiff(before, after)
      ? diffIdentityArray(before, after, path, options)
      : diffPlainArray(before, after, path, options);
    return maybeCollapse(path, before, after, delta, options);
  }

  if (isObject(before) && isObject(after)) {
    const result: MutableDelta = {
      forward: [],
      inverse: [],
      usedSubtreeFallback: false
    };
    const keys = new Set([...Object.keys(before), ...Object.keys(after)]);
    for (const key of keys) {
      if (UNSAFE_PATH_KEYS.has(key)) {
        throw new Error(`Unsafe delta value key: ${key}`);
      }
      const beforeHasKey = hasOwn(before, key);
      const afterHasKey = hasOwn(after, key);
      const childPath = [...path, key];
      if (!beforeHasKey && afterHasKey) {
        result.forward.push({
          kind: 'set',
          path: childPath,
          value: after[key]
        });
        result.inverse.unshift({ kind: 'remove', path: childPath });
        continue;
      }
      if (beforeHasKey && !afterHasKey) {
        result.forward.push({ kind: 'remove', path: childPath });
        result.inverse.unshift({
          kind: 'set',
          path: childPath,
          value: before[key]
        });
        continue;
      }
      appendDelta(
        result,
        diffAt(before[key], after[key], childPath, options)
      );
    }
    if (objectKeyOrderChanged(before, after)) {
      result.forward.push({
        kind: 'reorder',
        path,
        keys: Object.keys(after)
      });
      result.inverse.push({
        kind: 'reorder',
        path,
        keys: Object.keys(before)
      });
    }
    return maybeCollapse(path, before, after, result, options);
  }

  return replacementDelta(path, before, after);
}

export function createReversibleDelta(
  before: unknown,
  after: unknown,
  options: { maxOperations?: number } = {}
): ReversibleDelta {
  try {
    const delta = diffAt(before, after, [], {
      maxOperations: Math.max(
        1,
        Math.trunc(options.maxOperations ?? DEFAULT_MAX_OPERATIONS)
      )
    });
    return {
      ...delta,
      estimatedBytes: estimateDeltaBytes(delta.forward, delta.inverse)
    };
  } catch (error) {
    /*
     * A valid JSON value must remain undoable even if a future specialized
     * differ hits an invariant it cannot satisfy. Unsupported, cyclic, or
     * unsafe values still fail here when the fallback is measured.
     */
    const fallback = replacementDelta([], before, after, true);
    try {
      return {
        ...fallback,
        estimatedBytes: estimateDeltaBytes(
          fallback.forward,
          fallback.inverse
        )
      };
    } catch {
      throw error;
    }
  }
}

function applyOperationAt(
  current: unknown,
  operation: DeltaOperation,
  pathIndex: number
): unknown {
  if (pathIndex < operation.path.length) {
    const segment = operation.path[pathIndex];
    if (isUnsafePathKey(segment)) {
      throw new Error(`Unsafe delta path segment: ${String(segment)}`);
    }
    if (Array.isArray(current)) {
      if (
        typeof segment !== 'number' ||
        !Number.isInteger(segment) ||
        segment < 0 ||
        segment >= current.length
      ) {
        throw new Error(`Invalid delta array path index: ${String(segment)}`);
      }
      const next = [...current];
      next[segment] = applyOperationAt(
        current[segment],
        operation,
        pathIndex + 1
      );
      return next;
    }
    if (!isObject(current) || typeof segment !== 'string') {
      throw new Error(`Invalid delta object path segment: ${String(segment)}`);
    }
    if (pathIndex + 1 < operation.path.length && !hasOwn(current, segment)) {
      throw new Error(`Missing delta path property: ${segment}`);
    }
    const next = { ...current };
    if (pathIndex + 1 === operation.path.length) {
      if (operation.kind === 'remove') {
        if (!hasOwn(current, segment)) {
          throw new Error(`Cannot remove missing delta property: ${segment}`);
        }
        delete next[segment];
        return next;
      }
      next[segment] = applyOperationAt(
        current[segment],
        operation,
        pathIndex + 1
      );
      return next;
    }
    next[segment] = applyOperationAt(
      current[segment],
      operation,
      pathIndex + 1
    );
    return next;
  }

  switch (operation.kind) {
    case 'set':
      return operation.value;
    case 'remove':
      throw new Error('A root delta remove operation is not supported.');
    case 'splice': {
      if (!Array.isArray(current)) {
        throw new Error('Delta splice target is not an array.');
      }
      if (
        operation.index < 0 ||
        operation.index > current.length ||
        operation.deleteCount < 0 ||
        operation.index + operation.deleteCount > current.length
      ) {
        throw new Error('Delta splice range is outside the target array.');
      }
      const next = [...current];
      next.splice(
        operation.index,
        operation.deleteCount,
        ...operation.items
      );
      return next;
    }
    case 'move': {
      if (!Array.isArray(current)) {
        throw new Error('Delta move target is not an array.');
      }
      if (
        operation.from < 0 ||
        operation.from >= current.length ||
        operation.to < 0 ||
        operation.to >= current.length
      ) {
        throw new Error('Delta move range is outside the target array.');
      }
      if (operation.from === operation.to) return current;
      const next = [...current];
      const [moved] = next.splice(operation.from, 1);
      next.splice(operation.to, 0, moved);
      return next;
    }
    case 'reorder': {
      if (!isObject(current)) {
        throw new Error('Delta reorder target is not an object.');
      }
      const currentKeys = Object.keys(current);
      const requestedKeys = new Set(operation.keys);
      if (
        requestedKeys.size !== operation.keys.length ||
        currentKeys.length !== operation.keys.length ||
        operation.keys.some(
          (key) => UNSAFE_PATH_KEYS.has(key) || !hasOwn(current, key)
        )
      ) {
        throw new Error('Delta reorder keys do not match the target object.');
      }
      const next: Record<string, unknown> = {};
      operation.keys.forEach((key) => {
        next[key] = current[key];
      });
      return next;
    }
  }
}

export function applyDeltaOperations<T>(
  value: T,
  operations: readonly DeltaOperation[]
): T {
  let next: unknown = value;
  operations.forEach((operation, index) => {
    try {
      assertSafePath(operation.path);
      next = applyOperationAt(next, operation, 0);
    } catch (error) {
      throw new Error(
        `Delta operation ${index} (${operation.kind}) failed: ${
          error instanceof Error ? error.message : 'invalid operation'
        }`
      );
    }
  });
  return next as T;
}

export function applyHistoryEntry<T, Metadata>(
  value: T,
  entry: DeltaHistoryEntry<Metadata>,
  direction: 'forward' | 'inverse',
  currentRevision: string
): { value: T; revision: string } {
  const expectedRevision =
    direction === 'forward' ? entry.beforeRevision : entry.afterRevision;
  if (currentRevision !== expectedRevision) {
    throw new Error(
      `History revision mismatch for ${entry.id}: expected ${expectedRevision}, received ${currentRevision}.`
    );
  }
  try {
    return {
      value: applyDeltaOperations(
        value,
        direction === 'forward' ? entry.forward : entry.inverse
      ),
      revision:
        direction === 'forward'
          ? entry.afterRevision
          : entry.beforeRevision
    };
  } catch (error) {
    throw new Error(
      `History entry ${entry.id} "${entry.label}" ${direction} failed: ${
        error instanceof Error ? error.message : 'invalid delta'
      }`
    );
  }
}

export function createHistoryIdentity(prefix: string): string {
  return createId(prefix);
}

export function createDeltaHistoryEntry<Metadata>(
  before: unknown,
  after: unknown,
  options: {
    label: string;
    beforeRevision: string;
    afterRevision?: string;
    metadata: Metadata;
    id?: string;
    maxOperations?: number;
  }
): DeltaHistoryEntry<Metadata> | null {
  const delta = createReversibleDelta(before, after, {
    maxOperations: options.maxOperations
  });
  if (delta.forward.length === 0) return null;
  return {
    ...delta,
    id: options.id ?? createHistoryIdentity('history'),
    label: options.label,
    beforeRevision: options.beforeRevision,
    afterRevision:
      options.afterRevision ?? createHistoryIdentity('revision'),
    metadata: options.metadata
  };
}

export function pruneDeltaHistory<Metadata>(
  entries: readonly DeltaHistoryEntry<Metadata>[],
  limits: DeltaHistoryLimits
): PrunedDeltaHistory<Metadata> {
  const maxEntries = Math.max(1, Math.trunc(limits.maxEntries));
  const maxBytes = Math.max(1, Math.trunc(limits.maxBytes));
  let retained = entries.slice(-maxEntries);
  let retainedBytes = retained.reduce(
    (bytes, entry) => bytes + entry.estimatedBytes,
    0
  );
  while (retained.length > 1 && retainedBytes > maxBytes) {
    retainedBytes -= retained[0].estimatedBytes;
    retained = retained.slice(1);
  }
  return {
    entries: retained,
    retainedBytes,
    oversizedNewestEntry:
      retained.length === 1 && retainedBytes > maxBytes
  };
}
