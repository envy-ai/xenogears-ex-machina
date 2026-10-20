import { useMemo } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { EditorPreferenceProvider, EditorPreferenceStore } from './editor/EditorPreferenceStore';
import { EditorRenderErrorBoundary } from './editor/EditorRenderErrorBoundary';
import { EditorOperationOwnerRegistry, EditorOperationOwnersProvider } from './editor/EditorOperationOwners';
import './styles/editor.scss';
import './studio.scss';

function StudioRoot() {
  const store = useMemo(() => new EditorPreferenceStore(), []);
  const registry = useMemo(() => new EditorOperationOwnerRegistry(), []);
  return <EditorPreferenceProvider store={store}><EditorOperationOwnersProvider registry={registry}><App /></EditorOperationOwnersProvider></EditorPreferenceProvider>;
}
const element = document.getElementById('root');
if (!element) throw new Error('Missing studio root');
createRoot(element).render(<EditorRenderErrorBoundary fallback={error => <main className="editor-shell studio-shell" data-editor-theme="graphite-teal"><section role="alert" className="studio-errors"><h1 className="editor-heading editor-heading--page">Studio failed to render</h1><pre className="editor-text editor-text--danger">{error instanceof Error ? error.stack ?? error.message : String(error)}</pre><button className="editor-button" onClick={() => window.location.reload()}>Reload studio</button></section></main>}><StudioRoot /></EditorRenderErrorBoundary>);
