// Adapted from rpg_creator src/editor/editorState.ts lines 3552–3582 and
// 3606–3648 at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
// MIT grant extended by the author on 2026-10-21 for this repository.
import { useRef, useState, useSyncExternalStore } from 'react';
import type { FocusEvent, KeyboardEvent, SetStateAction } from 'react';
import {
  applyHistoryEntry, createDeltaHistoryEntry, createHistoryIdentity, createReversibleDelta,
  pruneDeltaHistory,
} from './deltaHistory';
import type { DeltaHistoryEntry } from './deltaHistory';

export interface NameHistorySnapshot<T> {
  /** Last successfully validated and persisted value. */
  value: T;
  /** Editable draft; a failed commit preserves it for correction. */
  draft: T;
  dirty: boolean;
  pending: boolean;
  canUndo: boolean;
  canRedo: boolean;
  error: unknown;
}

export interface NameHistoryController<T> {
  getSnapshot(): NameHistorySnapshot<T>;
  subscribe(listener: () => void): () => void;
  setValue(next: SetStateAction<T>): void;
  commit(next?: T, label?: string): Promise<void>;
  cancelDraft(): void;
  undo(): Promise<void>;
  redo(): Promise<void>;
  /** Replace a loaded/validated sidecar; does not save it or retain its history. */
  reset(value: T): void;
  clearError(): void;
}

type Entry = DeltaHistoryEntry<null>;
const HISTORY_LIMITS = { maxEntries: 256, maxBytes: 4 * 1024 * 1024 };

/** JSON-value history. Validation and persistence belong to onCommit, in that order. */
export function createNameHistory<T>(
  initial: T,
  onCommit: (value: T) => Promise<void>,
): NameHistoryController<T> {
  let revision = createHistoryIdentity('names-revision');
  let undo: readonly Entry[] = [];
  let redo: readonly Entry[] = [];
  const listeners = new Set<() => void>();
  let snapshot: NameHistorySnapshot<T> = {
    value: structuredClone(initial), draft: structuredClone(initial),
    dirty: false, pending: false, canUndo: false, canRedo: false, error: null,
  };
  let pendingCommit: { value: T; promise: Promise<void> } | null = null;

  function publish(next: NameHistorySnapshot<T>) {
    snapshot = {
      ...next,
      canUndo: undo.length > 0 && !next.pending && !next.dirty,
      canRedo: redo.length > 0 && !next.pending && !next.dirty,
    };
    listeners.forEach(listener => listener());
  }
  function assertIdle() {
    if (snapshot.pending) throw new Error('A name transaction is still being saved.');
  }
  function reject(error: unknown) {
    console.error(error);
    publish({ ...snapshot, pending: false, error });
    throw error;
  }
  async function persist(candidate: T, accept: () => void) {
    publish({ ...snapshot, pending: true });
    try {
      // Nothing mutates the committed value, revision or either history stack
      // until the caller has validated AND persisted the candidate.
      await onCommit(structuredClone(candidate));
      accept();
      publish({ ...snapshot, value: candidate, draft: structuredClone(candidate), dirty: false,
        pending: false, error: null });
    } catch (error) {
      reject(error);
    }
  }

  const controller: NameHistoryController<T> = {
    getSnapshot: () => snapshot,
    subscribe: listener => { listeners.add(listener); return () => listeners.delete(listener); },
    setValue(next) {
      assertIdle();
      const candidate = structuredClone(typeof next === 'function'
        ? (next as (previous: T) => T)(structuredClone(snapshot.draft)) : next);
      const dirty = createReversibleDelta(snapshot.value, candidate).forward.length > 0;
      publish({ ...snapshot, draft: candidate, dirty });
    },
    commit(next = snapshot.draft, label = 'Rename') {
      try {
        const candidate = structuredClone(next);
        // A blur followed by the Rename button can request the same save twice.
        // Join that one transaction; never start a second concurrent write.
        if (pendingCommit && createReversibleDelta(pendingCommit.value, candidate).forward.length === 0) {
          return pendingCommit.promise;
        }
        assertIdle();
        const entry = createDeltaHistoryEntry(snapshot.value, candidate, {
          label, beforeRevision: revision, metadata: null,
        });
        if (!entry) {
          publish({ ...snapshot, draft: candidate, dirty: false });
          return Promise.resolve();
        }
        const nextUndo = pruneDeltaHistory([...undo, entry], HISTORY_LIMITS).entries;
        publish({ ...snapshot, draft: candidate, dirty: true });
        const promise: Promise<void> = persist(candidate, () => {
          undo = nextUndo;
          redo = [];
          revision = entry.afterRevision;
        }).finally(() => { if (pendingCommit?.promise === promise) pendingCommit = null; });
        pendingCommit = { value: candidate, promise };
        return promise;
      } catch (error) {
        console.error(error);
        publish({ ...snapshot, error });
        return Promise.reject(error);
      }
    },
    cancelDraft() {
      assertIdle();
      publish({ ...snapshot, draft: structuredClone(snapshot.value), dirty: false });
    },
    async undo() {
      assertIdle();
      if (snapshot.dirty) throw new Error('Commit or cancel the current name edit before undoing.');
      const entry = undo.at(-1);
      if (!entry) return;
      const candidate = applyHistoryEntry(snapshot.value, entry, 'inverse', revision);
      await persist(candidate.value, () => {
        undo = undo.slice(0, -1);
        redo = [...redo, entry];
        revision = candidate.revision;
      });
    },
    async redo() {
      assertIdle();
      if (snapshot.dirty) throw new Error('Commit or cancel the current name edit before redoing.');
      const entry = redo.at(-1);
      if (!entry) return;
      const candidate = applyHistoryEntry(snapshot.value, entry, 'forward', revision);
      await persist(candidate.value, () => {
        redo = redo.slice(0, -1);
        undo = [...undo, entry];
        revision = candidate.revision;
      });
    },
    reset(value) {
      assertIdle();
      const candidate = structuredClone(value);
      revision = createHistoryIdentity('names-revision');
      undo = [];
      redo = [];
      publish({ value: candidate, draft: structuredClone(candidate), dirty: false, pending: false,
        canUndo: false, canRedo: false, error: null });
    },
    clearError() { publish({ ...snapshot, error: null }); },
  };
  return controller;
}

