// Copied from rpg_creator src/editor/EditorPreferenceStore.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import { createContext, useContext, type ReactNode } from 'react';
import { WORKSPACE_LAYOUT_STORAGE_KEY } from './workspaceLayout';

export interface EditorPreferenceStorage extends Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> {
  readonly writable: boolean;
}

/** Studio's host preference scope contains only the dock layout. */
export class EditorPreferenceStore {
  readonly hostStorage: EditorPreferenceStorage;

  constructor(options: { storage?: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> } = {}) {
    const storage = options.storage ?? window.localStorage;
    const assertKey = (key: string) => {
      if (key !== WORKSPACE_LAYOUT_STORAGE_KEY) throw new Error(`Unknown studio preference: ${key}.`);
    };
    this.hostStorage = {
      writable: true,
      getItem(key) { assertKey(key); return storage.getItem(key); },
      setItem(key, value) { assertKey(key); storage.setItem(key, value); },
      removeItem(key) { assertKey(key); storage.removeItem(key); }
    };
  }

  reportError(error: unknown): never {
    throw error;
  }
}

const EditorPreferenceContext = createContext<EditorPreferenceStore | null>(null);

export function EditorPreferenceProvider({ store, children }: {
  store: EditorPreferenceStore;
  children: ReactNode;
}) {
  return <EditorPreferenceContext.Provider value={store}>{children}</EditorPreferenceContext.Provider>;
}

export function useEditorPreferenceStore(): EditorPreferenceStore {
  const value = useContext(EditorPreferenceContext);
  if (!value) throw new Error('EditorPreferenceProvider must be mounted before editor preference consumers.');
  return value;
}
