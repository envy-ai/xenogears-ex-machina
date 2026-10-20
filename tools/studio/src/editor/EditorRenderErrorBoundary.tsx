// Copied from rpg_creator src/editor/EditorRenderErrorBoundary.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import { Component, type ReactNode } from 'react';

type Props = {
  children: ReactNode;
  fallback: (error: unknown) => ReactNode;
};

type State = { failed: boolean; error: unknown };

/** Keeps the editor's recovery controls mounted when its project view cannot render. */
export class EditorRenderErrorBoundary extends Component<Props, State> {
  state: State = { failed: false, error: null };

  static getDerivedStateFromError(error: unknown): State {
    return { failed: true, error };
  }

  componentDidCatch(error: unknown) {
    console.error('The editor project view failed to render.', error);
  }

  render() {
    return this.state.failed
      ? this.props.fallback(this.state.error)
      : this.props.children;
  }
}