function formHistoryLabel(element: HTMLElement): string {
  const ariaLabel = element.getAttribute('aria-label')?.trim();
  const labelledText = element.closest('label')?.querySelector(':scope > span')?.textContent?.trim();
  const placeholder = element.getAttribute('placeholder')?.trim();
  const name = ariaLabel || labelledText || placeholder || 'field';
  return `Edit ${name.slice(0, 80)}`;
}

function isNameHistoryControl(target: EventTarget | null): target is HTMLElement {
  if (target instanceof HTMLTextAreaElement) return !target.readOnly && !target.disabled;
  if (!(target instanceof HTMLInputElement) || target.readOnly || target.disabled) return false;
  return !['button', 'checkbox', 'file', 'hidden', 'image', 'radio', 'reset', 'search', 'submit']
    .includes(target.type);
}

export function useNameHistory<T>(initial: T, onCommit: (value: T) => Promise<void>) {
  const commitRef = useRef(onCommit);
  commitRef.current = onCommit;
  const [controller] = useState(() => createNameHistory(initial, value => commitRef.current(value)));
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  const label = useRef('Rename');
  // Rejections have already reached onCommit's application error handler and
  // snapshot.error. This event handler keeps that original error visible.
  const commitField = () => { void controller.commit(undefined, label.current).catch(() => {}); };
  const fieldProps = {
    onFocusCapture(event: FocusEvent<HTMLElement>) {
      if (isNameHistoryControl(event.target)) label.current = formHistoryLabel(event.target);
    },
    onBlurCapture(event: FocusEvent<HTMLElement>) {
      if (isNameHistoryControl(event.target) && controller.getSnapshot().dirty) commitField();
    },
    onKeyDownCapture(event: KeyboardEvent<HTMLElement>) {
      if (event.key === 'Escape' && controller.getSnapshot().dirty && !controller.getSnapshot().pending) {
        event.preventDefault();
        event.stopPropagation();
        controller.cancelDraft();
      } else if (event.key === 'Enter' && isNameHistoryControl(event.target)
        && !(event.target instanceof HTMLTextAreaElement) && !event.nativeEvent.isComposing) {
        event.preventDefault();
        commitField();
      } else if ((event.ctrlKey || event.metaKey) && !controller.getSnapshot().dirty
        && !controller.getSnapshot().pending) {
        const key = event.key.toLowerCase();
        if (key === 'z' || key === 'y') {
          event.preventDefault();
          const operation = key === 'y' || event.shiftKey ? controller.redo : controller.undo;
          void operation().catch(() => {});
        }
      }
    },
  };
  return { ...snapshot, ...controller, fieldProps };
}
