import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createNameHistory } from '../../src/editor/NameHistory';

type Names = { variables: Record<string, string> };
const initial: Names = { variables: { '2': 'Variable 2' } };

describe('name history transactions', () => {
  beforeEach(() => vi.spyOn(console, 'error').mockImplementation(() => {}));
  afterEach(() => vi.restoreAllMocks());

  it('persists a rename and persists its undo and redo before changing committed state', async () => {
    const writes: Names[] = [];
    const history = createNameHistory(initial, async value => { writes.push(value); });
    const renamed: Names = { variables: { '2': 'Hero HP' } };

    await history.commit(renamed, 'Rename variable 2');
    expect(history.getSnapshot()).toMatchObject({ value: renamed, draft: renamed, dirty: false, canUndo: true, canRedo: false });
    await history.undo();
    expect(history.getSnapshot()).toMatchObject({ value: initial, dirty: false, canUndo: false, canRedo: true });
    await history.redo();
    expect(history.getSnapshot()).toMatchObject({ value: renamed, dirty: false, canUndo: true, canRedo: false });
    expect(writes).toEqual([renamed, initial, renamed]);
  });

  it('keeps the committed value and existing history when persistence rejects', async () => {
    const sentinel = new Error('sidecar write failed');
    let shouldFail = false;
    const history = createNameHistory(initial, async () => {
      if (shouldFail) throw sentinel;
    });
    await history.commit({ variables: { '2': 'First name' } });
    shouldFail = true;
    const failed: Names = { variables: { '2': 'Unwritten name' } };

    await expect(history.commit(failed)).rejects.toBe(sentinel);
    expect(history.getSnapshot()).toMatchObject({
      value: { variables: { '2': 'First name' } },
      draft: failed,
      dirty: true,
      pending: false,
      canUndo: false,
      error: sentinel,
    });
    history.cancelDraft();
    expect(history.getSnapshot()).toMatchObject({ dirty: false, canUndo: true });
    shouldFail = false;
    await history.undo();
    expect(history.getSnapshot().value).toEqual(initial);
  });

  it('coalesces the same in-flight save and rejects a conflicting transaction', async () => {
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const persist = vi.fn(async () => gate);
    const history = createNameHistory(initial, persist);
    const candidate: Names = { variables: { '2': 'Coalesced name' } };

    const first = history.commit(candidate);
    const duplicate = history.commit(candidate);
    expect(duplicate).toBe(first);
    expect(persist).toHaveBeenCalledTimes(1);
    await expect(history.commit({ variables: { '2': 'Different name' } })).rejects.toThrow('transaction is still being saved');
    release();
    await Promise.all([first, duplicate]);
    expect(history.getSnapshot()).toMatchObject({ value: candidate, pending: false, dirty: false });
    expect(persist).toHaveBeenCalledTimes(1);
  });
});
