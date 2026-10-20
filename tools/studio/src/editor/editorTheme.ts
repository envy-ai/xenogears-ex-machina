// Copied from rpg_creator src/editor/editorTheme.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import { readStoredJson } from './storedJson';

export const EDITOR_THEME_STORAGE_KEY = 'wayfarer-forge.editor-theme.v1';

export const EDITOR_THEMES = [
  { id: 'solarized-dark', label: 'Solarized Dark', density: 'compact' },
  {
    id: 'dark-slate-pastel',
    label: 'Dark Slate Pastel',
    density: 'compact'
  },
  { id: 'graphite-teal', label: 'Graphite Teal', density: 'compact' },
  { id: 'wayfarer-dark', label: 'Saga Dark', density: 'comfortable' }
] as const;

export type EditorThemeId = (typeof EDITOR_THEMES)[number]['id'];
export type EditorThemeDensity =
  (typeof EDITOR_THEMES)[number]['density'];

export const DEFAULT_EDITOR_THEME: EditorThemeId = 'graphite-teal';

export function parseEditorTheme(value: unknown): EditorThemeId {
  if (!EDITOR_THEMES.some((theme) => theme.id === value)) {
    throw new TypeError(`Unknown editor theme: ${String(value)}.`);
  }
  return value as EditorThemeId;
}

export function editorThemeDensity(
  themeId: EditorThemeId
): EditorThemeDensity {
  return EDITOR_THEMES.find((theme) => theme.id === themeId)?.density ?? 'compact';
}

export function readEditorTheme(
  storage: Pick<Storage, 'getItem'>
): EditorThemeId {
  return readStoredJson(
    EDITOR_THEME_STORAGE_KEY,
    parseEditorTheme,
    DEFAULT_EDITOR_THEME,
    storage,
    { rawString: true }
  );
}

export function saveEditorTheme(
  storage: Pick<Storage, 'setItem'>,
  theme: EditorThemeId
) {
  storage.setItem(EDITOR_THEME_STORAGE_KEY, theme);
}
