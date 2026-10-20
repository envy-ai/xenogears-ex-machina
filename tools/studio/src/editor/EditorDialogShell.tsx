// Copied from rpg_creator src/editor/EditorDialogShell.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import { z } from 'zod';
import { isCurrentEditorDialog, useEditorOperationOwner } from './EditorOperationOwners';
import { useEffect, useId, useMemo, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

interface ActiveEditorDialog {
  id: symbol;
  onClose: () => void;
}

/**
 * Open dialog shells in mount order. Escape closes only the most recently
 * mounted shell so a dialog stacked above another modal (for example a
 * confirmation above the database window) does not also close the modal
 * underneath.
 */
const activeEditorDialogs: ActiveEditorDialog[] = [];

export interface EditorDialogShellProps {
  /** Backdrop class owned by the shared surface contract or the feature. */
  backdropClassName: string;
  /**
   * Invoked when Escape is pressed. Must produce the same outcome as the
   * dialog's cancel/close action.
   */
  onClose: () => void;
  /** Optional test id preserved on the backdrop element. */
  testId?: string;
  /**
   * The dialog surface element. It keeps its own semantic role
   * (`dialog` or `alertdialog`), `aria-modal`, and accessible name.
   */
  children: ReactNode;
}

/**
 * Shared modal shell for editor dialogs. It owns portal placement under
 * `.editor-shell` (so backdrop and surface theme contracts apply), the
 * backdrop element, and Escape-to-close. Feature dialogs compose it around
 * their own surface markup and keep ownership of the surface role, naming,
 * and actions.
 */
export function EditorDialogShell({
  backdropClassName,
  onClose,
  testId,
  children
}: EditorDialogShellProps) {
  const id = useMemo(() => Symbol('editor-dialog'), []);
  const ownerId = useId();
  const backdropRef = useRef<HTMLDivElement>(null);
  const surface = () => backdropRef.current?.querySelector<HTMLElement>('[role="dialog"][aria-modal="true"], [role="alertdialog"][aria-modal="true"]') ?? null;
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEditorOperationOwner({ id: `dialog-shell.${testId ?? 'modal'}.${ownerId}`, label: testId ?? 'Editor dialog',
    isCurrent: () => isCurrentEditorDialog(surface()),
    describe: () => ({ testId, label: surface()?.getAttribute('aria-label') ?? surface()?.querySelector('h2')?.textContent ?? null }),
    operations: [
      { id: 'inspect', description: 'Inspect this exact mounted dialog shell and its current accessible label.', kind: 'read', schema: z.strictObject({}), execute(_args, guard) {
        guard.assertCurrent(); return { testId, label: surface()?.getAttribute('aria-label') ?? surface()?.querySelector('h2')?.textContent ?? null };
      } },
      { id: 'close_requested', description: 'Request the same close/cancel outcome as Escape on this exact mounted dialog. Existing dirty-state confirmation and caller decisions remain authoritative; this only acknowledges delivery.', kind: 'view', releasesOwner: true, schema: z.strictObject({}), async execute(_args, guard) {
        guard.assertCurrent(); await onCloseRef.current(); return { requested: true };
      } }
    ]
  });

  useEffect(() => {
    const entry: ActiveEditorDialog = {
      id,
      onClose: () => onCloseRef.current()
    };
    activeEditorDialogs.push(entry);

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== 'Escape') {
        return;
      }
      const topmost =
        activeEditorDialogs[activeEditorDialogs.length - 1];
      if (topmost?.id !== id) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      topmost.onClose();
    }

    window.addEventListener('keydown', handleKeyDown, true);
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true);
      const index = activeEditorDialogs.indexOf(entry);
      if (index >= 0) {
        activeEditorDialogs.splice(index, 1);
      }
    };
  }, [id]);

  return createPortal(
    <div ref={backdropRef} className={backdropClassName} role="presentation" data-testid={testId}>
      {children}
    </div>,
    document.querySelector<HTMLElement>('.editor-shell') ?? document.body
  );
}
