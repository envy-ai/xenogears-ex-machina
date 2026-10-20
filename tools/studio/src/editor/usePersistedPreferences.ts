// Copied from rpg_creator src/editor/usePersistedPreferences.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import { useEffect, useRef, type Dispatch, type SetStateAction } from 'react';
import { useEditorPreferenceStore } from './EditorPreferenceStore';

/**
 * Persists host preferences to the editor file service whenever they change and, when a clamp
 * is provided, re-clamps them against the current viewport on window resize.
 */
export function usePersistedPreferences<T>(
  storageKey: string,
  preferences: T,
  setPreferences: Dispatch<SetStateAction<T>>,
  clamp?: (current: T) => T
): void {
  const { hostStorage } = useEditorPreferenceStore();
  const initialized = useRef(false);
  useEffect(() => {
    if (!initialized.current) {
      initialized.current = true;
      return;
    }
    hostStorage.setItem(storageKey, JSON.stringify(preferences));
  }, [hostStorage, storageKey, preferences]);

  const clampRef = useRef(clamp);
  useEffect(() => {
    clampRef.current = clamp;
  });

  useEffect(() => {
    const onResize = () => {
      const clampCurrent = clampRef.current;
      if (clampCurrent) {
        setPreferences((current) => clampCurrent(current));
      }
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [setPreferences]);
}
