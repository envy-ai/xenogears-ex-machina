// Copied from rpg_creator src/editor/ContextMenu.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type Ref,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent
} from 'react';
import { createPortal, flushSync } from 'react-dom';
import { z } from 'zod';
import { isCurrentEditorDialog, useEditorOperationOwner } from './EditorOperationOwners';
import type {
  EditorCommand,
  EditorContextMenuRequest
} from './editorCommands';
import { EditorDialogShell } from './EditorDialogShell';

interface ContextMenuProps {
  request: EditorContextMenuRequest | null;
  onClose: () => void;
}

function enabledItems(menu: HTMLElement): HTMLButtonElement[] {
  return [...menu.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]')].filter(
    (item) =>
      item.getAttribute('aria-disabled') !== 'true' &&
      item.offsetParent !== null
  );
}

export function ContextMenu({ request, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: 0, top: 0 });
  const [expandedCommandId, setExpandedCommandId] = useState<string | null>(
    null
  );
  const requestKey = request
    ? `${JSON.stringify(request.target)}:${request.label}:${request.x}:${request.y}`
    : null;

  useEffect(() => {
    setExpandedCommandId(null);
  }, [requestKey]);

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!request || !menu) return;
    const margin = 8;
    const bounds = menu.getBoundingClientRect();
    setPosition({
      left: Math.max(
        margin,
        Math.min(request.x, window.innerWidth - bounds.width - margin)
      ),
      top: Math.max(
        margin,
        Math.min(request.y, window.innerHeight - bounds.height - margin)
      )
    });
    enabledItems(menu)[0]?.focus();
  }, [expandedCommandId, requestKey]);

  useEffect(() => {
    if (!request) return;
    const closeFromPointer = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    const closeFromBlur = () => onClose();
    document.addEventListener('pointerdown', closeFromPointer, true);
    window.addEventListener('blur', closeFromBlur);
    return () => {
      document.removeEventListener('pointerdown', closeFromPointer, true);
      window.removeEventListener('blur', closeFromBlur);
    };
  }, [onClose, requestKey]);

  useEffect(() => {
    const restoreFocus = request?.restoreFocus;
    return () => restoreFocus?.focus();
  }, [requestKey]);

  if (!request) return null;

  function runCommand(command: EditorCommand) {
    if (command.disabledReason) return;
    if (command.children?.length) {
      setExpandedCommandId((current) =>
        current === command.id ? null : command.id
      );
      return;
    }
    onClose();
    command.execute();
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
    const menu = menuRef.current;
    if (!menu) return;
    const items = enabledItems(menu);
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === 'Escape' || event.key === 'Tab') {
      event.preventDefault();
      onClose();
      request?.restoreFocus?.focus();
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const direction = event.key === 'ArrowDown' ? 1 : -1;
      items[(index + direction + items.length) % items.length]?.focus();
      return;
    }
    if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      items[event.key === 'Home' ? 0 : items.length - 1]?.focus();
      return;
    }
    if (event.key === 'ArrowRight') {
      const commandId = (document.activeElement as HTMLElement | null)?.dataset
        .commandId;
      const command = request?.commands.find(
        (candidate) => candidate.id === commandId
      );
      if (command?.children?.length) {
        event.preventDefault();
        setExpandedCommandId(command.id);
        requestAnimationFrame(() => {
          menu
            .querySelector<HTMLButtonElement>(
              `[data-parent-command-id="${command.id}"]`
            )
            ?.focus();
        });
      }
      return;
    }
    if (event.key === 'ArrowLeft' && expandedCommandId) {
      event.preventDefault();
      const parent = menu.querySelector<HTMLButtonElement>(
        `[data-command-id="${expandedCommandId}"]`
      );
      setExpandedCommandId(null);
      requestAnimationFrame(() => parent?.focus());
    }
  }

  let previousGroup: string | undefined;
  const menu = (
    <div
      ref={menuRef}
      className="editor-context-menu"
      role="menu"
      aria-label={request.label}
      style={position}
      onKeyDown={handleKeyDown}
      onContextMenu={(event) => event.preventDefault()}
    >
      <div className="editor-context-menu-label">{request.label}</div>
      {request.commands.map((command) => {
        const separator =
          previousGroup !== undefined && command.group !== previousGroup;
        previousGroup = command.group;
        return (
          <div key={command.id} className={separator ? 'menu-group separated' : 'menu-group'}>
            <button
              type="button"
              role={command.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
              aria-checked={command.checked}
              aria-disabled={Boolean(command.disabledReason)}
              aria-haspopup={command.children?.length ? 'menu' : undefined}
              aria-expanded={
                command.children?.length
                  ? expandedCommandId === command.id
                  : undefined
              }
              data-command-id={command.id}
              className={[
                'editor-button',
                command.dangerous ? 'editor-button--danger' : undefined
              ].filter(Boolean).join(' ')}
              title={command.disabledReason}
              onClick={() => runCommand(command)}
            >
              <span className="menu-check" aria-hidden="true">
                {command.checked === undefined ? '' : command.checked ? '✓' : ''}
              </span>
              {command.swatchColor ? (
                <span
                  className="editor-context-menu-swatch"
                  style={{ backgroundColor: command.swatchColor }}
                  aria-hidden="true"
                />
              ) : null}
              <span>{command.label}</span>
              {command.children?.length && (
                <span className="menu-tree-arrow" aria-hidden="true">
                  {expandedCommandId === command.id ? '▾' : '▸'}
                </span>
              )}
              {command.disabledReason && <small className="editor-text editor-text--caption">{command.disabledReason}</small>}
            </button>
            {command.children?.length &&
              expandedCommandId === command.id && (
                <div
                  className="editor-context-menu-children"
                  role="group"
                  aria-label={`${command.label} nodes`}
                >
                  {command.children.map((child) => (
                    <button
                      type="button"
                      role={child.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
                      aria-checked={child.checked}
                      key={child.id}
                      aria-disabled={Boolean(child.disabledReason)}
                      data-parent-command-id={command.id}
                      className={[
                        'editor-button',
                        child.dangerous ? 'editor-button--danger' : undefined
                      ].filter(Boolean).join(' ')}
                      title={child.disabledReason}
                      onClick={() => runCommand(child)}
                    >
                      <span className="menu-check" aria-hidden="true">
                        {child.checked === undefined ? '' : child.checked ? '✓' : ''}
                      </span>
                      {child.swatchColor ? (
                        <span
                          className="editor-context-menu-swatch"
                          style={{ backgroundColor: child.swatchColor }}
                          aria-hidden="true"
                        />
                      ) : null}
                      <span>{child.label}</span>
                      {child.disabledReason && (
                        <small className="editor-text editor-text--caption">{child.disabledReason}</small>
                      )}
                    </button>
                  ))}
                </div>
              )}
          </div>
        );
      })}
    </div>
  );
  return createPortal(
    menu,
    document.querySelector<HTMLElement>('.editor-shell') ?? document.body
  );
}

export function ContextMenuButton({
  label,
  onOpen,
  className = ''
}: {
  label: string;
  onOpen: (event: ReactMouseEvent<HTMLButtonElement>) => void;
  className?: string;
}) {
  return (
    <button
      type="button"
      className={['editor-button', (`context-menu-button ${className}`.trim())].filter(Boolean).join(' ')}
      aria-label="More actions"
      aria-description={label}
      title={label}
      onClick={onOpen}
    >
      <span aria-hidden="true">…</span>
    </button>
  );
}

type DecisionCallback = () => unknown | Promise<unknown>;
function assignDecisionRef<T>(ref: Ref<T> | undefined, value: T | null) {
  if (typeof ref === 'function') return ref(value);
  if (ref) ref.current = value;
}

function useDecisionOwner(options: {
  ownerId?: string; title: string; ref: { current: HTMLElement | null };
  value?: string; choices?: Array<{ value: string; label: string }>;
  setValue?: (value: string) => void;
  confirm: (value?: string) => unknown | Promise<unknown>;
  cancel: DecisionCallback;
}) {
  const generatedId = useId();
  const busyRef = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  async function invoke(callback: DecisionCallback) {
    if (busyRef.current) throw new Error('This editor decision is already being resolved.');
    const surface = options.ref.current;
    busyRef.current = true; setBusy(true); setError(null);
    try {
      await callback();
      // This is a decision-delivery receipt, not an authored commit claim.
      return { callbackCompleted: true, closed: !surface?.isConnected };
    } catch (failure) {
      console.error(failure);
      setError(failure instanceof Error ? failure : new Error(String(failure)));
      throw failure;
    } finally { busyRef.current = false; setBusy(false); }
  }
  useEditorOperationOwner({ id: options.ownerId ?? `decision.${generatedId}`, label: options.title,
    isCurrent: () => isCurrentEditorDialog(options.ref.current),
    describe: () => ({ title: options.title, busy, value: options.value?.slice(0, 10000), choiceCount: options.choices?.length }),
    operations: [
      { id: 'inspect', description: `Inspect the current ${options.title} decision and bounded available choices.`, kind: 'read', schema: z.strictObject({}), execute(_args, guard) {
        guard.assertCurrent(); return { title: options.title, busy, value: options.value?.slice(0, 10000), valueTruncated: (options.value?.length ?? 0) > 10000,
          options: options.choices?.slice(0, 100), choiceCount: options.choices?.length, optionsTruncated: (options.choices?.length ?? 0) > 100,
          error: error ? { message: error.message, stack: error.stack } : null };
      } },
      ...(options.setValue ? [{ id: 'set_value', description: 'Set this decision draft field without submitting it.', kind: 'view' as const, schema: z.strictObject({ value: z.string() }), execute(raw: unknown, guard: import('./EditorOperationOwners').EditorExecutionGuard) {
        guard.assertCurrent(); const value = (raw as { value: string }).value;
        if (busyRef.current) throw new Error('This editor decision is already being resolved.');
        if (options.choices && !options.choices.some(choice => choice.value === value)) throw new Error('The requested value is not an available choice.');
        flushSync(() => options.setValue!(value)); return { valueUpdated: value !== options.value };
      } }] : []),
      { id: 'confirm', description: `Deliver the current ${options.title} decision to its authoritative callback and await any returned work. A receipt acknowledges callback completion/closure; it does not fabricate a project commit.`, kind: 'side-effect', releasesOwner: true,
        schema: options.setValue ? z.strictObject({ value: z.string() }) : z.strictObject({}), execute(raw, guard) {
          guard.assertCurrent(); return invoke(() => options.confirm(options.setValue ? (raw as { value: string }).value : undefined));
        } },
      { id: 'cancel', description: 'Cancel this exact current decision using its authoritative callback.', kind: 'view', releasesOwner: true, schema: z.strictObject({}), execute(_args, guard) { guard.assertCurrent(); return invoke(options.cancel); } }
    ]
  });
  return { busy, error, confirm: (value?: string) => invoke(() => options.confirm(value)), cancel: () => invoke(options.cancel) };
}

export function EditorPromptDialog({
  title,
  label,
  value,
  confirmLabel = 'Apply',
  onConfirm,
  onCancel,
  dialogRef,
  ownerId
}: {
  ownerId?: string;
  title: string;
  label: string;
  value: string;
  confirmLabel?: string;
  onConfirm: (value: string) => unknown | Promise<unknown>;
  onCancel: () => unknown | Promise<unknown>;
  /** Actual portaled surface, for mounted owner lifetime checks. */
  dialogRef?: Ref<HTMLFormElement>;
}) {
  const [draft, setDraft] = useState(value);
  const ownRef = useRef<HTMLFormElement>(null);
  const decision = useDecisionOwner({ ownerId, title, ref: ownRef, value: draft, setValue: setDraft, cancel: onCancel,
    confirm(input = draft) { if (!input.trim()) throw new Error('Enter a nonempty value before submitting this prompt.'); return onConfirm(input.trim()); }
  });
  return (
    <EditorDialogShell backdropClassName="editor-dialog-backdrop" onClose={() => { void decision.cancel().catch(() => undefined); }}>
      <form
        ref={element => { ownRef.current = element; return assignDecisionRef(dialogRef, element); }}
        className="editor-small-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="editor-prompt-title"
        onSubmit={(event) => {
          event.preventDefault();
          if (draft.trim()) void decision.confirm(draft).catch(() => undefined);
        }}
      >
        <h2 className="editor-heading editor-heading--page" id="editor-prompt-title">{title}</h2>
        <label className="editor-field-label">
          <span>{label}</span>
          <input className="editor-control editor-control--text"
            autoFocus
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
        </label>
        {decision.error && <pre className="editor-text editor-text--error" role="alert">{decision.error.stack ?? decision.error.message}</pre>}
        <footer>
          <button className="editor-button" type="button" disabled={decision.busy} onClick={() => { void decision.cancel().catch(() => undefined); }}>Cancel</button>
          <button className="editor-button" type="submit" disabled={decision.busy || !draft.trim()}>{confirmLabel}</button>
        </footer>
      </form>
    </EditorDialogShell>
  );
}

export function EditorChoiceDialog({
  title,
  label,
  value,
  options,
  confirmLabel = 'Apply',
  onConfirm,
  onCancel,
  ownerId,
  dialogRef
}: {
  ownerId?: string;
  dialogRef?: Ref<HTMLFormElement>;
  title: string;
  label: string;
  value: string;
  options: Array<{ value: string; label: string }>;
  confirmLabel?: string;
  onConfirm: (value: string) => unknown | Promise<unknown>;
  onCancel: () => unknown | Promise<unknown>;
}) {
  const [draft, setDraft] = useState(value);
  const ownRef = useRef<HTMLFormElement>(null);
  const decision = useDecisionOwner({ ownerId, title, ref: ownRef, value: draft, choices: options, setValue: setDraft, cancel: onCancel,
    confirm(input = draft) { if (!options.some(option => option.value === input)) throw new Error('The requested value is not an available choice.'); return onConfirm(input); }
  });
  return (
    <EditorDialogShell backdropClassName="editor-dialog-backdrop" onClose={() => { void decision.cancel().catch(() => undefined); }}>
      <form
        ref={element => { ownRef.current = element; return assignDecisionRef(dialogRef, element); }}
        className="editor-small-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="editor-choice-title"
        onSubmit={(event) => {
          event.preventDefault();
          void decision.confirm(draft).catch(() => undefined);
        }}
      >
        <h2 className="editor-heading editor-heading--page" id="editor-choice-title">{title}</h2>
        <label className="editor-field-label">
          <span>{label}</span>
          <select className="editor-control editor-control--select"
            autoFocus
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          >
            {options.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
        </label>
        {decision.error && <pre className="editor-text editor-text--error" role="alert">{decision.error.stack ?? decision.error.message}</pre>}
        <footer>
          <button className="editor-button" type="button" disabled={decision.busy} onClick={() => { void decision.cancel().catch(() => undefined); }}>Cancel</button>
          <button className="editor-button" type="submit" disabled={decision.busy || !options.some(option => option.value === draft)}>{confirmLabel}</button>
        </footer>
      </form>
    </EditorDialogShell>
  );
}

export function EditorConfirmDialog({
  title,
  message,
  confirmLabel,
  dangerous = false,
  onConfirm,
  onCancel,
  dialogRef,
  ownerId
}: {
  ownerId?: string;
  title: string;
  message: string;
  confirmLabel: string;
  dangerous?: boolean;
  onConfirm: DecisionCallback;
  onCancel: () => unknown | Promise<unknown>;
  /** Actual portaled surface, for mounted owner lifetime checks. */
  dialogRef?: Ref<HTMLElement>;
}) {
  const ownRef = useRef<HTMLElement>(null);
  const decision = useDecisionOwner({ ownerId, title, ref: ownRef, cancel: onCancel, confirm: onConfirm });
  return (
    <EditorDialogShell backdropClassName="editor-dialog-backdrop" onClose={() => { void decision.cancel().catch(() => undefined); }}>
      <section
        ref={element => { ownRef.current = element; return assignDecisionRef(dialogRef, element); }}
        className="editor-small-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="editor-confirm-title"
        aria-describedby="editor-confirm-message"
      >
        <h2 className="editor-heading editor-heading--page" id="editor-confirm-title">{title}</h2>
        <p className="editor-text" id="editor-confirm-message">{message}</p>
        {decision.error && <pre className="editor-text editor-text--error" role="alert">{decision.error.stack ?? decision.error.message}</pre>}
        <footer>
          <button className="editor-button" type="button" autoFocus disabled={decision.busy} onClick={() => { void decision.cancel().catch(() => undefined); }}>Cancel</button>
          <button
            type="button"
            className={[
              'editor-button',
              dangerous ? 'editor-button--danger' : undefined
            ].filter(Boolean).join(' ')}
            disabled={decision.busy}
            onClick={() => { void decision.confirm().catch(() => undefined); }}
          >
            {confirmLabel}
          </button>
        </footer>
      </section>
    </EditorDialogShell>
  );
}
