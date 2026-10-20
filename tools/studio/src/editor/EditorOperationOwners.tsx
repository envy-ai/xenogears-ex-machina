// Copied from rpg_creator src/editor/EditorOperationOwners.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import { createContext, useContext, useLayoutEffect, useRef, type ReactNode } from 'react';
export interface EditorExecutionGuard {
  signal: AbortSignal;
  assertCurrent: () => void;
  isCurrent: () => boolean;
}

export interface EditorOperation {
  id: string;
  ownerId?: string;
  description: string;
  kind: 'read' | 'view' | 'side-effect' | 'authored';
  schema: { parse: (args: unknown) => unknown };
  allowsConflict?: () => boolean;
  execute: (args: unknown, guard: EditorExecutionGuard) => unknown | Promise<unknown>;
}

export type EditorOwnedOperation = EditorOperation & {
  /** The successful operation intentionally closes or hands off its mounted owner. */
  releasesOwner?: boolean;
};

export interface EditorOperationOwner {
  id: string;
  label: string;
  /** A mounted owner may resolve its own modal, never a nested unrelated modal. */
  isCurrent: () => boolean;
  describe: () => unknown;
  operations: readonly EditorOwnedOperation[];
}

/** Mounted UI owners expose their authoritative state/closures through live leases. */
export class EditorOperationOwnerRegistry {
  private readonly owners = new Map<string, () => EditorOperationOwner>();

  register(id: string, read: () => EditorOperationOwner): () => void {
    if (this.owners.has(id)) throw new Error(`Editor operation owner is already mounted: ${id}`);
    const owner = read();
    if (!id || owner.id !== id) throw new Error('The editor operation owner identity does not match its registration.');
    const operationIds = owner.operations.map(operation => operation.id);
    if (operationIds.some(operationId => !operationId) || new Set(operationIds).size !== operationIds.length) throw new Error(`Editor operation IDs must be nonempty and unique within owner: ${id}`);
    this.owners.set(id, read);
    return () => { if (this.owners.get(id) === read) this.owners.delete(id); };
  }

  getOwners(): unknown[] {
    return [...this.owners.values()].map(read => {
      const owner = read();
      return { id: owner.id, label: owner.label, current: owner.isCurrent(), context: owner.describe() };
    });
  }

  getOperations(): EditorOperation[] {
    return [...this.owners.entries()].flatMap(([ownerId, read]) => read().operations.map(operation => ({
      ...operation,
      id: `${ownerId}.${operation.id}`,
      ownerId,
      allowsConflict: () => this.owners.get(ownerId) === read && read().isCurrent(),
      execute: async (args, guard) => {
        guard.assertCurrent();
        if (this.owners.get(ownerId) !== read) throw new Error(`Editor operation owner has closed: ${ownerId}`);
        const owner = read();
        if (!owner.isCurrent()) throw new Error(`Editor operation owner is blocked by another interaction: ${ownerId}`);
        const current = owner.operations.find(candidate => candidate.id === operation.id);
        if (!current) throw new Error(`Editor operation is no longer available: ${ownerId}.${operation.id}`);
        const assertCurrent = () => {
          guard.assertCurrent();
          if (this.owners.get(ownerId) !== read) throw new Error(`Editor operation owner has closed: ${ownerId}`);
          if (!read().isCurrent()) throw new Error(`Editor operation owner is blocked by another interaction: ${ownerId}`);
        };
        const result = await current.execute(current.schema.parse(args), {
          signal: guard.signal,
          assertCurrent,
          isCurrent: () => { try { assertCurrent(); return true; } catch { return false; } }
        });
        // Do not recheck project revision after an authored operation's own commit.
        // Normal operations must still retain their owner when their promise resolves.
        if (!current.releasesOwner) {
          if (this.owners.get(ownerId) !== read) throw new Error(`Editor operation owner has closed: ${ownerId}`);
          if (!read().isCurrent()) throw new Error(`Editor operation owner is blocked by another interaction: ${ownerId}`);
        }
        return result;
      }
    })));
  }
}

const OwnerContext = createContext<EditorOperationOwnerRegistry | null>(null);
export function EditorOperationOwnersProvider({ registry, children }: { registry: EditorOperationOwnerRegistry; children: ReactNode }) {
  return <OwnerContext.Provider value={registry}>{children}</OwnerContext.Provider>;
}

export function useEditorOperationOwnersRegistry(): EditorOperationOwnerRegistry | null {
  return useContext(OwnerContext);
}

/** Registration is optional when an editor is mounted without an operation registry. */
export function useEditorOperationOwner(owner: EditorOperationOwner): void {
  const registry = useContext(OwnerContext);
  const latest = useRef(owner);
  latest.current = owner;
  useLayoutEffect(() => registry?.register(owner.id, () => latest.current), [registry, owner.id]);
}

/** Owner-local modal permission, using the editor's actual mounted dialog order. */
export function isCurrentEditorDialog(element: HTMLElement | null): boolean {
  if (!element?.isConnected) return false;
  const dialogs = [...document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"], [role="alertdialog"][aria-modal="true"]')]
    .filter(dialog => !dialog.hidden && dialog.getAttribute('aria-hidden') !== 'true');
  return dialogs.at(-1) === element;
}

/** Embedded controls belong to their containing modal, or to the editor when no modal is open. */
export function isCurrentEditorSurface(element: HTMLElement | null): boolean {
  if (!element?.isConnected) return false;
  const dialog = element.closest<HTMLElement>('[role="dialog"][aria-modal="true"], [role="alertdialog"][aria-modal="true"]');
  if (dialog) return isCurrentEditorDialog(dialog);
  return ![...document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"], [role="alertdialog"][aria-modal="true"]')]
    .some(candidate => !candidate.hidden && candidate.getAttribute('aria-hidden') !== 'true');
}
